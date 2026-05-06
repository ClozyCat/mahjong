use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Value, json};

use crate::app::room_runtime::SpectatorIdentity;
use crate::core::ids::Seat;
use crate::core::state::{
    DisplayMeldOrientation, DisplayMeldState, DisplayMeldTileState, MatchState, PendingAction,
    RoomState, RoundState, SettlementKongScoreDetailEntry,
};
use crate::projection::SeatProjectionSupport;
use crate::projection::hand_insight::{HandInsightsView, build_hand_insights_view};

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
    owner_user_id: Option<i64>,
    multiplier: i64,
    seats: Vec<PublicSeatView>,
    spectators: Vec<PublicSpectatorView>,
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
struct PublicSpectatorView {
    user_id: i64,
    display_name: String,
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
    hand_insights: Option<HandInsightsView>,
    score_state: ScoreStateView,
    players: Vec<PlayerSeatView>,
}

#[derive(Debug, Clone, Serialize)]
struct PlayerSeatView {
    seat_index: Seat,
    nickname: Option<String>,
    connected: bool,
    is_ready_hand: bool,
    concealed_count: usize,
    concealed_tiles: Option<Vec<PrivateTileView>>,
    melds: Vec<Vec<String>>,
    display_melds: Vec<DisplayMeldView>,
    flowers: Vec<String>,
    discards: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DisplayMeldView {
    tiles: Vec<DisplayMeldTileView>,
}

#[derive(Debug, Clone, Serialize)]
struct DisplayMeldTileView {
    code: String,
    orientation: &'static str,
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
    room_snapshot_message_with_spectators(state, &[], local_seat, support)
}

pub(crate) fn room_snapshot_message_with_spectators(
    state: &RoomState,
    spectators: &[SpectatorIdentity],
    local_seat: Seat,
    support: &SeatProjectionSupport,
) -> Value {
    let payload = PlayerRoomSnapshot {
        table_code: state.table_code.clone(),
        phase: state.phase.clone(),
        mode: state.mode.clone(),
        owner_user_id: state.owner_user_id,
        multiplier: state.multiplier,
        seats: public_seats(state),
        spectators: public_spectators(spectators),
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
                "owner_user_id": state.owner_user_id,
                "multiplier": state.multiplier,
                "seats": [],
                "spectators": [],
                "local_seat": local_seat,
                "reconnect_token": Value::Null,
                "match_state": Value::Null,
                "private_state": Value::Null,
                "continue_action": Value::Null,
            }
        })
    })
}

pub fn observer_room_snapshot_message(state: &RoomState) -> Value {
    observer_room_snapshot_message_with_spectators(state, &[])
}

pub(crate) fn observer_room_snapshot_message_with_spectators(
    state: &RoomState,
    spectators: &[SpectatorIdentity],
) -> Value {
    let payload = PlayerRoomSnapshot {
        table_code: state.table_code.clone(),
        phase: state.phase.clone(),
        mode: state.mode.clone(),
        owner_user_id: state.owner_user_id,
        multiplier: state.multiplier,
        seats: public_seats(state),
        spectators: public_spectators(spectators),
        local_seat: usize::MAX,
        reconnect_token: None,
        match_state: state.match_state.clone(),
        private_state: observer_round_state(state),
        continue_action: continue_action_snapshot(state),
    };
    let mut value = serde_json::to_value(RoomSnapshotMessage {
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
                "owner_user_id": state.owner_user_id,
                "multiplier": state.multiplier,
                "seats": [],
                "spectators": [],
                "local_seat": Value::Null,
                "reconnect_token": Value::Null,
                "match_state": Value::Null,
                "private_state": Value::Null,
                "continue_action": Value::Null,
            }
        })
    });
    value["payload"]["local_seat"] = Value::Null;
    value
}

