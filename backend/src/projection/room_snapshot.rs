use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Value, json};

use crate::core::ids::Seat;
use crate::core::state::{MatchState, PendingAction, RoomState, SettlementKongScoreDetailEntry};
use crate::projection::SeatProjectionSupport;
use crate::rules::skills::{
    EffectInstance, EquippedSkillView, KnowledgeEffect, SkillDraftSelectionView,
    build_skill_projection, equipped_skill_views, public_skill_view, skill_action_options,
    skill_draft_view,
};

#[derive(Debug, Clone, Serialize)]
struct RoomSnapshotMessage {
    #[serde(rename = "type")]
    kind: &'static str,
    payload: PlayerRoomSnapshot,
}

#[derive(Debug, Clone, Serialize)]
struct PlayerRoomSnapshot {
    table_code: String,
    phase: String,
    mode: String,
    seats: Vec<PublicSeatView>,
    local_seat: Seat,
    reconnect_token: Option<String>,
    match_state: Option<MatchState>,
    private_state: Option<PlayerRoundView>,
    continue_action: Option<ContinueActionView>,
}

#[derive(Debug, Clone, Serialize)]
struct PublicSeatView {
    seat_index: Seat,
    nickname: Option<String>,
    connected: bool,
    ready: bool,
    is_bot: bool,
    seat_type: String,
}

#[derive(Debug, Clone, Serialize)]
struct PlayerRoundView {
    round_id: String,
    round_wind: String,
    dealer_seat: Seat,
    current_actor: Seat,
    wall_tiles_remaining: usize,
    last_discard: Option<String>,
    pending_action: Option<PendingActionView>,
    skill_draft: Option<SkillDraftSelectionView>,
    score_state: ScoreStateView,
    equipped_skills: Vec<EquippedSkillView>,
    visible_effects: Vec<VisibleEffectView>,
    private_knowledge: Vec<KnowledgeView>,
    players: Vec<PlayerSeatView>,
}

#[derive(Debug, Clone, Serialize)]
struct PlayerSeatView {
    seat_index: Seat,
    nickname: Option<String>,
    connected: bool,
    concealed_count: usize,
    concealed_tiles: Option<Vec<PrivateTileView>>,
    melds: Vec<Vec<String>>,
    flowers: Vec<String>,
    discards: Vec<String>,
    equipped_skill: Option<EquippedSkillView>,
}

#[derive(Debug, Clone, Serialize)]
struct PrivateTileView {
    tile_id: String,
    tile_key: String,
}

#[derive(Debug, Clone, Serialize)]
struct ScoreStateView {
    flower_count_by_seat: BTreeMap<Seat, i64>,
    kong_score_detail: Vec<SettlementKongScoreDetailEntry>,
    kong_delta_by_seat: BTreeMap<Seat, i64>,
    current_round_delta_by_seat: BTreeMap<Seat, i64>,
    base_cumulative_scores: BTreeMap<Seat, i64>,
    projected_cumulative_scores: BTreeMap<Seat, i64>,
}

#[derive(Debug, Clone, Serialize)]
struct VisibleEffectView {
    effect_id: String,
    effect_type: String,
    owner: Seat,
    target_seats: Vec<Seat>,
    remaining_turns: Option<u8>,
    stacks: u8,
    source_skill: Option<String>,
    payload: Value,
}

