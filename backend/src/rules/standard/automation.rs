use std::collections::HashSet;

use serde_json::Value;

use crate::bot::{self, BotAction};
use crate::core::state::{PendingAction, RoomState};
use crate::projection::bot_view::{BotClaimOption, build_bot_context_view};
use crate::room_scoring::RoomScoringCache;

use super::actions::{
    apply_discard_action, can_resolve_claim_window_timeout_locally, can_resolve_discard_locally,
    can_resolve_rob_kong_timeout_locally, claim_window_supported_locally,
    resolve_claim_window_timeout, resolve_rob_kong_timeout, rob_kong_window_supported_locally,
};
use super::flow::{apply_flower_action, apply_opening_flowers_pass};
use super::meld::{
    SelfKongKind, available_self_kongs_from_cache, claim_tile_id_options,
    seats_with_hu_candidate_for_tile,
};
use super::runtime::project_room_state;
use super::win::{can_declare_hu_with_cache, claim_window_offers_claim};

pub fn next_bot_action(room: &Value) -> Option<BotAction> {
    let state = project_room_state(room).ok()?;
    if state.phase != "playing" {
        return None;
    }
    let pending_timeout = state.pending_timeout.as_ref()?;
    let round = state.round_state.as_ref()?;
    match pending_timeout.kind.as_str() {
        "opening_flowers" => {
            let seat_index = round.current_actor;
            if !seat_is_bot(&state, seat_index) {
                return None;
            }
            let cache = RoomScoringCache::from_state(&state);
            let tile_ids = player_first_flower_tile_id_from_cache(&cache, seat_index)
                .map(|value| vec![value])
                .unwrap_or_default();
            Some(BotAction {
                seat_index,
                action_type: if tile_ids.is_empty() {
                    "pass".to_string()
                } else {
                    "flower".to_string()
                },
                tile_ids,
            })
        }
        "active_turn" => {
            let seat_index = round.current_actor;
            if !seat_is_bot(&state, seat_index) {
                return None;
            }
            let cache = RoomScoringCache::from_state(&state);
            if can_declare_hu_with_cache(room, &cache, seat_index, None, None) {
                return Some(BotAction {
                    seat_index,
                    action_type: "hu".to_string(),
                    tile_ids: vec![],
                });
            }
            if let Some(tile_id) = player_first_flower_tile_id_from_cache(&cache, seat_index) {
                return Some(BotAction {
                    seat_index,
                    action_type: "flower".to_string(),
                    tile_ids: vec![tile_id],
                });
            }
            choose_bot_active_turn_action_with_cache(room, &cache, seat_index)
        }
        "claim_window" => match round.pending_action.as_ref()? {
            PendingAction::RobKongWindow(rob) => {
                let seat_index = rob.offered_hu_seats.iter().copied().find(|seat| {
                    seat_is_bot(&state, *seat) && !rob.responded_seats.contains(seat)
                })?;
                Some(BotAction {
                    seat_index,
                    action_type: "hu".to_string(),
                    tile_ids: vec![],
                })
            }
            PendingAction::ClaimWindow(claim) => {
                let cache = RoomScoringCache::from_state(&state);
                let seat_index = claim
                    .claim_window
                    .iter()
                    .enumerate()
                    .find(|(seat, claims)| {
                        seat_is_bot(&state, *seat)
                            && !claims.is_empty()
                            && !claim.responded_seats.contains(seat)
                    })
                    .map(|(seat, _)| seat)?;
                choose_bot_claim_action_with_cache(room, &cache, seat_index)
            }
            _ => None,
        },
        _ => None,
    }
}

