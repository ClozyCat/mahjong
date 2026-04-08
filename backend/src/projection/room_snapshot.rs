use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Value, json};

use crate::core::ids::Seat;
use crate::core::state::{PendingAction, RoomState};
use crate::projection::SeatProjectionSupport;
use crate::rules::skills::{
    EffectInstance, KnowledgeEffect, build_skill_projection, skill_action_options,
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
    match_state: Option<Value>,
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
    score_state: ScoreStateView,
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
}

#[derive(Debug, Clone, Serialize)]
struct PrivateTileView {
    tile_id: String,
    tile_key: String,
}

#[derive(Debug, Clone, Serialize)]
struct ScoreStateView {
    flower_count_by_seat: BTreeMap<Seat, i64>,
    kong_score_detail: Vec<Value>,
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
        match_state: state
            .match_state
            .as_ref()
            .map(|match_state| serde_json::to_value(match_state).unwrap_or(Value::Null)),
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
        score_state: score_state_view(state),
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
        flower_count_by_seat.insert(seat, flower_total);
        base_cumulative_scores.insert(seat, base);
        projected_cumulative_scores.insert(seat, base);
        current_round_delta_by_seat.insert(seat, 0);
        kong_delta_by_seat.insert(seat, 0);
    }

    ScoreStateView {
        flower_count_by_seat,
        kong_score_detail: Vec::new(),
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