#[derive(Debug, Clone, Serialize)]
struct KnowledgeView {
    target_seat: Option<Seat>,
    tile_ids: Vec<String>,
    tile_keys: Vec<String>,
    source_skill: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ContinueActionView {
    action_id: String,
    confirmed_seats: Vec<Seat>,
    required_seats: Vec<Seat>,
    online_seats: Vec<Seat>,
    auto_advance_deadline_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum PendingActionView {
    #[serde(rename = "opening_flowers")]
    OpeningFlowers {
        seat_index: Seat,
        deadline_at: Option<String>,
        options: Vec<String>,
    },
    #[serde(rename = "active_turn")]
    ActiveTurn {
        seat_index: Seat,
        deadline_at: Option<String>,
        drawn_tile_id: Option<String>,
        restricted_discard_tile_ids: Vec<String>,
        options: Vec<String>,
    },
    #[serde(rename = "claim_window")]
    ClaimWindow {
        discarder_seat: Seat,
        deadline_at: Option<String>,
        responded_seats: Vec<Seat>,
        options: Vec<String>,
    },
    #[serde(rename = "rob_kong_window")]
    RobKongWindow {
        actor_seat: Seat,
        tile_key: Option<String>,
        deadline_at: Option<String>,
        responded_seats: Vec<Seat>,
        options: Vec<String>,
    },
    #[serde(rename = "skill_draft")]
    SkillDraft {
        seat_index: Seat,
        deadline_at: Option<String>,
        options: Vec<String>,
    },
}

pub fn room_snapshot_message(
    state: &RoomState,
    local_seat: Seat,
    support: &SeatProjectionSupport,
) -> Value {
    let payload = PlayerRoomSnapshot {
        table_code: state.table_code.clone(),
        phase: state.phase.clone(),
        mode: state.mode.clone(),
        seats: public_seats(state),
        local_seat,
        reconnect_token: reconnect_token(state, local_seat),
        match_state: state.match_state.clone(),
        private_state: private_round_state(state, local_seat, support),
        continue_action: continue_action_snapshot(state),
    };
    serde_json::to_value(RoomSnapshotMessage {
        kind: "room_snapshot",
        payload,
    })
    .unwrap_or_else(|_| {
        json!({
            "type": "room_snapshot",
            "payload": {
                "table_code": state.table_code,
                "phase": state.phase,
                "mode": state.mode,
                "seats": [],
                "local_seat": local_seat,
                "reconnect_token": Value::Null,
                "match_state": Value::Null,
                "private_state": Value::Null,
                "continue_action": Value::Null,
            }
        })
    })
}

pub fn build_pending_action_view(
    state: &RoomState,
    local_seat: Seat,
    support: &SeatProjectionSupport,
) -> Option<PendingActionView> {
    let pending_timeout = state.pending_timeout.as_ref()?;
    let round = state.round_state.as_ref()?;
    let deadline_at = pending_timeout.deadline_at.clone();

    match pending_timeout.kind.as_str() {
        "opening_flowers" => {
            if round.current_actor != local_seat {
                return None;
            }
            let options = if support.has_concealed_flower {
                vec!["flower".to_string()]
            } else {
                vec!["pass".to_string()]
            };
            Some(PendingActionView::OpeningFlowers {
                seat_index: local_seat,
                deadline_at,
                options,
            })
        }
        "active_turn" => {
            if round.current_actor != local_seat {
                return None;
            }
            let mut options = vec!["discard".to_string()];
            if support.has_concealed_flower {
                options.push("flower".to_string());
            }
            if support.has_self_kong {
                options.push("kong".to_string());
            }
            if support.can_hu {
                options.push("hu".to_string());
            }
            options.extend(skill_action_options(state, local_seat));
            Some(PendingActionView::ActiveTurn {
                seat_index: local_seat,
                deadline_at,
                drawn_tile_id: pending_timeout.drawn_tile_id.clone(),
                restricted_discard_tile_ids: support.restricted_discard_tile_ids.clone(),
                options,
            })
        }
        "claim_window" => {
            let PendingAction::ClaimWindow(claim) = round.pending_action.as_ref()? else {
                return None;
            };
            let options = claim
                .claim_window
                .get(local_seat)
                .cloned()
                .unwrap_or_default();
            let is_responded = claim.responded_seats.contains(&local_seat);
            let mut payload_options = options;
            if !payload_options.is_empty() && !is_responded {
                payload_options.push("pass".to_string());
            }
            Some(PendingActionView::ClaimWindow {
                discarder_seat: claim.discarder_seat,
                deadline_at,
                responded_seats: claim.responded_seats.clone(),
                options: payload_options,
            })
        }
        "rob_kong_window" => {
            let PendingAction::RobKongWindow(rob) = round.pending_action.as_ref()? else {
                return None;
            };
            let offered = rob.offered_hu_seats.contains(&local_seat);
            let is_responded = rob.responded_seats.contains(&local_seat);
            let options = if offered && !is_responded {
                vec!["hu".to_string(), "pass".to_string()]
            } else {
                Vec::new()
            };
            Some(PendingActionView::RobKongWindow {
                actor_seat: rob.actor_seat,
                tile_key: rob.tile_key.clone(),
                deadline_at,
                responded_seats: rob.responded_seats.clone(),
                options,
            })
        }
        "skill_draft" => {
            skill_draft_view(state, local_seat).map(|selection| PendingActionView::SkillDraft {
                seat_index: local_seat,
                deadline_at: Some(selection.deadline_at),
                options: vec!["select_skill".to_string(), "decline_skill".to_string()],
            })
        }
        _ => None,
    }
}

fn public_seats(state: &RoomState) -> Vec<PublicSeatView> {
    state
        .seats
        .iter()
        .map(|seat| PublicSeatView {
            seat_index: seat.seat_index,
            nickname: seat.nickname.clone(),
            connected: seat.connected,
            ready: seat.ready,
            is_bot: seat.is_bot,
            seat_type: seat.seat_type.clone(),
        })
        .collect()
}

fn reconnect_token(state: &RoomState, local_seat: Seat) -> Option<String> {
    state
        .seats
        .iter()
        .find(|seat| seat.seat_index == local_seat)
        .and_then(|seat| seat.reconnect_token.clone())
}

fn private_round_state(
    state: &RoomState,
    local_seat: Seat,
    support: &SeatProjectionSupport,
) -> Option<PlayerRoundView> {
    let round = state.round_state.as_ref()?;
    let skill_projection = build_skill_projection(state, local_seat);
    let private_players = round
        .players
        .iter()
        .map(|player| {
            let seat_info = state
                .seats
                .iter()
                .find(|seat| seat.seat_index == player.seat);
            let concealed_tiles = if round.phase == "settlement" || player.seat == local_seat {
                Some(
                    player
                        .concealed_tiles
                        .iter()
                        .map(|tile| PrivateTileView {
                            tile_id: tile.tile_id.clone(),
                            tile_key: tile.tile_key.clone(),
                        })
                        .collect(),
                )
            } else {
                None
            };
            PlayerSeatView {
                seat_index: player.seat,
                nickname: seat_info.and_then(|seat| seat.nickname.clone()),
                connected: seat_info.map(|seat| seat.connected).unwrap_or(false),
                concealed_count: player.concealed_tiles.len(),
                concealed_tiles,
                melds: player.melds.clone(),
                flowers: player
                    .flowers
                    .iter()
                    .map(|tile| tile.tile_key.clone())
                    .collect(),
                discards: player
                    .discards
                    .iter()
                    .map(|tile| tile.tile_key.clone())
                    .collect(),
                equipped_skill: public_skill_view(state, player.seat),
            }
        })
        .collect();

    Some(PlayerRoundView {
        round_id: round.round_id.clone(),
        round_wind: round.round_wind.clone(),
        dealer_seat: round.dealer_seat,
        current_actor: round.current_actor,
        wall_tiles_remaining: round.wall.live_tiles_remaining(),
        last_discard: round
            .last_discard
            .as_ref()
            .map(|tile| tile.tile_key.clone()),
        pending_action: build_pending_action_view(state, local_seat, support),
        skill_draft: skill_draft_view(state, local_seat),
        score_state: score_state_view(state),
        equipped_skills: equipped_skill_views(state, local_seat),
        visible_effects: visible_effects(skill_projection.visible_effects.as_slice()),
        private_knowledge: private_knowledge(skill_projection.private_knowledge.as_slice()),
        players: private_players,
    })
}

fn score_state_view(state: &RoomState) -> ScoreStateView {
    let seat_count = state
        .round_state
        .as_ref()
        .map(|round| round.players.len())
        .unwrap_or(0)
        .max(state.seats.len())
        .max(4);
    let mut flower_count_by_seat = BTreeMap::new();
    let mut base_cumulative_scores = BTreeMap::new();
    let mut projected_cumulative_scores = BTreeMap::new();
    let mut current_round_delta_by_seat = BTreeMap::new();
    let mut kong_delta_by_seat = BTreeMap::new();
    let settlement = state
        .round_state
        .as_ref()
        .and_then(|round| round.settlement.as_ref())
        .cloned();
    let kong_score_detail = settlement
        .as_ref()
        .map(|value| value.kong_score_detail.clone())
        .unwrap_or_default();
    let score_adjustments_by_seat = state
        .round_state
        .as_ref()
        .map(|round| round.skill_trackers.score_adjustments_by_seat.clone())
        .unwrap_or_default();

    for entry in &kong_score_detail {
        for (seat, delta) in &entry.delta_by_seat {
            *kong_delta_by_seat.entry(*seat).or_default() += *delta;
        }
    }

    for seat in 0..seat_count {
        let flower_total = state
            .round_state
            .as_ref()
            .and_then(|round| round.players.get(seat))
            .map(|player| player.flowers.len() as i64)
            .unwrap_or(0);
        let base = state
            .match_state
            .as_ref()
            .and_then(|match_state| match_state.cumulative_scores.get(&seat).copied())
            .unwrap_or(0);
        let score_adjustment = score_adjustments_by_seat.get(&seat).copied().unwrap_or(0);
        let settlement_total = settlement
            .as_ref()
            .and_then(|value| value.score_delta.total_delta_by_seat.get(&seat))
            .copied()
            .unwrap_or(0);
        let current_delta = if settlement.is_some() {
            settlement_total
        } else {
            kong_delta_by_seat.get(&seat).copied().unwrap_or(0) + score_adjustment
        };
        let base_score = if settlement.is_some() {
            base - settlement_total
        } else {
            base - score_adjustment
        };
        flower_count_by_seat.insert(seat, flower_total);
        base_cumulative_scores.insert(seat, base_score);
        projected_cumulative_scores.insert(seat, base_score + current_delta);
        current_round_delta_by_seat.insert(seat, current_delta);
        kong_delta_by_seat.entry(seat).or_insert(0);
    }

    ScoreStateView {
        flower_count_by_seat,
        kong_score_detail,
        kong_delta_by_seat,
        current_round_delta_by_seat,
        base_cumulative_scores,
        projected_cumulative_scores,
    }
}

fn continue_action_snapshot(state: &RoomState) -> Option<ContinueActionView> {
    state
        .continue_action
        .as_ref()
        .map(|continue_action| ContinueActionView {
            action_id: continue_action.action_id.clone(),
            confirmed_seats: continue_action.confirmed_seats.clone(),
            required_seats: continue_action.required_seats.clone(),
            online_seats: continue_action.online_seats.clone(),
            auto_advance_deadline_at: continue_action.auto_advance_deadline_at.clone(),
        })
}

fn visible_effects(effects: &[EffectInstance]) -> Vec<VisibleEffectView> {
    effects
        .iter()
        .map(|effect| VisibleEffectView {
            effect_id: effect.effect_id.clone(),
            effect_type: effect.effect_type.clone(),
            owner: effect.owner,
            target_seats: effect.target_seats.clone(),
            remaining_turns: effect.remaining_turns,
            stacks: effect.stacks,
            source_skill: effect.source_skill.clone(),
            payload: effect.payload.clone(),
        })
        .collect()
}

fn private_knowledge(knowledge: &[KnowledgeEffect]) -> Vec<KnowledgeView> {
    knowledge
        .iter()
        .map(|knowledge| KnowledgeView {
            target_seat: knowledge.target_seat,
            tile_ids: knowledge.tile_ids.clone(),
            tile_keys: knowledge.tile_keys.clone(),
            source_skill: knowledge.source_skill.clone(),
            description: knowledge.description.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::room_snapshot_message;
    use crate::core::state::{
        MatchState, PlayerRoundState, RoomState, RoundScoreTrackers, RoundSettlement,
        RoundSkillTrackers, RoundState, SeatState, SettlementKongScoreDetailEntry,
        SettlementScoreDelta,
    };
    use crate::projection::SeatProjectionSupport;

    #[test]
    fn score_state_projection_includes_immediate_skill_adjustments() {
        let state = RoomState {
            table_code: "ROOM42".to_string(),
            phase: "playing".to_string(),
            mode: "skill".to_string(),
            test_mode: false,
            enforce_minimum_eight_fan: true,
            seats: seats(),
            match_state: Some(MatchState {
                prevailing_wind: "east".to_string(),
                hand_number: 1,
                dealer_seat: 0,
                cumulative_scores: BTreeMap::from([(0, -3), (1, 0), (2, 0), (3, 0)]),
                match_finished: false,
                last_completed_round_id: None,
                statistics: Default::default(),
                skill_trackers: Default::default(),
            }),
            round_state: Some(RoundState {
                round_id: "round-1".to_string(),
                dealer_seat: 0,
                round_wind: "east".to_string(),
                current_actor: 0,
                phase: "playing".to_string(),
                players: players(),
                score_trackers: RoundScoreTrackers::default(),
                skill_trackers: RoundSkillTrackers {
                    score_adjustments_by_seat: BTreeMap::from([(0, -3)]),
                    ..Default::default()
                },
                ..Default::default()
            }),
            pending_timeout: None,
            continue_action: None,
        };

        let snapshot = room_snapshot_message(&state, 0, &SeatProjectionSupport::default());
        let score_state = &snapshot["payload"]["private_state"]["score_state"];
        assert_eq!(score_state["base_cumulative_scores"]["0"], 0);
        assert_eq!(score_state["current_round_delta_by_seat"]["0"], -3);
        assert_eq!(score_state["projected_cumulative_scores"]["0"], -3);
    }

    #[test]
    fn score_state_projection_uses_total_settlement_delta_without_double_counting_kongs() {
        let state = RoomState {
            table_code: "ROOM42".to_string(),
            phase: "settlement".to_string(),
            mode: "skill".to_string(),
            test_mode: false,
            enforce_minimum_eight_fan: true,
            seats: seats(),
            match_state: Some(MatchState {
                prevailing_wind: "east".to_string(),
                hand_number: 1,
                dealer_seat: 0,
                cumulative_scores: BTreeMap::from([(0, -9), (1, 9), (2, 0), (3, 0)]),
                match_finished: false,
                last_completed_round_id: Some("round-1".to_string()),
                statistics: Default::default(),
                skill_trackers: Default::default(),
            }),
            round_state: Some(RoundState {
                round_id: "round-1".to_string(),
                dealer_seat: 0,
                round_wind: "east".to_string(),
                current_actor: 0,
                phase: "settlement".to_string(),
                players: players(),
                settlement: Some(RoundSettlement {
                    provisional: true,
                    win_type: "discard".to_string(),
                    winner_seat: Some(1),
                    discarder_seat: Some(0),
                    display_win_label: None,
                    fan_total: 8,
                    fan_keys: Vec::new(),
                    fan_breakdown: Vec::new(),
                    score_delta: SettlementScoreDelta {
                        provisional: true,
                        basic_points: 8,
                        base_points: 8,
                        fan_total: 8,
                        minimum_qualifying_fan_total: 8,
                        fan_delta_by_seat: BTreeMap::from([(0, -8), (1, 8), (2, 0), (3, 0)]),
                        kong_delta_by_seat: BTreeMap::from([(0, -1), (1, 1), (2, 0), (3, 0)]),
                        total_delta_by_seat: BTreeMap::from([(0, -9), (1, 9), (2, 0), (3, 0)]),
                    },
                    flower_count: 0,
                    draw_type: None,
                    kong_score_detail: vec![SettlementKongScoreDetailEntry {
                        kong_type: "concealed_kong".to_string(),
                        actor_seat: 1,
                        payer_seats: vec![0],
                        delta_by_seat: BTreeMap::from([(0, -1), (1, 1), (2, 0), (3, 0)]),
                    }],
                }),
                ..Default::default()
            }),
            pending_timeout: None,
            continue_action: None,
        };

        let snapshot = room_snapshot_message(&state, 0, &SeatProjectionSupport::default());
        let score_state = &snapshot["payload"]["private_state"]["score_state"];
        assert_eq!(score_state["base_cumulative_scores"]["0"], 0);
        assert_eq!(score_state["current_round_delta_by_seat"]["0"], -9);
        assert_eq!(score_state["projected_cumulative_scores"]["0"], -9);
        assert_eq!(score_state["projected_cumulative_scores"]["1"], 9);
    }

    fn seats() -> Vec<SeatState> {
        (0..4)
            .map(|seat_index| SeatState {
                seat_index,
                connected: true,
                ready: true,
                seat_type: "human".to_string(),
                ..Default::default()
            })
            .collect()
    }

    fn players() -> Vec<PlayerRoundState> {
        (0..4)
            .map(|seat| PlayerRoundState {
                seat,
                ..Default::default()
            })
            .collect()
    }
}