pub fn build_pending_action_view(
    state: &RoomState,
    local_seat: Seat,
    support: &SeatProjectionSupport,
) -> Option<PendingActionView> {
    let pending_timeout = state.pending_timeout.as_ref()?;
    let round = state.round_state.as_ref()?;
    let deadline_at = pending_timeout.deadline_at.clone();
    let local_player = round
        .players
        .iter()
        .find(|player| player.seat == local_seat);
    let is_local_ready_hand = local_player.is_some_and(|player| player.is_ready_hand);

    match pending_timeout.kind.as_str() {
        "active_turn" => {
            let is_local_turn = pending_timeout.seat_index == local_seat;
            let mut options = Vec::new();
            if is_local_turn {
                if support.can_hu {
                    options.push("hu".to_string());
                }
                if is_local_ready_hand {
                    if support.has_self_kong {
                        options.push("kong".to_string());
                    }
                } else {
                    options.insert(0, "discard".to_string());
                    if support.has_concealed_flower {
                        options.push("flower".to_string());
                    }
                    if support.has_self_kong {
                        options.push("kong".to_string());
                    }
                    if support.can_ready_hand {
                        options.push("ready_hand".to_string());
                    }
                }
                if support.can_hu || (is_local_ready_hand && support.has_self_kong) {
                    options.push("pass".to_string());
                }
            }

            Some(PendingActionView::ActiveTurn {
                seat_index: pending_timeout.seat_index,
                deadline_at,
                drawn_tile_id: if is_local_turn {
                    pending_timeout.drawn_tile_id.clone()
                } else {
                    None
                },
                restricted_discard_tile_ids: if is_local_turn {
                    support.restricted_discard_tile_ids.clone()
                } else {
                    Vec::new()
                },
                options,
            })
        }
        "claim_window" => match round.pending_action.as_ref()? {
            PendingAction::ClaimWindow(claim) => {
                let options = claim
                    .claim_window
                    .get(local_seat)
                    .cloned()
                    .unwrap_or_default();
                let is_responded = claim.responded_seats.contains(&local_seat);
                let mut payload_options = if is_local_ready_hand {
                    if is_responded {
                        Vec::new()
                    } else {
                        options
                            .into_iter()
                            .filter(|option| option == "hu" || option == "kong")
                            .collect()
                    }
                } else {
                    options
                };
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
            PendingAction::RobKongWindow(rob) => {
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
        },
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

fn public_spectators(spectators: &[SpectatorIdentity]) -> Vec<PublicSpectatorView> {
    spectators
        .iter()
        .map(|spectator| PublicSpectatorView {
            user_id: spectator.user_id,
            display_name: spectator.display_name.clone(),
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
                is_ready_hand: player.is_ready_hand,
                concealed_count: player.concealed_tiles.len(),
                concealed_tiles,
                melds: project_melds(round, player.seat, local_seat, &player.melds),
                display_melds: project_display_melds(
                    round,
                    player.seat,
                    local_seat,
                    &player.melds,
                    &player.display_melds,
                ),
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
        hand_insights: build_hand_insights_view(state, local_seat, support),
        score_state: score_state_view(state),
        players: private_players,
    })
}
fn observer_round_state(state: &RoomState) -> Option<PlayerRoundView> {
    let round = state.round_state.as_ref()?;
    let private_players = round
        .players
        .iter()
        .map(|player| {
            let seat_info = state
                .seats
                .iter()
                .find(|seat| seat.seat_index == player.seat);
            PlayerSeatView {
                seat_index: player.seat,
                nickname: seat_info.and_then(|seat| seat.nickname.clone()),
                connected: seat_info.map(|seat| seat.connected).unwrap_or(false),
                is_ready_hand: player.is_ready_hand,
                concealed_count: player.concealed_tiles.len(),
                concealed_tiles: Some(
                    player
                        .concealed_tiles
                        .iter()
                        .map(|tile| PrivateTileView {
                            tile_id: tile.tile_id.clone(),
                            tile_key: tile.tile_key.clone(),
                        })
                        .collect(),
                ),
                melds: player.melds.clone(),
                display_melds: project_display_melds_for_observer(&player.display_melds),
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
        pending_action: observer_pending_action(state),
        hand_insights: None,
        score_state: score_state_view(state),
        players: private_players,
    })
}
fn observer_pending_action(state: &RoomState) -> Option<PendingActionView> {
    let pending_timeout = state.pending_timeout.as_ref()?;
    match pending_timeout.kind.as_str() {
        "active_turn" => Some(PendingActionView::ActiveTurn {
            seat_index: pending_timeout.seat_index,
            deadline_at: pending_timeout.deadline_at.clone(),
            drawn_tile_id: None,
            restricted_discard_tile_ids: Vec::new(),
            options: Vec::new(),
        }),
        "claim_window" => {
            let round = state.round_state.as_ref()?;
            let PendingAction::ClaimWindow(claim) = round.pending_action.as_ref()? else {
                return None;
            };
            Some(PendingActionView::ClaimWindow {
                discarder_seat: claim.discarder_seat,
                deadline_at: pending_timeout.deadline_at.clone(),
                responded_seats: claim.responded_seats.clone(),
                options: Vec::new(),
            })
        }
        "rob_kong_window" => {
            let round = state.round_state.as_ref()?;
            let PendingAction::RobKongWindow(rob) = round.pending_action.as_ref()? else {
                return None;
            };
            Some(PendingActionView::RobKongWindow {
                actor_seat: rob.actor_seat,
                tile_key: rob.tile_key.clone(),
                deadline_at: pending_timeout.deadline_at.clone(),
                responded_seats: rob.responded_seats.clone(),
                options: Vec::new(),
            })
        }
        _ => None,
    }
}

fn project_melds(
    round: &RoundState,
    player_seat: Seat,
    local_seat: Seat,
    melds: &[Vec<String>],
) -> Vec<Vec<String>> {
    melds
        .iter()
        .map(|meld| {
            if should_hide_concealed_kong(round, player_seat, local_seat, meld) {
                vec![String::new(); meld.len()]
            } else {
                meld.clone()
            }
        })
        .collect()
}

fn project_display_melds(
    round: &RoundState,
    player_seat: Seat,
    local_seat: Seat,
    melds: &[Vec<String>],
    display_melds: &[DisplayMeldState],
) -> Vec<DisplayMeldView> {
    display_melds
        .iter()
        .enumerate()
        .map(|(meld_index, display_meld)| {
            let meld = melds.get(meld_index).map(Vec::as_slice).unwrap_or(&[]);
            let tiles =
                project_display_meld_tiles(round, player_seat, local_seat, meld, display_meld);
            DisplayMeldView {
                tiles: serialize_display_meld_tiles(&tiles),
            }
        })
        .collect()
}
fn project_display_melds_for_observer(display_melds: &[DisplayMeldState]) -> Vec<DisplayMeldView> {
    display_melds
        .iter()
        .map(|meld| DisplayMeldView {
            tiles: serialize_display_meld_tiles(&meld.tiles),
        })
        .collect()
}

fn project_display_meld_tiles(
    round: &RoundState,
    player_seat: Seat,
    local_seat: Seat,
    meld: &[String],
    display_meld: &DisplayMeldState,
) -> Vec<DisplayMeldTileState> {
    if !is_concealed_kong_meld(round, player_seat, meld) {
        return display_meld.tiles.clone();
    }

    if round.phase == "settlement" || player_seat == local_seat {
        return display_meld.tiles.clone();
    }

    display_meld
        .tiles
        .iter()
        .map(|_| DisplayMeldTileState {
            code: String::new(),
            orientation: DisplayMeldOrientation::FaceDown,
        })
        .collect()
}

fn should_hide_concealed_kong(
    round: &RoundState,
    player_seat: Seat,
    local_seat: Seat,
    meld: &[String],
) -> bool {
    round.phase != "settlement"
        && player_seat != local_seat
        && is_concealed_kong_meld(round, player_seat, meld)
}

fn is_concealed_kong_meld(round: &RoundState, player_seat: Seat, meld: &[String]) -> bool {
    let Some(tile_key) = repeated_kong_tile_key(meld) else {
        return false;
    };

    round.score_trackers.kong_entries.iter().any(|entry| {
        entry.actor_seat == player_seat
            && entry.kong_type == "concealed_kong"
            && entry.tile_key.as_deref() == Some(tile_key)
    })
}

fn repeated_kong_tile_key(meld: &[String]) -> Option<&str> {
    if meld.len() != 4 {
        return None;
    }
    let first = meld.first()?.as_str();
    if first.is_empty() || meld.iter().any(|tile_key| tile_key != first) {
        return None;
    }
    Some(first)
}

fn serialize_display_meld_tiles(tiles: &[DisplayMeldTileState]) -> Vec<DisplayMeldTileView> {
    tiles
        .iter()
        .map(|tile| DisplayMeldTileView {
            code: tile.code.clone(),
            orientation: match tile.orientation {
                DisplayMeldOrientation::Normal => "normal",
                DisplayMeldOrientation::Rotated => "rotated",
                DisplayMeldOrientation::UpsideDown => "upside_down",
                DisplayMeldOrientation::FaceDown => "face_down",
            },
        })
        .collect()
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
        let settlement_total = settlement
            .as_ref()
            .and_then(|value| value.score_delta.total_delta_by_seat.get(&seat))
            .copied()
            .unwrap_or(0);
        let current_delta = if settlement.is_some() {
            settlement_total
        } else {
            kong_delta_by_seat.get(&seat).copied().unwrap_or(0)
        };
        let base_score = if settlement.is_some() {
            base - settlement_total
        } else {
            base
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

#[cfg(test)]
mod tests {
    use super::observer_room_snapshot_message;
    use super::{build_pending_action_view, room_snapshot_message};
    use crate::core::state::PendingTimeout;
    use crate::core::state::{
        ClaimWindowAction, DisplayMeldOrientation, DisplayMeldState, DisplayMeldTileState,
        KongTrackerEntry, MatchState, PendingAction, PlayerRoundState, RoomState,
        RoundScoreTrackers, RoundSettlement, RoundState, SeatState, SettlementKongScoreDetailEntry,
        SettlementScoreDelta,
    };
    use crate::core::tile::Tile;
    use crate::projection::SeatProjectionSupport;
    use std::collections::BTreeMap;

    #[test]
    fn playing_snapshot_hides_other_players_concealed_kong_tile_key() {
        let state = state_with_concealed_kong("playing");
        let snapshot = room_snapshot_message(&state, 0, &SeatProjectionSupport::default());
        let remote_player = &snapshot["payload"]["private_state"]["players"][1];

        assert_eq!(
            remote_player["melds"],
            serde_json::json!([["", "", "", ""]])
        );
        assert_eq!(
            remote_player["display_melds"][0]["tiles"],
            serde_json::json!([
                {"code": "", "orientation": "face_down"},
                {"code": "", "orientation": "face_down"},
                {"code": "", "orientation": "face_down"},
                {"code": "", "orientation": "face_down"}
            ])
        );
    }

    #[test]
    fn playing_snapshot_keeps_local_concealed_kong_owner_view() {
        let state = state_with_concealed_kong("playing");
        let snapshot = room_snapshot_message(&state, 1, &SeatProjectionSupport::default());
        let local_player = &snapshot["payload"]["private_state"]["players"][1];

        assert_eq!(
            local_player["melds"],
            serde_json::json!([["t5", "t5", "t5", "t5"]])
        );
        assert_eq!(
            local_player["display_melds"][0]["tiles"],
            serde_json::json!([
                {"code": "t5", "orientation": "face_down"},
                {"code": "t5", "orientation": "normal"},
                {"code": "t5", "orientation": "normal"},
                {"code": "t5", "orientation": "face_down"}
            ])
        );
    }

    #[test]
    fn settlement_snapshot_reveals_concealed_kong_to_other_players() {
        let state = state_with_concealed_kong("settlement");
        let snapshot = room_snapshot_message(&state, 0, &SeatProjectionSupport::default());
        let remote_player = &snapshot["payload"]["private_state"]["players"][1];

        assert_eq!(
            remote_player["melds"],
            serde_json::json!([["t5", "t5", "t5", "t5"]])
        );
        assert_eq!(
            remote_player["display_melds"][0]["tiles"],
            serde_json::json!([
                {"code": "t5", "orientation": "face_down"},
                {"code": "t5", "orientation": "normal"},
                {"code": "t5", "orientation": "normal"},
                {"code": "t5", "orientation": "face_down"}
            ])
        );
    }

    #[test]
    fn score_state_projection_uses_total_settlement_delta_without_double_counting_kongs() {
        let state = RoomState {
            table_code: "ROOM42".to_string(),
            phase: "settlement".to_string(),
            mode: "normal".to_string(),
            owner_user_id: None,
            multiplier: 1,
            seats: seats(),
            match_state: Some(MatchState {
                prevailing_wind: "east".to_string(),
                hand_number: 1,
                dealer_seat: 0,
                cumulative_scores: BTreeMap::from([(0, -9), (1, 9), (2, 0), (3, 0)]),
                match_finished: false,
                last_completed_round_id: Some("round-1".to_string()),
                statistics: Default::default(),
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
                    winning_details: Vec::new(),
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

    #[test]
    fn active_turn_projection_keeps_deadline_visible_for_observers() {
        let state = RoomState {
            table_code: "ROOM42".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            owner_user_id: None,
            multiplier: 1,
            seats: seats(),
            match_state: None,
            round_state: Some(RoundState {
                round_id: "round-1".to_string(),
                dealer_seat: 0,
                round_wind: "east".to_string(),
                current_actor: 1,
                phase: "playing".to_string(),
                players: players(),
                ..Default::default()
            }),
            pending_timeout: Some(PendingTimeout {
                kind: "active_turn".to_string(),
                seat_index: 1,
                deadline_at: Some("2026-04-20T12:00:30.000Z".to_string()),
                drawn_tile_id: Some("w3#1".to_string()),
            }),
            continue_action: None,
        };

        let pending_action =
            build_pending_action_view(&state, 0, &SeatProjectionSupport::default())
                .expect("observer should see active turn");
        let snapshot = room_snapshot_message(&state, 0, &SeatProjectionSupport::default());

        assert_eq!(
            snapshot["payload"]["private_state"]["pending_action"]["type"],
            "active_turn"
        );
        assert_eq!(
            snapshot["payload"]["private_state"]["pending_action"]["seat_index"],
            1
        );
        assert_eq!(
            snapshot["payload"]["private_state"]["pending_action"]["deadline_at"],
            "2026-04-20T12:00:30.000Z"
        );
        assert_eq!(
            snapshot["payload"]["private_state"]["pending_action"]["options"],
            serde_json::json!([])
        );
        assert!(snapshot["payload"]["private_state"]["pending_action"]["drawn_tile_id"].is_null());
        assert!(
            snapshot["payload"]["private_state"]["pending_action"]["restricted_discard_tile_ids"]
                .as_array()
                .is_some_and(|items| items.is_empty())
        );
        assert_eq!(pending_action.seat_index(), Some(1));
        assert_eq!(
            pending_action.deadline_at().as_deref(),
            Some("2026-04-20T12:00:30.000Z")
        );
    }
    #[test]
    fn observer_snapshot_exposes_all_concealed_tiles_without_prompt_options() {
        let mut state = RoomState {
            table_code: "ROOM42".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            owner_user_id: None,
            multiplier: 1,
            seats: seats(),
            match_state: None,
            round_state: Some(RoundState {
                round_id: "round-1".to_string(),
                dealer_seat: 0,
                round_wind: "east".to_string(),
                current_actor: 1,
                phase: "playing".to_string(),
                players: players(),
                ..Default::default()
            }),
            pending_timeout: Some(PendingTimeout {
                kind: "active_turn".to_string(),
                seat_index: 1,
                deadline_at: Some("2026-04-20T12:00:30.000Z".to_string()),
                drawn_tile_id: Some("w3#draw".to_string()),
            }),
            continue_action: None,
        };

        let round = state.round_state.as_mut().expect("round exists");
        for player in &mut round.players {
            player.concealed_tiles = vec![Tile {
                tile_id: format!("w{}#{}", player.seat + 1, player.seat),
                tile_key: format!("w{}", player.seat + 1),
                kind: "suit".to_string(),
                ..Default::default()
            }];
        }

        let snapshot = observer_room_snapshot_message(&state);
        let players = snapshot["payload"]["private_state"]["players"]
            .as_array()
            .expect("players should serialize");

        assert!(snapshot["payload"]["local_seat"].is_null());
        assert!(snapshot["payload"]["reconnect_token"].is_null());
        assert_eq!(
            snapshot["payload"]["private_state"]["pending_action"]["options"],
            serde_json::json!([])
        );
        assert!(players.iter().all(|player| {
            player["concealed_tiles"]
                .as_array()
                .is_some_and(|tiles| tiles.len() == 1)
        }));
    }

    #[test]
    fn active_turn_projection_includes_pass_for_self_hu() {
        let state = RoomState {
            table_code: "ROOM42".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            owner_user_id: None,
            multiplier: 1,
            seats: seats(),
            match_state: None,
            round_state: Some(RoundState {
                round_id: "round-1".to_string(),
                dealer_seat: 0,
                round_wind: "east".to_string(),
                current_actor: 0,
                phase: "playing".to_string(),
                players: players(),
                ..Default::default()
            }),
            pending_timeout: Some(PendingTimeout {
                kind: "active_turn".to_string(),
                seat_index: 0,
                deadline_at: Some("2026-04-20T12:00:30.000Z".to_string()),
                drawn_tile_id: Some("w3#draw".to_string()),
            }),
            continue_action: None,
        };

        let support = SeatProjectionSupport {
            can_hu: true,
            ..Default::default()
        };
        let snapshot = room_snapshot_message(&state, 0, &support);

        assert_eq!(
            snapshot["payload"]["private_state"]["pending_action"]["options"],
            serde_json::json!(["discard", "hu", "pass"])
        );
    }

    #[test]
    fn active_turn_projection_includes_pass_for_ready_hand_self_hu() {
        let mut ready_hand_players = players();
        ready_hand_players[0].is_ready_hand = true;

        let state = RoomState {
            table_code: "ROOM42".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            owner_user_id: None,
            multiplier: 1,
            seats: seats(),
            match_state: None,
            round_state: Some(RoundState {
                round_id: "round-1".to_string(),
                dealer_seat: 0,
                round_wind: "east".to_string(),
                current_actor: 0,
                phase: "playing".to_string(),
                players: ready_hand_players,
                ..Default::default()
            }),
            pending_timeout: Some(PendingTimeout {
                kind: "active_turn".to_string(),
                seat_index: 0,
                deadline_at: Some("2026-04-20T12:00:30.000Z".to_string()),
                drawn_tile_id: Some("w3#draw".to_string()),
            }),
            continue_action: None,
        };

        let support = SeatProjectionSupport {
            can_hu: true,
            has_self_kong: true,
            ..Default::default()
        };
        let snapshot = room_snapshot_message(&state, 0, &support);

        assert_eq!(
            snapshot["payload"]["private_state"]["pending_action"]["options"],
            serde_json::json!(["hu", "kong", "pass"])
        );
    }

    #[test]
    fn active_turn_projection_includes_pass_for_ready_hand_self_kong() {
        let mut ready_hand_players = players();
        ready_hand_players[0].is_ready_hand = true;

        let state = RoomState {
            table_code: "ROOM42".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            owner_user_id: None,
            multiplier: 1,
            seats: seats(),
            match_state: None,
            round_state: Some(RoundState {
                round_id: "round-1".to_string(),
                dealer_seat: 0,
                round_wind: "east".to_string(),
                current_actor: 0,
                phase: "playing".to_string(),
                players: ready_hand_players,
                ..Default::default()
            }),
            pending_timeout: Some(PendingTimeout {
                kind: "active_turn".to_string(),
                seat_index: 0,
                deadline_at: Some("2026-04-20T12:00:30.000Z".to_string()),
                drawn_tile_id: Some("w3#draw".to_string()),
            }),
            continue_action: None,
        };

        let support = SeatProjectionSupport {
            has_self_kong: true,
            ..Default::default()
        };
        let snapshot = room_snapshot_message(&state, 0, &support);

        assert_eq!(
            snapshot["payload"]["private_state"]["pending_action"]["options"],
            serde_json::json!(["kong", "pass"])
        );
    }

    #[test]
    fn claim_window_projection_includes_pass_for_ready_hand_discard_hu() {
        let mut ready_hand_players = players();
        ready_hand_players[0].is_ready_hand = true;

        let state = RoomState {
            table_code: "ROOM42".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            owner_user_id: None,
            multiplier: 1,
            seats: seats(),
            match_state: None,
            round_state: Some(RoundState {
                round_id: "round-1".to_string(),
                dealer_seat: 0,
                round_wind: "east".to_string(),
                current_actor: 1,
                phase: "playing".to_string(),
                players: ready_hand_players,
                pending_action: Some(PendingAction::ClaimWindow(ClaimWindowAction {
                    discarder_seat: 1,
                    claim_window: vec![
                        vec!["kong".to_string(), "hu".to_string()],
                        vec![],
                        vec![],
                        vec![],
                    ],
                    responded_seats: vec![],
                    claim_responses: vec![],
                })),
                ..Default::default()
            }),
            pending_timeout: Some(PendingTimeout {
                kind: "claim_window".to_string(),
                seat_index: 1,
                deadline_at: Some("2026-04-20T12:00:30.000Z".to_string()),
                drawn_tile_id: None,
            }),
            continue_action: None,
        };

        let snapshot = room_snapshot_message(&state, 0, &SeatProjectionSupport::default());

        assert_eq!(
            snapshot["payload"]["private_state"]["pending_action"]["options"],
            serde_json::json!(["kong", "hu", "pass"])
        );
    }

    #[test]
    fn rob_kong_projection_includes_pass_for_ready_hand_hu() {
        let mut ready_hand_players = players();
        ready_hand_players[0].is_ready_hand = true;

        let state = RoomState {
            table_code: "ROOM42".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            owner_user_id: None,
            multiplier: 1,
            seats: seats(),
            match_state: None,
            round_state: Some(RoundState {
                round_id: "round-1".to_string(),
                dealer_seat: 0,
                round_wind: "east".to_string(),
                current_actor: 1,
                phase: "playing".to_string(),
                players: ready_hand_players,
                pending_action: Some(PendingAction::RobKongWindow(
                    crate::core::state::RobKongWindowAction {
                        actor_seat: 1,
                        tile_id: Some("w3#add".to_string()),
                        tile_key: Some("w3".to_string()),
                        meld_index: Some(0),
                        offered_hu_seats: vec![0],
                        responded_seats: vec![],
                        claim_responses: vec![],
                    },
                )),
                ..Default::default()
            }),
            pending_timeout: Some(PendingTimeout {
                kind: "claim_window".to_string(),
                seat_index: 0,
                deadline_at: Some("2026-04-20T12:00:30.000Z".to_string()),
                drawn_tile_id: None,
            }),
            continue_action: None,
        };

        let snapshot = room_snapshot_message(&state, 0, &SeatProjectionSupport::default());

        assert_eq!(
            snapshot["payload"]["private_state"]["pending_action"]["options"],
            serde_json::json!(["hu", "pass"])
        );
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

    fn state_with_concealed_kong(phase: &str) -> RoomState {
        let mut players = players();
        players[1].melds = vec![vec![
            "t5".to_string(),
            "t5".to_string(),
            "t5".to_string(),
            "t5".to_string(),
        ]];
        players[1].display_melds = vec![concealed_kong_display_meld("t5")];

        RoomState {
            table_code: "ROOM42".to_string(),
            phase: phase.to_string(),
            mode: "normal".to_string(),
            owner_user_id: None,
            multiplier: 1,
            seats: seats(),
            match_state: None,
            round_state: Some(RoundState {
                round_id: "round-1".to_string(),
                dealer_seat: 0,
                round_wind: "east".to_string(),
                current_actor: 1,
                phase: phase.to_string(),
                players,
                score_trackers: RoundScoreTrackers {
                    kong_entries: vec![KongTrackerEntry {
                        kong_type: "concealed_kong".to_string(),
                        actor_seat: 1,
                        payer_seats: vec![0, 2, 3],
                        tile_key: Some("t5".to_string()),
                    }],
                },
                ..Default::default()
            }),
            pending_timeout: None,
            continue_action: None,
        }
    }

    fn concealed_kong_display_meld(tile_key: &str) -> DisplayMeldState {
        DisplayMeldState {
            tiles: (0..4)
                .map(|index| DisplayMeldTileState {
                    code: tile_key.to_string(),
                    orientation: if index == 0 || index == 3 {
                        DisplayMeldOrientation::FaceDown
                    } else {
                        DisplayMeldOrientation::Normal
                    },
                })
                .collect(),
        }
    }
}