pub fn try_process_due_timeout(room: &mut Value) -> Option<Vec<Value>> {
    let state = project_room_state(room).ok()?;
    let pending_timeout = state.pending_timeout.as_ref()?;
    let round = state.round_state.as_ref()?;

    match pending_timeout.kind.as_str() {
        "active_turn" => {
            let seat_index = round.current_actor;
            let cache = RoomScoringCache::from_state(&state);
            let tile_id = pending_timeout
                .drawn_tile_id
                .clone()
                .or_else(|| last_concealed_tile_id_from_cache(&cache, seat_index))?;
            if !can_resolve_discard_locally(room, seat_index, &tile_id) {
                return None;
            }
            apply_discard_action(room, seat_index, &tile_id).ok()
        }
        "opening_flowers" => {
            let seat_index = round.current_actor;
            let result = if let Some(tile_id) = pending_timeout.drawn_tile_id.clone() {
                apply_flower_action(room, seat_index, &[tile_id])
            } else {
                apply_opening_flowers_pass(room, seat_index)
            };
            result.ok()
        }
        "claim_window" => {
            if claim_window_supported_locally(room)
                && can_resolve_claim_window_timeout_locally(room)
            {
                resolve_claim_window_timeout(room).ok()
            } else if rob_kong_window_supported_locally(room)
                && can_resolve_rob_kong_timeout_locally(room)
            {
                resolve_rob_kong_timeout(room).ok()
            } else {
                None
            }
        }
        _ => None,
    }
}

fn seat_is_bot(state: &RoomState, seat_index: usize) -> bool {
    state
        .seats
        .iter()
        .find(|seat| seat.seat_index == seat_index)
        .map(|seat| seat.is_bot)
        .unwrap_or(false)
}

fn player_first_flower_tile_id_from_cache(
    cache: &RoomScoringCache,
    seat_index: usize,
) -> Option<String> {
    cache
        .player(seat_index)?
        .concealed_tiles
        .iter()
        .find(|tile| tile.kind == "flower")
        .map(|tile| tile.tile_id.clone())
}

fn last_concealed_tile_id_from_cache(
    cache: &RoomScoringCache,
    seat_index: usize,
) -> Option<String> {
    cache
        .player(seat_index)?
        .concealed_tiles
        .last()
        .map(|tile| tile.tile_id.clone())
}

fn choose_bot_active_turn_action_with_cache(
    room: &Value,
    cache: &RoomScoringCache,
    seat_index: usize,
) -> Option<BotAction> {
    let self_kong_candidates = available_self_kongs_from_cache(cache, seat_index);
    let add_kong_risk_tiles = self_kong_candidates
        .iter()
        .filter(|candidate| candidate.kind == SelfKongKind::Add)
        .filter(|candidate| {
            !seats_with_hu_candidate_for_tile(room, seat_index, &candidate.tile_key).is_empty()
        })
        .map(|candidate| candidate.tile_key.clone())
        .collect::<HashSet<_>>();
    let bot_context = build_bot_context_view(
        cache,
        seat_index,
        Vec::new(),
        self_kong_candidates,
        add_kong_risk_tiles,
    )?;
    bot::choose_active_turn_action(&bot_context)
}

fn choose_bot_claim_action_with_cache(
    room: &Value,
    cache: &RoomScoringCache,
    seat_index: usize,
) -> Option<BotAction> {
    let pending_action = room
        .get("round_state")
        .and_then(|round| round.get("pending_action"))?;
    if claim_window_offers_claim(pending_action, seat_index, "hu") {
        return Some(BotAction {
            seat_index,
            action_type: "hu".to_string(),
            tile_ids: vec![],
        });
    }
    let claim_options = ["kong", "pung", "chow"]
        .into_iter()
        .filter(|claim_type| claim_window_offers_claim(pending_action, seat_index, claim_type))
        .flat_map(|claim_type| {
            claim_tile_id_options(cache, seat_index, claim_type)
                .into_iter()
                .map(move |tile_ids| BotClaimOption {
                    action_type: claim_type.to_string(),
                    tile_ids,
                })
        })
        .collect::<Vec<_>>();
    let bot_context =
        build_bot_context_view(cache, seat_index, claim_options, Vec::new(), HashSet::new())?;
    bot::choose_claim_action(&bot_context)
}
