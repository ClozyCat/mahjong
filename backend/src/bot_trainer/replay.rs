use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::botzone::{BotZoneAction, BotZoneMatch, BotZoneResult};
use crate::bot::action_space::{TILE_KIND_COUNT, tile_index};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DecisionKind {
    ActiveTurn,
    ClaimWindow,
    RobKong,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum TrainingLabel {
    Discard { tile_key: String },
    ClaimChow { middle_tile_key: String },
    ClaimPung { tile_key: String },
    ClaimKong { tile_key: String },
    SelfKong { kind: String, tile_key: String },
    Hu,
    Pass,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TrainingDecisionSampleV2 {
    pub(crate) schema_version: u32,
    pub(crate) match_id: String,
    pub(crate) decision_index: u64,
    pub(crate) seat_index: usize,
    pub(crate) decision_kind: DecisionKind,
    pub(crate) context: SerializableBotContext,
    pub(crate) legal_actions: Vec<String>,
    pub(crate) label: TrainingLabel,
    pub(crate) outcome: SampleOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SampleOutcome {
    pub(crate) score_delta: i64,
    pub(crate) won: bool,
    pub(crate) dealt_in: bool,
    pub(crate) round_drawn: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SerializableBotContext {
    pub(crate) seat_index: usize,
    pub(crate) seat_count: usize,
    pub(crate) dealer_seat: usize,
    pub(crate) round_wind: String,
    pub(crate) cumulative_scores: Vec<i64>,
    pub(crate) wall_tiles_remaining: i64,
    pub(crate) visible_tile_keys: Vec<String>,
    pub(crate) opponent_discards_by_seat: Vec<Vec<String>>,
    pub(crate) opponent_melds_by_seat: Vec<Vec<Vec<String>>>,
    pub(crate) player: SerializableBotPlayer,
    pub(crate) restricted_discard_tile_key: Option<String>,
    pub(crate) drawn_tile_id: Option<String>,
    pub(crate) self_kong_candidates: Vec<SerializableSelfKongCandidate>,
    pub(crate) claim_options: Vec<SerializableClaimOption>,
    pub(crate) last_discard_tile_key: Option<String>,
    pub(crate) add_kong_risk_tiles: HashSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SerializableBotPlayer {
    pub(crate) concealed_tiles: Vec<SerializableBotTile>,
    pub(crate) concealed_tile_counts: Vec<u8>,
    pub(crate) meld_tile_key_groups: Vec<Vec<String>>,
    pub(crate) flower_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SerializableBotTile {
    pub(crate) tile_id: String,
    pub(crate) tile_key: String,
    pub(crate) is_flower: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SerializableClaimOption {
    pub(crate) action_type: String,
    pub(crate) tile_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SerializableSelfKongCandidate {
    pub(crate) kind: String,
    pub(crate) tile_ids: Vec<String>,
    pub(crate) tile_key: String,
    pub(crate) meld_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReplayError {
    message: String,
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ReplayError {}

pub(crate) fn replay_match_to_samples(
    record: &BotZoneMatch,
) -> Result<Vec<TrainingDecisionSampleV2>, ReplayError> {
    let mut state = ReplayState::new(record);
    let mut samples = Vec::new();
    let mut decision_index = 0_u64;
    let outcome_by_seat = outcome_by_seat(record);

    for (event_index, event) in record.events.iter().enumerate() {
        match &event.action {
            BotZoneAction::Draw { tile_key } => {
                state.add_tile(event.actor, tile_key);
            }
            BotZoneAction::Play { tile_key } => {
                let outcome = outcome_by_seat[event.actor].clone();
                samples.push(state.active_turn_sample(
                    record,
                    &mut decision_index,
                    event.actor,
                    TrainingLabel::Discard {
                        tile_key: tile_key.clone(),
                    },
                    outcome,
                ));
                state.remove_one_tile(event.actor, tile_key);
                state.discards[event.actor].push(tile_key.clone());
                state.visible_tile_keys.push(tile_key.clone());
                state.last_discard_tile_key = Some(tile_key.clone());

                let declared_claims = declared_claims_after_play(record, event_index);
                samples.extend(state.claim_samples(
                    record,
                    &mut decision_index,
                    event.actor,
                    tile_key,
                    &declared_claims,
                    &outcome_by_seat,
                ));
            }
            BotZoneAction::Chi { middle_tile_key } => {
                state.apply_chow(event.actor, middle_tile_key);
            }
            BotZoneAction::Peng { tile_key } => {
                state.apply_same_tile_meld(event.actor, tile_key, 2);
            }
            BotZoneAction::Gang { tile_key } => {
                state.apply_same_tile_meld(event.actor, tile_key, 3);
            }
            BotZoneAction::AnGang { tile_key } => {
                state.apply_same_tile_meld(event.actor, tile_key, 4);
            }
            BotZoneAction::BuGang { tile_key } => {
                state.apply_add_kong(event.actor, tile_key);
            }
            BotZoneAction::Hu { .. } => {}
        }
    }

    Ok(samples)
}

#[derive(Clone)]
struct DeclaredClaim {
    seat_index: usize,
    label: TrainingLabel,
}

struct ReplayState {
    hands: [Vec<SerializableBotTile>; 4],
    discards: [Vec<String>; 4],
    melds: [Vec<Vec<String>>; 4],
    visible_tile_keys: Vec<String>,
    last_discard_tile_key: Option<String>,
    next_sequence: usize,
}

impl ReplayState {
    fn new(record: &BotZoneMatch) -> Self {
        let mut next_sequence = 0_usize;
        let hands = std::array::from_fn(|seat| {
            record.deals[seat]
                .iter()
                .map(|tile_key| {
                    let tile = make_tile(&record.match_id, seat, next_sequence, tile_key);
                    next_sequence += 1;
                    tile
                })
                .collect()
        });
        Self {
            hands,
            discards: std::array::from_fn(|_| Vec::new()),
            melds: std::array::from_fn(|_| Vec::new()),
            visible_tile_keys: Vec::new(),
            last_discard_tile_key: None,
            next_sequence,
        }
    }

    fn add_tile(&mut self, seat: usize, tile_key: &str) {
        let tile = make_tile("draw", seat, self.next_sequence, tile_key);
        self.next_sequence += 1;
        self.hands[seat].push(tile);
    }

    fn remove_one_tile(&mut self, seat: usize, tile_key: &str) -> Option<SerializableBotTile> {
        let index = self.hands[seat]
            .iter()
            .position(|tile| tile.tile_key == tile_key)?;
        Some(self.hands[seat].remove(index))
    }

    fn active_turn_sample(
        &self,
        record: &BotZoneMatch,
        decision_index: &mut u64,
        seat_index: usize,
        label: TrainingLabel,
        outcome: SampleOutcome,
    ) -> TrainingDecisionSampleV2 {
        let context = self.context(record, seat_index, Vec::new());
        let legal_actions = legal_discard_actions(&context);
        let sample = TrainingDecisionSampleV2 {
            schema_version: 2,
            match_id: record.match_id.clone(),
            decision_index: *decision_index,
            seat_index,
            decision_kind: DecisionKind::ActiveTurn,
            context,
            legal_actions,
            label,
            outcome,
        };
        *decision_index += 1;
        sample
    }

    fn claim_samples(
        &self,
        record: &BotZoneMatch,
        decision_index: &mut u64,
        discarder_seat: usize,
        discarded_tile_key: &str,
        declared_claims: &[DeclaredClaim],
        outcome_by_seat: &[SampleOutcome; 4],
    ) -> Vec<TrainingDecisionSampleV2> {
        let mut samples = Vec::new();
        for seat_index in 0..4 {
            if seat_index == discarder_seat {
                continue;
            }
            let claim_options = self.claim_options(seat_index, discarder_seat, discarded_tile_key);
            if claim_options.is_empty() {
                continue;
            }
            let label = declared_claims
                .iter()
                .find(|claim| claim.seat_index == seat_index)
                .map(|claim| claim.label.clone())
                .unwrap_or(TrainingLabel::Pass);
            let context = self.context(record, seat_index, claim_options);
            let legal_actions = claim_legal_actions(&context);
            samples.push(TrainingDecisionSampleV2 {
                schema_version: 2,
                match_id: record.match_id.clone(),
                decision_index: *decision_index,
                seat_index,
                decision_kind: DecisionKind::ClaimWindow,
                context,
                legal_actions,
                label,
                outcome: outcome_by_seat[seat_index].clone(),
            });
            *decision_index += 1;
        }
        samples
    }

    fn context(
        &self,
        record: &BotZoneMatch,
        seat_index: usize,
        claim_options: Vec<SerializableClaimOption>,
    ) -> SerializableBotContext {
        SerializableBotContext {
            seat_index,
            seat_count: 4,
            dealer_seat: 0,
            round_wind: record.round_wind.clone(),
            cumulative_scores: vec![0, 0, 0, 0],
            wall_tiles_remaining: (83_i64 - self.visible_tile_keys.len() as i64).max(0),
            visible_tile_keys: self.visible_tile_keys.clone(),
            opponent_discards_by_seat: self.discards.iter().cloned().collect(),
            opponent_melds_by_seat: self.melds.iter().cloned().collect(),
            player: SerializableBotPlayer {
                concealed_tiles: self.hands[seat_index].clone(),
                concealed_tile_counts: tile_counts(&self.hands[seat_index]).to_vec(),
                meld_tile_key_groups: self.melds[seat_index].clone(),
                flower_count: 0,
            },
            restricted_discard_tile_key: None,
            drawn_tile_id: self.hands[seat_index]
                .last()
                .map(|tile| tile.tile_id.clone()),
            self_kong_candidates: Vec::new(),
            claim_options,
            last_discard_tile_key: self.last_discard_tile_key.clone(),
            add_kong_risk_tiles: HashSet::new(),
        }
    }

    fn claim_options(
        &self,
        seat_index: usize,
        discarder_seat: usize,
        discarded_tile_key: &str,
    ) -> Vec<SerializableClaimOption> {
        let counts = tile_counts(&self.hands[seat_index]);
        let same_count = tile_index(discarded_tile_key)
            .map(|index| counts[index])
            .unwrap_or(0);
        let mut options = Vec::new();
        if same_count >= 2 {
            options.push(SerializableClaimOption {
                action_type: "pung".to_string(),
                tile_ids: take_tile_ids(&self.hands[seat_index], discarded_tile_key, 2),
            });
        }
        if same_count >= 3 {
            options.push(SerializableClaimOption {
                action_type: "kong".to_string(),
                tile_ids: take_tile_ids(&self.hands[seat_index], discarded_tile_key, 3),
            });
        }
        if seat_index == (discarder_seat + 1) % 4 {
            if let Some(tile_ids) = chow_tile_ids(&self.hands[seat_index], discarded_tile_key) {
                options.push(SerializableClaimOption {
                    action_type: "chow".to_string(),
                    tile_ids,
                });
            }
        }
        options
    }

    fn apply_same_tile_meld(&mut self, seat: usize, tile_key: &str, count: usize) {
        for _ in 0..count {
            self.remove_one_tile(seat, tile_key);
        }
        self.melds[seat].push(vec![tile_key.to_string(); count + 1]);
    }

    fn apply_chow(&mut self, seat: usize, middle_tile_key: &str) {
        let Some(last_discard) = self.last_discard_tile_key.clone() else {
            return;
        };
        if let Some(tile_ids) = chow_tile_ids(&self.hands[seat], &last_discard) {
            for tile_id in tile_ids {
                if let Some(tile) = self.hands[seat]
                    .iter()
                    .find(|tile| tile.tile_id == tile_id)
                    .cloned()
                {
                    self.remove_one_tile(seat, &tile.tile_key);
                }
            }
        }
        self.melds[seat].push(chow_group(&last_discard, middle_tile_key));
    }

    fn apply_add_kong(&mut self, seat: usize, tile_key: &str) {
        self.remove_one_tile(seat, tile_key);
        if let Some(meld) = self.melds[seat]
            .iter_mut()
            .find(|meld| meld.len() == 3 && meld.iter().all(|key| key == tile_key))
        {
            meld.push(tile_key.to_string());
        }
    }
}

fn declared_claims_after_play(record: &BotZoneMatch, event_index: usize) -> Vec<DeclaredClaim> {
    let mut claims = record.events[event_index]
        .ignored_claims
        .iter()
        .filter_map(|claim| label_for_claim_action(&claim.action).map(|label| (claim.actor, label)))
        .map(|(seat_index, label)| DeclaredClaim { seat_index, label })
        .collect::<Vec<_>>();

    if let Some(next_event) = record.events.get(event_index + 1) {
        if let Some(label) = label_for_claim_action(&next_event.action) {
            claims.push(DeclaredClaim {
                seat_index: next_event.actor,
                label,
            });
        }
    }
    claims
}

fn label_for_claim_action(action: &BotZoneAction) -> Option<TrainingLabel> {
    match action {
        BotZoneAction::Chi { middle_tile_key } => Some(TrainingLabel::ClaimChow {
            middle_tile_key: middle_tile_key.clone(),
        }),
        BotZoneAction::Peng { tile_key } => Some(TrainingLabel::ClaimPung {
            tile_key: tile_key.clone(),
        }),
        BotZoneAction::Gang { tile_key } => Some(TrainingLabel::ClaimKong {
            tile_key: tile_key.clone(),
        }),
        BotZoneAction::Hu { .. } => Some(TrainingLabel::Hu),
        _ => None,
    }
}

fn legal_discard_actions(context: &SerializableBotContext) -> Vec<String> {
    let mut seen = HashSet::new();
    context
        .player
        .concealed_tiles
        .iter()
        .filter(|tile| !tile.is_flower)
        .filter(|tile| seen.insert(tile.tile_key.clone()))
        .map(|tile| format!("discard:{}", tile.tile_key))
        .collect()
}

fn claim_legal_actions(context: &SerializableBotContext) -> Vec<String> {
    let mut actions = vec!["pass".to_string()];
    let Some(last_discard) = context.last_discard_tile_key.as_deref() else {
        return actions;
    };
    for option in &context.claim_options {
        match option.action_type.as_str() {
            "chow" => actions.push(format!(
                "claim:chow:{}",
                chow_middle_key(last_discard, &option.tile_ids, context)
            )),
            "pung" => actions.push(format!("claim:pung:{last_discard}")),
            "kong" => actions.push(format!("claim:kong:{last_discard}")),
            "hu" => actions.push("claim:hu".to_string()),
            _ => {}
        }
    }
    actions.sort();
    actions.dedup();
    actions
}

fn outcome_by_seat(record: &BotZoneMatch) -> [SampleOutcome; 4] {
    let (score_delta, round_drawn) = match &record.result {
        BotZoneResult::Hu { score_delta, .. } => (*score_delta, false),
        BotZoneResult::Huang { score_delta } => (*score_delta, true),
    };
    let winner = record.events.iter().find_map(|event| match event.action {
        BotZoneAction::Hu { .. } => Some(event.actor),
        _ => None,
    });
    let discarder = record
        .events
        .iter()
        .rev()
        .find_map(|event| match event.action {
            BotZoneAction::Play { .. } => Some(event.actor),
            _ => None,
        });
    std::array::from_fn(|seat| SampleOutcome {
        score_delta: score_delta[seat],
        won: winner == Some(seat),
        dealt_in: winner.is_some() && discarder == Some(seat) && winner != Some(seat),
        round_drawn,
    })
}

fn make_tile(match_id: &str, seat: usize, sequence: usize, tile_key: &str) -> SerializableBotTile {
    SerializableBotTile {
        tile_id: format!("{match_id}:s{seat}:{tile_key}:{sequence}"),
        tile_key: tile_key.to_string(),
        is_flower: false,
    }
}

fn tile_counts(tiles: &[SerializableBotTile]) -> [u8; TILE_KIND_COUNT] {
    let mut counts = [0_u8; TILE_KIND_COUNT];
    for tile in tiles {
        if let Some(index) = tile_index(&tile.tile_key) {
            counts[index] = counts[index].saturating_add(1);
        }
    }
    counts
}

fn take_tile_ids(tiles: &[SerializableBotTile], tile_key: &str, count: usize) -> Vec<String> {
    tiles
        .iter()
        .filter(|tile| tile.tile_key == tile_key)
        .map(|tile| tile.tile_id.clone())
        .take(count)
        .collect()
}

fn chow_tile_ids(tiles: &[SerializableBotTile], discarded_tile_key: &str) -> Option<Vec<String>> {
    let discard_index = tile_index(discarded_tile_key)?;
    for (left, right) in chow_required_tile_pairs(discard_index) {
        let left_key = tile_key_for_index(left);
        let right_key = tile_key_for_index(right);
        let left_id = tiles
            .iter()
            .find(|tile| tile.tile_key == left_key)
            .map(|tile| tile.tile_id.clone());
        let right_id = tiles
            .iter()
            .find(|tile| tile.tile_key == right_key)
            .map(|tile| tile.tile_id.clone());
        if let (Some(left_id), Some(right_id)) = (left_id, right_id) {
            return Some(vec![left_id, right_id]);
        }
    }
    None
}

fn chow_middle_key(
    discarded_tile_key: &str,
    tile_ids: &[String],
    context: &SerializableBotContext,
) -> String {
    let mut keys = vec![discarded_tile_key.to_string()];
    for tile_id in tile_ids {
        if let Some(tile) = context
            .player
            .concealed_tiles
            .iter()
            .find(|tile| &tile.tile_id == tile_id)
        {
            keys.push(tile.tile_key.clone());
        }
    }
    keys.sort_by_key(|key| tile_index(key).unwrap_or(usize::MAX));
    keys.get(1)
        .cloned()
        .unwrap_or_else(|| discarded_tile_key.to_string())
}

fn chow_group(discarded_tile_key: &str, middle_tile_key: &str) -> Vec<String> {
    let Some(middle_index) = tile_index(middle_tile_key) else {
        return vec![discarded_tile_key.to_string()];
    };
    vec![
        tile_key_for_index(middle_index.saturating_sub(1)).to_string(),
        tile_key_for_index(middle_index).to_string(),
        tile_key_for_index(middle_index + 1).to_string(),
    ]
}

fn chow_required_tile_pairs(tile_index: usize) -> Vec<(usize, usize)> {
    if tile_index >= 27 {
        return Vec::new();
    }
    let rank = tile_index % 9 + 1;
    let mut pairs = Vec::new();
    if rank >= 3 {
        pairs.push((tile_index - 2, tile_index - 1));
    }
    if (2..=8).contains(&rank) {
        pairs.push((tile_index - 1, tile_index + 1));
    }
    if rank <= 7 {
        pairs.push((tile_index + 1, tile_index + 2));
    }
    pairs
}

fn tile_key_for_index(index: usize) -> &'static str {
    crate::bot::action_space::TILE_KEYS
        .get(index)
        .copied()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot_trainer::botzone::parse_match;

    const SIMPLE_MATCH: &str = r#"
Match fixture
Wind F1
Player 0 Deal T6 W1 W2
Player 1 Deal T4 T5 B1
Player 2 Deal J1 J1 B2
Player 3 Deal W2 W3 B6
Player 0 Draw B1
Player 0 Play T6
Score -8 8 0 0
"#;

    const IGNORE_CLAIM_MATCH: &str = r#"
Match ignore
Player 0 Deal B6 B6 W1
Player 1 Deal B6 W2 W3
Player 2 Deal T1 T2 T3
Player 3 Deal W4 W5 W6
Player 1 Draw T9
Player 1 Play B6 Ignore Player 0 PENG B6
Score 0 0 0 0
"#;

    const PASS_CLAIM_MATCH: &str = r#"
Match pass
Player 0 Deal W1 W9 T9
Player 1 Deal B1 B2 B3
Player 2 Deal T1 T2 T3
Player 3 Deal W2 W3 B6
Player 2 Draw T9
Player 2 Play W1
Score 0 0 0 0
"#;

    #[test]
    fn replay_emits_active_turn_discard_sample_before_play_event() {
        let record = parse_match(SIMPLE_MATCH).expect("match");
        let samples = replay_match_to_samples(&record).expect("samples");

        let first_discard = samples
            .iter()
            .find(|sample| sample.decision_kind == DecisionKind::ActiveTurn)
            .expect("active turn sample");

        assert_eq!(first_discard.seat_index, 0);
        assert_eq!(
            first_discard.label,
            TrainingLabel::Discard {
                tile_key: "t6".to_string()
            }
        );
        assert!(
            first_discard
                .legal_actions
                .iter()
                .any(|action| action == "discard:t6")
        );
    }

    #[test]
    fn replay_treats_ignore_claim_as_positive_label() {
        let record = parse_match(IGNORE_CLAIM_MATCH).expect("match");
        let samples = replay_match_to_samples(&record).expect("samples");

        let ignored_claim = samples
            .iter()
            .find(|sample| {
                sample.seat_index == 0
                    && sample.label
                        == TrainingLabel::ClaimPung {
                            tile_key: "b6".to_string(),
                        }
            })
            .expect("ignored pung sample");

        assert_eq!(ignored_claim.decision_kind, DecisionKind::ClaimWindow);
        assert!(
            ignored_claim
                .legal_actions
                .iter()
                .any(|action| action == "claim:pung:b6")
        );
    }

    #[test]
    fn replay_emits_pass_for_unclaimed_legal_claim() {
        let record = parse_match(PASS_CLAIM_MATCH).expect("match");
        let samples = replay_match_to_samples(&record).expect("samples");

        let pass = samples
            .iter()
            .find(|sample| {
                sample.seat_index == 3
                    && sample
                        .legal_actions
                        .iter()
                        .any(|action| action.starts_with("claim:chow"))
                    && sample.label == TrainingLabel::Pass
            })
            .expect("legal chow pass sample");

        assert_eq!(pass.decision_kind, DecisionKind::ClaimWindow);
    }
}
