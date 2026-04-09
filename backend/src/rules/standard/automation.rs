use std::collections::HashSet;

use serde_json::Value;

use crate::bot::{self, BotAction};
use crate::core::engine::try_handle_player_action_in_room_state;
use crate::core::state::{PendingAction, RoomState};
use crate::projection::bot_view::{BotClaimOption, build_bot_context_view};
use crate::room_scoring::RoomScoringCache;
use crate::rules::skills;

#[cfg(test)]
use super::actions::{
    apply_discard_action, can_resolve_claim_window_timeout_locally, can_resolve_discard_locally,
    can_resolve_rob_kong_timeout_locally, claim_window_supported_locally,
    resolve_claim_window_timeout, resolve_rob_kong_timeout, rob_kong_window_supported_locally,
};
#[cfg(test)]
use super::flow::{apply_flower_action, apply_opening_flowers_pass};
#[cfg(test)]
use super::meld::seats_with_hu_candidate_for_tile;
use super::meld::{
    SelfKongKind, available_self_kongs_from_cache, claim_tile_id_options,
    seats_with_hu_candidate_for_tile_in_room_state,
};
#[cfg(test)]
use super::runtime::project_room_state;
use super::win::can_declare_hu_with_cache_for_state;
#[cfg(test)]
use super::win::claim_window_offers_claim;

pub fn next_bot_action_in_room_state(room: &RoomState) -> Result<Option<BotAction>, String> {
    Ok(next_bot_action_for_state(room))
}

pub fn try_process_due_timeout_in_room_state(
    room: &mut RoomState,
) -> Result<Option<Vec<Value>>, String> {
    let pending_timeout = match room.pending_timeout.as_ref() {
        Some(timeout) => timeout.clone(),
        None => return Ok(None),
    };
    let round = match room.round_state.as_ref() {
        Some(round) => round,
        None => return Ok(None),
    };

    match pending_timeout.kind.as_str() {
        "active_turn" => {
            let seat_index = round.current_actor;
            let cache = RoomScoringCache::from_state(room);
            let tile_id = pending_timeout
                .drawn_tile_id
                .clone()
                .or_else(|| last_concealed_tile_id_from_cache(&cache, seat_index))
                .ok_or_else(|| "invalid_action".to_string())?;
            extract_emitted_messages(try_handle_player_action_in_room_state(
                room,
                seat_index,
                "discard",
                std::slice::from_ref(&tile_id),
            )?)
        }
        "opening_flowers" => {
            let seat_index = round.current_actor;
            if let Some(tile_id) = pending_timeout.drawn_tile_id {
                extract_emitted_messages(try_handle_player_action_in_room_state(
                    room,
                    seat_index,
                    "flower",
                    std::slice::from_ref(&tile_id),
                )?)
            } else {
                extract_emitted_messages(try_handle_player_action_in_room_state(
                    room,
                    seat_index,
                    "pass",
                    &[],
                )?)
            }
        }
        "claim_window" => resolve_claim_timeout_in_room_state(room),
        "skill_draft" => Ok(Some(skills::resolve_due_skill_draft_in_room_state(room)?)),
        _ => Ok(None),
    }
}

#[cfg(test)]
#[allow(dead_code)]
pub fn next_bot_action(room: &Value) -> Option<BotAction> {
    let state = project_room_state(room).ok()?;
    next_bot_action_for_state(&state)
}

#[cfg(test)]
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

#[allow(dead_code)]
#[cfg(test)]
fn choose_bot_active_turn_action_with_cache(
    room: &Value,
    cache: &RoomScoringCache,
    seat_index: usize,
) -> Option<BotAction> {
    let state = project_room_state(room).ok()?;
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
        &state,
        seat_index,
        Vec::new(),
        self_kong_candidates,
        add_kong_risk_tiles,
    )?;
    bot::choose_active_turn_action(&bot_context)
}

fn choose_bot_active_turn_action_with_cache_for_state(
    room: &RoomState,
    cache: &RoomScoringCache,
    seat_index: usize,
) -> Option<BotAction> {
    let self_kong_candidates = available_self_kongs_from_cache(cache, seat_index);
    let add_kong_risk_tiles = self_kong_candidates
        .iter()
        .filter(|candidate| candidate.kind == SelfKongKind::Add)
        .filter(|candidate| {
            !seats_with_hu_candidate_for_tile_in_room_state(room, seat_index, &candidate.tile_key)
                .is_empty()
        })
        .map(|candidate| candidate.tile_key.clone())
        .collect::<HashSet<_>>();
    let bot_context = build_bot_context_view(
        cache,
        room,
        seat_index,
        Vec::new(),
        self_kong_candidates,
        add_kong_risk_tiles,
    )?;
    bot::choose_active_turn_action(&bot_context)
}

#[allow(dead_code)]
#[cfg(test)]
fn choose_bot_claim_action_with_cache(
    room: &Value,
    cache: &RoomScoringCache,
    seat_index: usize,
) -> Option<BotAction> {
    let state = project_room_state(room).ok()?;
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
    let bot_context = build_bot_context_view(
        cache,
        &state,
        seat_index,
        claim_options,
        Vec::new(),
        HashSet::new(),
    )?;
    bot::choose_claim_action(&bot_context)
}

