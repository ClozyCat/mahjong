use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::botzone::{BotZoneAction, BotZoneMatch, BotZoneResult};
use crate::bot::action_space::{TILE_KIND_COUNT, tile_index};
use crate::rules::scoring::{
    EvaluationInput, TimingFeatures, evaluate_fans, extract_hand_features,
};

const TOTAL_TILE_COUNT: i64 = 136;

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
    pub(crate) fan_count: i64,
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
    pub(crate) discard_history: Vec<SerializableDiscardEvent>,
    pub(crate) player: SerializableBotPlayer,
    pub(crate) restricted_discard_tile_key: Option<String>,
    pub(crate) drawn_tile_id: Option<String>,
    pub(crate) self_kong_candidates: Vec<SerializableSelfKongCandidate>,
    pub(crate) claim_options: Vec<SerializableClaimOption>,
    pub(crate) last_discard_tile_key: Option<String>,
    pub(crate) add_kong_risk_tiles: HashSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SerializableDiscardEvent {
    pub(crate) seat_index: usize,
    pub(crate) tile_key: String,
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
    let exact_fan = calculate_match_exact_fan(record);
    let outcome_by_seat = outcome_by_seat(record, exact_fan);

    for (event_index, event) in record.events.iter().enumerate() {
        match &event.action {
            BotZoneAction::Draw { tile_key } => {
                state.add_tile(event.actor, tile_key);
            }
            BotZoneAction::Play { tile_key } => {
                let outcome = outcome_by_seat[event.actor].clone();
                if state.has_self_kong_candidates(event.actor) {
                    samples.push(state.self_kong_sample(
                        record,
                        &mut decision_index,
                        event.actor,
                        TrainingLabel::Pass,
                        outcome.clone(),
                    ));
                }
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
                state.discard_history.push(SerializableDiscardEvent {
                    seat_index: event.actor,
                    tile_key: tile_key.clone(),
                });
                state.last_discard_tile_key = Some(tile_key.clone());
                state.last_discarder_seat = Some(event.actor);
                state.current_drawn_tile_ids[event.actor] = None;
                state.restricted_discard_tile_key = None;

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
                samples.push(state.self_kong_sample(
                    record,
                    &mut decision_index,
                    event.actor,
                    TrainingLabel::SelfKong {
                        kind: "concealed_kong".to_string(),
                        tile_key: tile_key.clone(),
                    },
                    outcome_by_seat[event.actor].clone(),
                ));
                state.apply_concealed_kong(event.actor, tile_key);
            }
            BotZoneAction::BuGang { tile_key } => {
                samples.push(state.self_kong_sample(
                    record,
                    &mut decision_index,
                    event.actor,
                    TrainingLabel::SelfKong {
                        kind: "add_kong".to_string(),
                        tile_key: tile_key.clone(),
                    },
                    outcome_by_seat[event.actor].clone(),
                ));
                let declared_rob_kongs = declared_rob_kongs_after_add_kong(record, event_index);
                samples.extend(state.rob_kong_samples(
                    record,
                    &mut decision_index,
                    event.actor,
                    tile_key,
                    &declared_rob_kongs,
                    &outcome_by_seat,
                ));
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
    match_id: String,
    hands: [Vec<SerializableBotTile>; 4],
    discards: [Vec<String>; 4],
    discard_history: Vec<SerializableDiscardEvent>,
    melds: [Vec<Vec<String>>; 4],
    wall_tiles_remaining: i64,
    last_discard_tile_key: Option<String>,
    last_discarder_seat: Option<usize>,
    current_drawn_tile_ids: [Option<String>; 4],
    restricted_discard_tile_key: Option<String>,
    next_sequence: usize,
}

impl ReplayState {
    fn new(record: &BotZoneMatch) -> Self {
        let mut next_sequence = 0_usize;
        let total_dealt = record
            .deals
            .iter()
            .map(|deal| deal.len() as i64)
            .sum::<i64>();
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
            match_id: record.match_id.clone(),
            hands,
            discards: std::array::from_fn(|_| Vec::new()),
            discard_history: Vec::new(),
            melds: std::array::from_fn(|_| Vec::new()),
            wall_tiles_remaining: (TOTAL_TILE_COUNT - total_dealt).max(0),
            last_discard_tile_key: None,
            last_discarder_seat: None,
            current_drawn_tile_ids: std::array::from_fn(|_| None),
            restricted_discard_tile_key: None,
            next_sequence,
        }
    }

    fn add_tile(&mut self, seat: usize, tile_key: &str) {
        let tile = make_tile(&self.match_id, seat, self.next_sequence, tile_key);
        let tile_id = tile.tile_id.clone();
        self.next_sequence += 1;
        self.wall_tiles_remaining = (self.wall_tiles_remaining - 1).max(0);
        self.hands[seat].push(tile);
        self.current_drawn_tile_ids[seat] = Some(tile_id);
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
        let context = self.context(
            record,
            seat_index,
            self.self_kong_candidates(seat_index),
            Vec::new(),
        );
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

    fn self_kong_sample(
        &self,
        record: &BotZoneMatch,
        decision_index: &mut u64,
        seat_index: usize,
        label: TrainingLabel,
        outcome: SampleOutcome,
    ) -> TrainingDecisionSampleV2 {
        let context = self.context(
            record,
            seat_index,
            self.self_kong_candidates(seat_index),
            Vec::new(),
        );
        let mut legal_actions = self_kong_legal_actions(&context);
        if let TrainingLabel::SelfKong { kind, tile_key } = &label {
            let action_id = format!("self_kong:{kind}:{tile_key}");
            if !legal_actions.iter().any(|action| action == &action_id) {
                legal_actions.push(action_id);
                legal_actions.sort();
                legal_actions.dedup();
            }
        }
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
            let label = declared_claims
                .iter()
                .find(|claim| claim.seat_index == seat_index)
                .map(|claim| claim.label.clone())
                .unwrap_or(TrainingLabel::Pass);
            let mut claim_options =
                self.claim_options(seat_index, discarder_seat, discarded_tile_key);
            if matches!(label, TrainingLabel::Hu)
                && !claim_options
                    .iter()
                    .any(|option| option.action_type == "hu")
            {
                claim_options.push(SerializableClaimOption {
                    action_type: "hu".to_string(),
                    tile_ids: Vec::new(),
                });
            }
            if claim_options.is_empty() {
                continue;
            }
            let context = self.context(record, seat_index, Vec::new(), claim_options);
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

    fn rob_kong_samples(
        &self,
        record: &BotZoneMatch,
        decision_index: &mut u64,
        kong_seat: usize,
        tile_key: &str,
        declared_rob_kongs: &[DeclaredClaim],
        outcome_by_seat: &[SampleOutcome; 4],
    ) -> Vec<TrainingDecisionSampleV2> {
        let mut samples = Vec::new();
        for seat_index in 0..4 {
            if seat_index == kong_seat {
                continue;
            }
            let label = declared_rob_kongs
                .iter()
                .find(|claim| claim.seat_index == seat_index)
                .map(|claim| claim.label.clone())
                .unwrap_or(TrainingLabel::Pass);
            let mut context = self.context(record, seat_index, Vec::new(), Vec::new());
            context.last_discard_tile_key = Some(tile_key.to_string());
            context.add_kong_risk_tiles.insert(tile_key.to_string());
            samples.push(TrainingDecisionSampleV2 {
                schema_version: 2,
                match_id: record.match_id.clone(),
                decision_index: *decision_index,
                seat_index,
                decision_kind: DecisionKind::RobKong,
                context,
                legal_actions: vec!["claim:hu".to_string(), "pass".to_string()],
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
        self_kong_candidates: Vec<SerializableSelfKongCandidate>,
        claim_options: Vec<SerializableClaimOption>,
    ) -> SerializableBotContext {
        SerializableBotContext {
            seat_index,
            seat_count: 4,
            dealer_seat: 0,
            round_wind: record.round_wind.clone(),
            cumulative_scores: vec![0, 0, 0, 0],
            wall_tiles_remaining: self.wall_tiles_remaining,
            visible_tile_keys: self.visible_tile_keys(),
            opponent_discards_by_seat: self.discards.iter().cloned().collect(),
            opponent_melds_by_seat: self.melds.iter().cloned().collect(),
            discard_history: self.discard_history.clone(),
            player: SerializableBotPlayer {
                concealed_tiles: self.hands[seat_index].clone(),
                concealed_tile_counts: tile_counts(&self.hands[seat_index]).to_vec(),
                meld_tile_key_groups: self.melds[seat_index].clone(),
                flower_count: 0,
            },
            restricted_discard_tile_key: self.restricted_discard_tile_key.clone(),
            drawn_tile_id: self.current_drawn_tile_ids[seat_index].clone(),
            self_kong_candidates,
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
            for tile_ids in chow_tile_id_options(&self.hands[seat_index], discarded_tile_key) {
                options.push(SerializableClaimOption {
                    action_type: "chow".to_string(),
                    tile_ids,
                });
            }
        }
        options
    }

    fn has_self_kong_candidates(&self, seat_index: usize) -> bool {
        !self.self_kong_candidates(seat_index).is_empty()
    }

    fn self_kong_candidates(&self, seat_index: usize) -> Vec<SerializableSelfKongCandidate> {
        let mut candidates = Vec::new();
        let mut seen_concealed = HashSet::new();
        for tile in &self.hands[seat_index] {
            if !seen_concealed.insert(tile.tile_key.clone()) {
                continue;
            }
            let tile_ids = take_tile_ids(&self.hands[seat_index], &tile.tile_key, 4);
            if tile_ids.len() == 4 {
                candidates.push(SerializableSelfKongCandidate {
                    kind: "concealed_kong".to_string(),
                    tile_ids,
                    tile_key: tile.tile_key.clone(),
                    meld_index: None,
                });
            }
        }

        for (meld_index, meld) in self.melds[seat_index].iter().enumerate() {
            if meld.len() != 3 {
                continue;
            }
            let Some(tile_key) = meld.first() else {
                continue;
            };
            if !meld.iter().all(|candidate| candidate == tile_key) {
                continue;
            }
            let tile_ids = take_tile_ids(&self.hands[seat_index], tile_key, 1);
            if tile_ids.len() == 1 {
                candidates.push(SerializableSelfKongCandidate {
                    kind: "add_kong".to_string(),
                    tile_ids,
                    tile_key: tile_key.clone(),
                    meld_index: Some(meld_index),
                });
            }
        }

        candidates
    }

    fn apply_same_tile_meld(&mut self, seat: usize, tile_key: &str, count: usize) {
        self.remove_claimed_discard_from_discards(tile_key);
        for _ in 0..count {
            self.remove_one_tile(seat, tile_key);
        }
        self.melds[seat].push(vec![tile_key.to_string(); count + 1]);
        self.current_drawn_tile_ids[seat] = None;
        self.restricted_discard_tile_key = Some(tile_key.to_string());
        self.last_discard_tile_key = None;
        self.last_discarder_seat = None;
    }

    fn apply_concealed_kong(&mut self, seat: usize, tile_key: &str) {
        for _ in 0..4 {
            self.remove_one_tile(seat, tile_key);
        }
        self.melds[seat].push(vec![tile_key.to_string(); 4]);
        self.current_drawn_tile_ids[seat] = None;
    }

    fn apply_chow(&mut self, seat: usize, middle_tile_key: &str) {
        let Some(last_discard) = self.last_discard_tile_key.clone() else {
            return;
        };
        self.remove_claimed_discard_from_discards(&last_discard);
        if let Some(tile_ids) =
            chow_tile_ids_for_middle(&self.hands[seat], &last_discard, middle_tile_key)
        {
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
        self.current_drawn_tile_ids[seat] = None;
        self.restricted_discard_tile_key = Some(last_discard);
        self.last_discard_tile_key = None;
        self.last_discarder_seat = None;
    }

    fn apply_add_kong(&mut self, seat: usize, tile_key: &str) {
        self.remove_one_tile(seat, tile_key);
        self.current_drawn_tile_ids[seat] = None;
        if let Some(meld) = self.melds[seat]
            .iter_mut()
            .find(|meld| meld.len() == 3 && meld.iter().all(|key| key == tile_key))
        {
            meld.push(tile_key.to_string());
        }
    }

    fn remove_claimed_discard_from_discards(&mut self, tile_key: &str) {
        if let Some(discarder_seat) = self.last_discarder_seat
            && self.discards[discarder_seat]
                .last()
                .is_some_and(|discard| discard == tile_key)
        {
            self.discards[discarder_seat].pop();
        }
    }

    fn visible_tile_keys(&self) -> Vec<String> {
        let mut visible_tile_keys = Vec::new();
        for discards in &self.discards {
            visible_tile_keys.extend(discards.iter().cloned());
        }
        for melds in &self.melds {
            for meld in melds {
                extend_visible_meld_tile_keys(&mut visible_tile_keys, meld);
            }
        }
        visible_tile_keys
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

fn declared_rob_kongs_after_add_kong(
    record: &BotZoneMatch,
    event_index: usize,
) -> Vec<DeclaredClaim> {
    let mut claims = record.events[event_index]
        .ignored_claims
        .iter()
        .filter_map(|claim| match &claim.action {
            BotZoneAction::Hu { .. } => Some(DeclaredClaim {
                seat_index: claim.actor,
                label: TrainingLabel::Hu,
            }),
            _ => None,
        })
        .collect::<Vec<_>>();

    if let Some(next_event) = record.events.get(event_index + 1)
        && matches!(next_event.action, BotZoneAction::Hu { .. })
    {
        claims.push(DeclaredClaim {
            seat_index: next_event.actor,
            label: TrainingLabel::Hu,
        });
    }

    claims.sort_by_key(|claim| claim.seat_index);
    claims.dedup_by_key(|claim| claim.seat_index);
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
        .filter(|tile| {
            Some(tile.tile_key.as_str()) != context.restricted_discard_tile_key.as_deref()
        })
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

fn self_kong_legal_actions(context: &SerializableBotContext) -> Vec<String> {
    let mut actions = vec!["pass".to_string()];
    for candidate in &context.self_kong_candidates {
        actions.push(format!(
            "self_kong:{}:{}",
            candidate.kind, candidate.tile_key
        ));
    }
    actions.sort();
    actions.dedup();
    actions
}

fn outcome_by_seat(record: &BotZoneMatch, exact_fan: i64) -> [SampleOutcome; 4] {
    let (score_delta, round_drawn) = match &record.result {
        BotZoneResult::Hu { score_delta, .. } => (*score_delta, false),
        BotZoneResult::Huang { score_delta } => (*score_delta, true),
    };
    let winner_event = record
        .events
        .iter()
        .enumerate()
        .find(|(_, event)| matches!(event.action, BotZoneAction::Hu { .. }));
    let winner = winner_event.map(|(_, event)| event.actor);
    let discarder = winner_event.and_then(|(event_index, _)| {
        let previous_event = event_index
            .checked_sub(1)
            .and_then(|index| record.events.get(index))?;
        match previous_event.action {
            BotZoneAction::Play { .. } => Some(previous_event.actor),
            _ => None,
        }
    });
    std::array::from_fn(|seat| SampleOutcome {
        score_delta: score_delta[seat],
        fan_count: if winner == Some(seat) { exact_fan } else { 0 },
        won: winner == Some(seat),
        dealt_in: winner.is_some() && discarder == Some(seat) && winner != Some(seat),
        round_drawn,
    })
}

fn calculate_match_exact_fan(record: &BotZoneMatch) -> i64 {
    let mut state = ReplayState::new(record);
    for event in &record.events {
        match &event.action {
            BotZoneAction::Draw { tile_key } => state.add_tile(event.actor, tile_key),
            BotZoneAction::Play { tile_key } => {
                state.remove_one_tile(event.actor, tile_key);
                state.discards[event.actor].push(tile_key.clone());
                state.last_discard_tile_key = Some(tile_key.clone());
                state.last_discarder_seat = Some(event.actor);
                state.current_drawn_tile_ids[event.actor] = None;
            }
            BotZoneAction::Chi { middle_tile_key } => state.apply_chow(event.actor, middle_tile_key),
            BotZoneAction::Peng { tile_key } => state.apply_same_tile_meld(event.actor, tile_key, 2),
            BotZoneAction::Gang { tile_key } => state.apply_same_tile_meld(event.actor, tile_key, 3),
            BotZoneAction::AnGang { tile_key } => state.apply_concealed_kong(event.actor, tile_key),
            BotZoneAction::BuGang { tile_key } => state.apply_add_kong(event.actor, tile_key),
            BotZoneAction::Hu { tile_key } => {
                let win_type = if state.current_drawn_tile_ids[event.actor].is_some() {
                    "self_draw"
                } else {
                    "discard"
                };
                return calculate_exact_fan(&state, record, event.actor, tile_key, win_type);
            }
        }
    }
    0
}

fn calculate_exact_fan(
    state: &ReplayState,
    record: &BotZoneMatch,
    winner_seat: usize,
    winning_tile: &str,
    win_type: &str,
) -> i64 {
    let seat_wind = seat_index_to_wind(winner_seat);
    let round_wind = &record.round_wind;

    let concealed_tile_keys: Vec<String> =
        state.hands[winner_seat].iter().map(|t| t.tile_key.clone()).collect();
    let meld_tile_key_groups = state.melds[winner_seat].clone();

    let features = extract_hand_features(
        &concealed_tile_keys,
        &meld_tile_key_groups,
        None,
        Some(winning_tile),
        Some(&seat_wind),
        Some(round_wind),
        None,
    );

    let timing = TimingFeatures {
        robbing_the_kong: win_type == "rob_kong",
        ..Default::default()
    };

    let input = EvaluationInput {
        win_type: win_type.to_string(),
        winner_seat: Some(winner_seat),
        discarder_seat: state.last_discarder_seat,
        ready_hand_declared: false,
        flower_count: 0,
        seat_count: 4,
        features,
        timing,
        kong_entries: Vec::new(), // 简化处理，暂不追踪历史杠分明细
        tile_keys: [concealed_tile_keys.clone(), meld_tile_key_groups.iter().flatten().cloned().collect()].concat(),
        visible_tile_keys: state.visible_tile_keys(),
        concealed_tile_keys,
        meld_tile_key_groups: meld_tile_key_groups.clone(),
        open_meld_tile_key_groups: meld_tile_key_groups,
        incoming_tile: Some(winning_tile.to_string()),
        winning_tile: Some(winning_tile.to_string()),
        decompositions: Vec::new(),
    };

    evaluate_fans(input).fan_total
}

fn seat_index_to_wind(index: usize) -> String {
    match index {
        0 => "east",
        1 => "north",
        2 => "west",
        3 => "south",
        _ => "east",
    }
    .to_string()
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

fn extend_visible_meld_tile_keys(target: &mut Vec<String>, meld_tile_keys: &[String]) {
    if meld_tile_keys.len() == 4
        && meld_tile_keys
            .iter()
            .all(|tile_key| tile_key == &meld_tile_keys[0])
    {
        target.extend(meld_tile_keys.iter().take(3).cloned());
    } else {
        target.extend(meld_tile_keys.iter().cloned());
    }
}

fn take_tile_ids(tiles: &[SerializableBotTile], tile_key: &str, count: usize) -> Vec<String> {
    tiles
        .iter()
        .filter(|tile| tile.tile_key == tile_key)
        .map(|tile| tile.tile_id.clone())
        .take(count)
        .collect()
}

fn chow_tile_id_options(
    tiles: &[SerializableBotTile],
    discarded_tile_key: &str,
) -> Vec<Vec<String>> {
    let Some(discard_index) = tile_index(discarded_tile_key) else {
        return Vec::new();
    };
    let mut options = Vec::new();
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
            options.push(vec![left_id, right_id]);
        }
    }
    options
}

fn chow_tile_ids_for_middle(
    tiles: &[SerializableBotTile],
    discarded_tile_key: &str,
    middle_tile_key: &str,
) -> Option<Vec<String>> {
    chow_tile_id_options(tiles, discarded_tile_key)
        .into_iter()
        .find(|tile_ids| {
            let mut keys = vec![discarded_tile_key.to_string()];
            for tile_id in tile_ids {
                if let Some(tile) = tiles.iter().find(|tile| &tile.tile_id == tile_id) {
                    keys.push(tile.tile_key.clone());
                }
            }
            keys.sort_by_key(|key| tile_index(key).unwrap_or(usize::MAX));
            keys.get(1).is_some_and(|key| key == middle_tile_key)
        })
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

    const DISCARD_HU_MATCH: &str = r#"
Match discard hu
Player 0 Deal W1 W9 T9
Player 1 Deal B1 B2 B3
Player 2 Deal T1 T2 T3
Player 3 Deal J2 J2 B6
Player 2 Draw T9
Player 2 Play J2
Player 3 Hu J2
Score 0 0 -8 8
"#;

    const CONCEALED_KONG_MATCH: &str = r#"
Match concealed kong
Player 0 Deal W1 W1 W1 W1
Player 1 Deal T1 T2 T3
Player 2 Deal B1 B2 B3
Player 3 Deal W2 W3 W4
Player 0 AnGang W1
Score 0 0 0 0
"#;

    const ADD_KONG_MATCH: &str = r#"
Match add kong
Player 0 Deal W1 B1 B2
Player 1 Deal W1 W1 T1
Player 2 Deal T2 T3 T4
Player 3 Deal B3 B4 B5
Player 0 Draw B3
Player 0 Play W1
Player 1 Peng W1
Player 1 Draw W1
Player 1 BuGang W1
Score 0 0 0 0
"#;

    const CLAIMED_TURN_CONTEXT_MATCH: &str = r#"
Match claimed turn context
Wind 1
Player 0 Deal B1 W1 W2 W3 W4 W5 W6 W7 W8 W9 T1 T2 T3
Player 1 Deal B1 B1 B1 T1 T2 T3 T4 T5 T6 T7 T8 T9 J1
Player 2 Deal W1 W2 W3 W4 W5 W6 W7 W8 W9 B2 B3 B4 B5
Player 3 Deal T1 T2 T3 T4 T5 T6 T7 T8 T9 J2 J3 F1 F2
Player 0 Draw J3
Player 0 Play B1
Player 1 Peng B1
Player 1 Play T1
Score 0 0 0 0
"#;

    const SELF_DRAW_MATCH: &str = r#"
Match self draw outcome
Player 0 Deal W1 W9 T9
Player 1 Deal B1 B2 B3
Player 2 Deal T1 T2 T3
Player 3 Deal J2 J2 B6
Player 0 Draw W2
Player 0 Play W9
Player 1 Draw B4
Player 1 Hu B4
Score 8 -8 0 0
"#;

    const ROB_KONG_MATCH: &str = r#"
Match rob kong
Player 0 Deal W1 B1 B2
Player 1 Deal W1 W1 T1
Player 2 Deal T2 T3 T4
Player 3 Deal B3 B4 B5
Player 0 Draw B3
Player 0 Play W1
Player 1 Peng W1
Player 1 Draw W1
Player 1 BuGang W1 Ignore Player 2 Hu W1
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

    #[test]
    fn replay_declared_discard_hu_is_a_legal_claim_action() {
        let record = parse_match(DISCARD_HU_MATCH).expect("match");
        let samples = replay_match_to_samples(&record).expect("samples");

        let hu = samples
            .iter()
            .find(|sample| {
                sample.seat_index == 3
                    && sample.decision_kind == DecisionKind::ClaimWindow
                    && sample.label == TrainingLabel::Hu
            })
            .expect("discard hu sample");

        assert!(hu.legal_actions.iter().any(|action| action == "claim:hu"));
    }

    #[test]
    fn replay_emits_concealed_kong_sample_before_an_gang() {
        let record = parse_match(CONCEALED_KONG_MATCH).expect("match");
        let samples = replay_match_to_samples(&record).expect("samples");

        let kong = samples
            .iter()
            .find(|sample| {
                sample.seat_index == 0
                    && sample.label
                        == TrainingLabel::SelfKong {
                            kind: "concealed_kong".to_string(),
                            tile_key: "w1".to_string(),
                        }
            })
            .expect("concealed kong sample");

        assert_eq!(kong.decision_kind, DecisionKind::ActiveTurn);
        assert!(
            kong.legal_actions
                .iter()
                .any(|action| action == "self_kong:concealed_kong:w1")
        );
        assert_eq!(kong.context.self_kong_candidates.len(), 1);
    }

    #[test]
    fn replay_emits_add_kong_sample_before_bu_gang() {
        let record = parse_match(ADD_KONG_MATCH).expect("match");
        let samples = replay_match_to_samples(&record).expect("samples");

        let kong = samples
            .iter()
            .find(|sample| {
                sample.seat_index == 1
                    && sample.label
                        == TrainingLabel::SelfKong {
                            kind: "add_kong".to_string(),
                            tile_key: "w1".to_string(),
                        }
            })
            .expect("add kong sample");

        assert_eq!(kong.decision_kind, DecisionKind::ActiveTurn);
        assert!(
            kong.legal_actions
                .iter()
                .any(|action| action == "self_kong:add_kong:w1")
        );
        assert_eq!(kong.context.self_kong_candidates[0].meld_index, Some(0));
    }

    #[test]
    fn claimed_turn_context_matches_runtime_active_turn_state() {
        let record = parse_match(CLAIMED_TURN_CONTEXT_MATCH).expect("match");
        let samples = replay_match_to_samples(&record).expect("samples");

        let claimed_turn = samples
            .iter()
            .find(|sample| {
                sample.seat_index == 1
                    && sample.decision_kind == DecisionKind::ActiveTurn
                    && sample.label
                        == TrainingLabel::Discard {
                            tile_key: "t1".to_string(),
                        }
            })
            .expect("active turn after pung claim");

        assert_eq!(claimed_turn.context.round_wind, "north");
        assert_eq!(claimed_turn.context.wall_tiles_remaining, 83);
        assert_eq!(claimed_turn.context.drawn_tile_id, None);
        assert_eq!(
            claimed_turn.context.restricted_discard_tile_key.as_deref(),
            Some("b1")
        );
        assert!(
            !claimed_turn
                .legal_actions
                .iter()
                .any(|action| action == "discard:b1")
        );
        assert!(!claimed_turn.context.opponent_discards_by_seat[0].contains(&"b1".to_string()));
        assert!(
            claimed_turn.context.opponent_melds_by_seat[1]
                .iter()
                .any(|meld| meld == &vec!["b1".to_string(), "b1".to_string(), "b1".to_string()])
        );
        assert_eq!(
            claimed_turn
                .context
                .visible_tile_keys
                .iter()
                .filter(|tile_key| tile_key.as_str() == "b1")
                .count(),
            3
        );
        assert_eq!(claimed_turn.context.last_discard_tile_key, None);
        assert_eq!(claimed_turn.context.discard_history.len(), 1);
        assert_eq!(claimed_turn.context.discard_history[0].seat_index, 0);
        assert_eq!(claimed_turn.context.discard_history[0].tile_key, "b1");
    }

    #[test]
    fn same_tile_key_after_chow_matches_runtime_restricted_discard_rule() {
        let record = parse_match(
            r#"
Match same key after chow
Player 0 Deal B9 W1 W2
Player 1 Deal B7 B8 B9 T3 T4 J3
Player 2 Deal W3 W4 W5
Player 3 Deal T1 T2 T3
Player 0 Draw J1
Player 0 Play B9
Player 1 Chi B8
Player 1 Play B9
Score 0 0 0 0
"#,
        )
        .expect("match");
        let samples = replay_match_to_samples(&record).expect("samples");

        let claimed_turn = samples
            .iter()
            .find(|sample| {
                sample.seat_index == 1
                    && sample.decision_kind == DecisionKind::ActiveTurn
                    && sample.context.restricted_discard_tile_key.as_deref() == Some("b9")
            })
            .expect("active turn after chow");

        assert!(
            !claimed_turn
                .legal_actions
                .iter()
                .any(|action| action == "discard:b9")
        );
    }

    #[test]
    fn self_draw_outcome_does_not_mark_previous_discarder_as_dealt_in() {
        let record = parse_match(SELF_DRAW_MATCH).expect("match");
        let samples = replay_match_to_samples(&record).expect("samples");

        assert!(samples.iter().all(|sample| !sample.outcome.dealt_in));
    }

    #[test]
    fn replay_emits_rob_kong_hu_and_pass_samples_after_bu_gang() {
        let record = parse_match(ROB_KONG_MATCH).expect("match");
        let samples = replay_match_to_samples(&record).expect("samples");

        let hu = samples
            .iter()
            .find(|sample| {
                sample.decision_kind == DecisionKind::RobKong
                    && sample.seat_index == 2
                    && sample.label == TrainingLabel::Hu
            })
            .expect("rob kong hu sample");
        assert!(hu.legal_actions.iter().any(|action| action == "claim:hu"));
        assert!(hu.context.add_kong_risk_tiles.contains("w1"));

        let pass = samples
            .iter()
            .find(|sample| {
                sample.decision_kind == DecisionKind::RobKong
                    && sample.seat_index == 3
                    && sample.label == TrainingLabel::Pass
            })
            .expect("rob kong pass sample");
        assert!(pass.legal_actions.iter().any(|action| action == "pass"));
    }
}