fn choose_bot_claim_action_with_cache_for_state(
    room: &RoomState,
    cache: &RoomScoringCache,
    seat_index: usize,
) -> Option<BotAction> {
    let claim = match room.round_state.as_ref()?.pending_action.as_ref()? {
        PendingAction::ClaimWindow(claim) => claim,
        _ => return None,
    };
    if claim
        .claim_window
        .get(seat_index)
        .is_some_and(|claims| claims.iter().any(|claim_type| claim_type == "hu"))
    {
        return Some(BotAction {
            seat_index,
            action_type: "hu".to_string(),
            tile_ids: vec![],
        });
    }
    let claim_options = ["kong", "pung", "chow"]
        .into_iter()
        .filter(|claim_type| {
            claim
                .claim_window
                .get(seat_index)
                .is_some_and(|claims| claims.iter().any(|claim| claim == claim_type))
        })
        .flat_map(|claim_type| {
            claim_tile_id_options(cache, seat_index, claim_type)
                .into_iter()
                .map(move |tile_ids| BotClaimOption {
                    action_type: claim_type.to_string(),
                    tile_ids,
                })
        })
        .collect::<Vec<_>>();
    let bot_context = build_bot_context_view(
        cache,
        room,
        seat_index,
        claim_options,
        Vec::new(),
        HashSet::new(),
    )?;
    bot::choose_claim_action(&bot_context)
}

fn next_bot_action_for_state(state: &RoomState) -> Option<BotAction> {
    if state.phase != "playing" {
        return None;
    }
    let pending_timeout = state.pending_timeout.as_ref()?;
    let round = state.round_state.as_ref()?;
    match pending_timeout.kind.as_str() {
        "opening_flowers" => {
            let seat_index = round.current_actor;
            if !seat_is_bot(state, seat_index) {
                return None;
            }
            let cache = RoomScoringCache::from_state(state);
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
            if !seat_is_bot(state, seat_index) {
                return None;
            }
            let cache = RoomScoringCache::from_state(state);
            if can_declare_hu_with_cache_for_state(state, &cache, seat_index, None, None) {
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
            choose_bot_active_turn_action_with_cache_for_state(state, &cache, seat_index)
        }
        "claim_window" => match round.pending_action.as_ref()? {
            PendingAction::RobKongWindow(rob) => {
                let seat_index = rob.offered_hu_seats.iter().copied().find(|seat| {
                    seat_is_bot(state, *seat) && !rob.responded_seats.contains(seat)
                })?;
                Some(BotAction {
                    seat_index,
                    action_type: "hu".to_string(),
                    tile_ids: vec![],
                })
            }
            PendingAction::ClaimWindow(claim) => {
                let cache = RoomScoringCache::from_state(state);
                let seat_index = claim
                    .claim_window
                    .iter()
                    .enumerate()
                    .find(|(seat, claims)| {
                        seat_is_bot(state, *seat)
                            && !claims.is_empty()
                            && !claim.responded_seats.contains(seat)
                    })
                    .map(|(seat, _)| seat)?;
                choose_bot_claim_action_with_cache_for_state(state, &cache, seat_index)
            }
            _ => None,
        },
        _ => None,
    }
}

fn resolve_claim_timeout_in_room_state(room: &mut RoomState) -> Result<Option<Vec<Value>>, String> {
    let mut emitted_messages = Vec::new();

    loop {
        let pending_action = room
            .round_state
            .as_ref()
            .and_then(|round| round.pending_action.clone());
        let Some(pending_action) = pending_action else {
            break;
        };
        let Some(seat_index) = pending_timeout_pass_seat(&pending_action) else {
            break;
        };

        let output = extract_emitted_messages(try_handle_player_action_in_room_state(
            room,
            seat_index,
            "pass",
            &[],
        )?)?;
        let Some(messages) = output else {
            return Ok(None);
        };
        emitted_messages.extend(messages);
    }

    Ok((!emitted_messages.is_empty()).then_some(emitted_messages))
}

fn pending_timeout_pass_seat(pending_action: &PendingAction) -> Option<usize> {
    match pending_action {
        PendingAction::ClaimWindow(claim) => claim
            .claim_window
            .iter()
            .enumerate()
            .find(|(seat, claims)| !claims.is_empty() && !claim.responded_seats.contains(seat))
            .map(|(seat, _)| seat),
        PendingAction::RobKongWindow(rob) => rob
            .offered_hu_seats
            .iter()
            .copied()
            .find(|seat| !rob.responded_seats.contains(seat)),
        _ => None,
    }
}

fn extract_emitted_messages(
    result: Option<Result<crate::core::engine::EngineOutput, String>>,
) -> Result<Option<Vec<Value>>, String> {
    match result {
        Some(Ok(output)) => Ok(Some(output.emitted_messages)),
        Some(Err(reason)) => Err(reason),
        None => Ok(None),
    }
}
