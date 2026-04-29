use std::collections::HashSet;

use serde_json::Value;

use crate::bot::arena::ArenaBotPolicyConfig;
use crate::bot::{self, BotAction};
use crate::core::engine::try_handle_player_action_in_room_state;
use crate::core::state::{PendingAction, RoomState};
use crate::projection::bot_view::{BotClaimOption, build_bot_context_view};
use crate::room_scoring::RoomScoringCache;

use super::actions::apply_discard_action_output_in_room_state;
#[cfg(test)]
use super::actions::{
    apply_discard_action, can_resolve_claim_window_timeout_locally, can_resolve_discard_locally,
    can_resolve_rob_kong_timeout_locally, claim_window_supported_locally,
    resolve_claim_window_timeout, resolve_rob_kong_timeout, rob_kong_window_supported_locally,
};
use super::flow::apply_flower_action_output_in_room_state;
#[cfg(test)]
use super::meld::seats_with_hu_candidate_for_tile;
use super::meld::{
    SelfKongKind, available_self_kongs_from_cache, claim_tile_id_options,
    seats_with_hu_candidate_for_tile_in_room_state,
};
#[cfg(test)]
use super::runtime::project_room_state;
#[cfg(test)]
use super::win::claim_window_offers_claim;
use super::win::{
    apply_hu_action_output_in_room_state, can_declare_hu_with_cache_for_state,
    hu_meets_bot_minimum_fan_for_state,
};

const MAX_SEATS: usize = 4;
type BotPolicyResolver<'a> = &'a dyn Fn(usize) -> ArenaBotPolicyConfig;

pub(crate) struct BotDecisionTrace {
    pub(crate) decision_kind: String,
    pub(crate) context: crate::bot::context::BotContext,
    pub(crate) action: BotAction,
}

pub fn next_bot_action_in_room_state(room: &RoomState) -> Result<Option<BotAction>, String> {
    Ok(next_bot_action_for_state(room))
}

pub fn next_bot_action_in_room_state_with_policy_resolver(
    room: &RoomState,
    policy_for_seat: BotPolicyResolver<'_>,
) -> Result<Option<BotAction>, String> {
    Ok(next_bot_action_for_state_with_policy_resolver(
        room,
        policy_for_seat,
    ))
}

pub(crate) fn next_bot_decision_trace_in_room_state_with_policy_resolver(
    room: &RoomState,
    policy_for_seat: BotPolicyResolver<'_>,
) -> Result<Option<BotDecisionTrace>, String> {
    Ok(next_bot_decision_trace_for_state_with_policy_resolver(
        room,
        policy_for_seat,
    ))
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
            if player_is_ready_hand(room, seat_index) {
                if let Some(tile_id) = player_first_flower_tile_id_from_cache(&cache, seat_index) {
                    return apply_flower_action_output_in_room_state(room, seat_index, &[tile_id])
                        .map(|output| Some(output.emitted_messages));
                }
                if can_declare_hu_with_cache_for_state(room, &cache, seat_index, None, None)
                    && (!seat_is_bot(room, seat_index)
                        || hu_meets_bot_minimum_fan_for_state(room, seat_index, "self_draw"))
                {
                    return apply_hu_action_output_in_room_state(room, seat_index)
                        .map(|output| Some(output.emitted_messages));
                }
                let restricted_tile_key = room
                    .round_state
                    .as_ref()
                    .and_then(|round| round.restricted_discard_tile_key.as_deref());
                let tile_id = timeout_discard_tile_id_from_cache(
                    &cache,
                    pending_timeout.drawn_tile_id.as_deref(),
                    restricted_tile_key,
                    seat_index,
                )
                .ok_or_else(|| "invalid_action".to_string())?;
                return apply_discard_action_output_in_room_state(room, seat_index, &tile_id)
                    .map(|output| Some(output.emitted_messages));
            }
            let restricted_tile_key = room
                .round_state
                .as_ref()
                .and_then(|round| round.restricted_discard_tile_key.as_deref());
            let tile_id = timeout_discard_tile_id_from_cache(
                &cache,
                pending_timeout.drawn_tile_id.as_deref(),
                restricted_tile_key,
                seat_index,
            )
            .ok_or_else(|| "invalid_action".to_string())?;
            extract_emitted_messages(try_handle_player_action_in_room_state(
                room,
                seat_index,
                "discard",
                std::slice::from_ref(&tile_id),
            )?)
        }
        "claim_window" => resolve_claim_timeout_in_room_state(room),
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
            let restricted_tile_key = state
                .round_state
                .as_ref()
                .and_then(|round| round.restricted_discard_tile_key.as_deref());
            let tile_id = timeout_discard_tile_id_from_cache(
                &cache,
                pending_timeout.drawn_tile_id.as_deref(),
                restricted_tile_key,
                seat_index,
            )?;
            if !can_resolve_discard_locally(room, seat_index, &tile_id) {
                return None;
            }
            apply_discard_action(room, seat_index, &tile_id).ok()
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

fn player_is_ready_hand(state: &RoomState, seat_index: usize) -> bool {
    state
        .round_state
        .as_ref()
        .and_then(|round| round.players.get(seat_index))
        .is_some_and(|player| player.is_ready_hand)
}

fn ready_hand_human_waits_for_manual_action(
    state: &RoomState,
    cache: &RoomScoringCache,
    seat_index: usize,
) -> bool {
    player_is_ready_hand(state, seat_index)
        && !seat_is_bot(state, seat_index)
        && (can_declare_hu_with_cache_for_state(state, cache, seat_index, None, None)
            || !available_self_kongs_from_cache(cache, seat_index).is_empty())
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

fn last_legal_concealed_tile_id_from_cache(
    cache: &RoomScoringCache,
    restricted_tile_key: Option<&str>,
    seat_index: usize,
) -> Option<String> {
    cache
        .player(seat_index)?
        .concealed_tiles
        .iter()
        .rev()
        .find(|tile| Some(tile.tile_key.as_str()) != restricted_tile_key)
        .map(|tile| tile.tile_id.clone())
}

fn timeout_discard_tile_id_from_cache(
    cache: &RoomScoringCache,
    pending_drawn_tile_id: Option<&str>,
    restricted_tile_key: Option<&str>,
    seat_index: usize,
) -> Option<String> {
    let player = cache.player(seat_index)?;

    if let Some(tile_id) = pending_drawn_tile_id {
        let valid_drawn_tile = player
            .concealed_tiles
            .iter()
            .find(|tile| {
                tile.tile_id == tile_id && Some(tile.tile_key.as_str()) != restricted_tile_key
            })
            .map(|tile| tile.tile_id.clone());
        if valid_drawn_tile.is_some() {
            return valid_drawn_tile;
        }
    }

    last_legal_concealed_tile_id_from_cache(cache, restricted_tile_key, seat_index)
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
    policy_config: &ArenaBotPolicyConfig,
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
    bot::choose_active_turn_action_with_config(&bot_context, policy_config)
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
    policy_config: &ArenaBotPolicyConfig,
) -> Option<BotAction> {
    let claim = match room.round_state.as_ref()?.pending_action.as_ref()? {
        PendingAction::ClaimWindow(claim) => claim,
        _ => return None,
    };
    if claim
        .claim_window
        .get(seat_index)
        .is_some_and(|claims| claims.iter().any(|claim_type| claim_type == "hu"))
        && hu_meets_bot_minimum_fan_for_state(room, seat_index, "discard")
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
    bot::choose_claim_action_with_config(&bot_context, policy_config)
}

fn next_ready_hand_action_for_state(
    state: &RoomState,
    cache: &RoomScoringCache,
    seat_index: usize,
    policy_config: &ArenaBotPolicyConfig,
) -> Option<BotAction> {
    if let Some(tile_id) = player_first_flower_tile_id_from_cache(cache, seat_index) {
        return Some(BotAction {
            seat_index,
            action_type: "flower".to_string(),
            tile_ids: vec![tile_id],
        });
    }

    if can_declare_hu_with_cache_for_state(state, cache, seat_index, None, None)
        && (!seat_is_bot(state, seat_index)
            || hu_meets_bot_minimum_fan_for_state(state, seat_index, "self_draw"))
    {
        return Some(BotAction {
            seat_index,
            action_type: "hu".to_string(),
            tile_ids: vec![],
        });
    }

    if seat_is_bot(state, seat_index) {
        if let Some(action) = choose_bot_active_turn_action_with_cache_for_state(
            state,
            cache,
            seat_index,
            policy_config,
        ) {
            return Some(action);
        }
    }

    let tile_id = state
        .pending_timeout
        .as_ref()
        .and_then(|timeout| timeout.drawn_tile_id.clone())
        .or_else(|| {
            let restricted_tile_key = state
                .round_state
                .as_ref()
                .and_then(|round| round.restricted_discard_tile_key.as_deref());
            last_legal_concealed_tile_id_from_cache(cache, restricted_tile_key, seat_index)
        })?;
    Some(BotAction {
        seat_index,
        action_type: "discard".to_string(),
        tile_ids: vec![tile_id],
    })
}

fn next_bot_action_for_state(state: &RoomState) -> Option<BotAction> {
    next_bot_action_for_state_with_policy_resolver(state, &|_| bot::bot_policy_config_from_env())
}

fn next_bot_action_for_state_with_policy_resolver(
    state: &RoomState,
    policy_for_seat: BotPolicyResolver<'_>,
) -> Option<BotAction> {
    if state.phase != "playing" {
        return None;
    }
    let pending_timeout = state.pending_timeout.as_ref()?;
    let round = state.round_state.as_ref()?;
    match pending_timeout.kind.as_str() {
        "active_turn" => {
            let seat_index = round.current_actor;
            let cache = RoomScoringCache::from_state(state);
            let policy_config = policy_for_seat(seat_index);
            if player_is_ready_hand(state, seat_index) {
                if ready_hand_human_waits_for_manual_action(state, &cache, seat_index) {
                    return None;
                }
                return next_ready_hand_action_for_state(state, &cache, seat_index, &policy_config);
            }
            if !seat_is_bot(state, seat_index) {
                return None;
            }
            if can_declare_hu_with_cache_for_state(state, &cache, seat_index, None, None)
                && hu_meets_bot_minimum_fan_for_state(state, seat_index, "self_draw")
            {
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
            choose_bot_active_turn_action_with_cache_for_state(
                state,
                &cache,
                seat_index,
                &policy_config,
            )
        }
        "claim_window" => match round.pending_action.as_ref()? {
            PendingAction::RobKongWindow(rob) => {
                let seat_index =
                    next_rob_kong_responder_seat(rob).filter(|seat| seat_is_bot(state, *seat))?;
                if !hu_meets_bot_minimum_fan_for_state(state, seat_index, "discard") {
                    return Some(BotAction {
                        seat_index,
                        action_type: "pass".to_string(),
                        tile_ids: vec![],
                    });
                }
                Some(BotAction {
                    seat_index,
                    action_type: "hu".to_string(),
                    tile_ids: vec![],
                })
            }
            PendingAction::ClaimWindow(claim) => {
                let cache = RoomScoringCache::from_state(state);
                let seat_index = next_claim_window_responder_seat(claim)
                    .filter(|seat| seat_is_bot(state, *seat))?;
                let policy_config = policy_for_seat(seat_index);
                choose_bot_claim_action_with_cache_for_state(
                    state,
                    &cache,
                    seat_index,
                    &policy_config,
                )
            }
        },
        _ => None,
    }
}

fn next_bot_decision_trace_for_state_with_policy_resolver(
    state: &RoomState,
    policy_for_seat: BotPolicyResolver<'_>,
) -> Option<BotDecisionTrace> {
    if state.phase != "playing" {
        return None;
    }
    let pending_timeout = state.pending_timeout.as_ref()?;
    let round = state.round_state.as_ref()?;
    match pending_timeout.kind.as_str() {
        "active_turn" => {
            let seat_index = round.current_actor;
            if !seat_is_bot(state, seat_index)
                || player_is_ready_hand(state, seat_index)
                || can_declare_hu_with_cache_for_state(
                    state,
                    &RoomScoringCache::from_state(state),
                    seat_index,
                    None,
                    None,
                )
            {
                return None;
            }
            let cache = RoomScoringCache::from_state(state);
            if player_first_flower_tile_id_from_cache(&cache, seat_index).is_some() {
                return None;
            }
            let self_kong_candidates = available_self_kongs_from_cache(&cache, seat_index);
            let add_kong_risk_tiles = self_kong_candidates
                .iter()
                .filter(|candidate| candidate.kind == SelfKongKind::Add)
                .filter(|candidate| {
                    !seats_with_hu_candidate_for_tile_in_room_state(
                        state,
                        seat_index,
                        &candidate.tile_key,
                    )
                    .is_empty()
                })
                .map(|candidate| candidate.tile_key.clone())
                .collect::<HashSet<_>>();
            let context = build_bot_context_view(
                &cache,
                state,
                seat_index,
                Vec::new(),
                self_kong_candidates,
                add_kong_risk_tiles,
            )?;
            let policy_config = policy_for_seat(seat_index);
            let action = bot::choose_active_turn_action_with_config(&context, &policy_config)?;
            Some(BotDecisionTrace {
                decision_kind: "active_turn".to_string(),
                context,
                action,
            })
        }
        "claim_window" => match round.pending_action.as_ref()? {
            PendingAction::ClaimWindow(claim) => {
                let cache = RoomScoringCache::from_state(state);
                let seat_index = next_claim_window_responder_seat(claim)
                    .filter(|seat| seat_is_bot(state, *seat))?;
                if claim
                    .claim_window
                    .get(seat_index)
                    .is_some_and(|claims| claims.iter().any(|claim_type| claim_type == "hu"))
                    && hu_meets_bot_minimum_fan_for_state(state, seat_index, "discard")
                {
                    return None;
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
                        claim_tile_id_options(&cache, seat_index, claim_type)
                            .into_iter()
                            .map(move |tile_ids| BotClaimOption {
                                action_type: claim_type.to_string(),
                                tile_ids,
                            })
                    })
                    .collect::<Vec<_>>();
                let context = build_bot_context_view(
                    &cache,
                    state,
                    seat_index,
                    claim_options,
                    Vec::new(),
                    HashSet::new(),
                )?;
                let policy_config = policy_for_seat(seat_index);
                let action = bot::choose_claim_action_with_config(&context, &policy_config)?;
                Some(BotDecisionTrace {
                    decision_kind: "claim_window".to_string(),
                    context,
                    action,
                })
            }
            PendingAction::RobKongWindow(_) => None,
        },
        _ => None,
    }
}

fn resolve_claim_timeout_in_room_state(room: &mut RoomState) -> Result<Option<Vec<Value>>, String> {
    let mut emitted_messages = Vec::new();
    let mut processed_any = false;

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
        processed_any = true;
        emitted_messages.extend(messages);
    }

    Ok(processed_any.then_some(emitted_messages))
}

fn pending_timeout_pass_seat(pending_action: &PendingAction) -> Option<usize> {
    match pending_action {
        PendingAction::ClaimWindow(claim) => next_claim_window_responder_seat(claim),
        PendingAction::RobKongWindow(rob) => next_rob_kong_responder_seat(rob),
    }
}

fn next_claim_window_responder_seat(
    claim: &crate::core::state::ClaimWindowAction,
) -> Option<usize> {
    response_order_from(claim.discarder_seat).find(|seat| {
        claim
            .claim_window
            .get(*seat)
            .is_some_and(|claims| !claims.is_empty())
            && !claim.responded_seats.contains(seat)
    })
}

fn next_rob_kong_responder_seat(rob: &crate::core::state::RobKongWindowAction) -> Option<usize> {
    response_order_from(rob.actor_seat)
        .find(|seat| rob.offered_hu_seats.contains(seat) && !rob.responded_seats.contains(seat))
}

fn response_order_from(origin_seat: usize) -> impl Iterator<Item = usize> {
    (1..MAX_SEATS).map(move |offset| (origin_seat + offset) % MAX_SEATS)
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{next_bot_action_in_room_state, try_process_due_timeout_in_room_state};
    use crate::core::engine::try_handle_player_action_in_room_state;
    use crate::core::state::RoomState;

    fn suit(tile_key: &str, tile_id: &str) -> serde_json::Value {
        json!({
            "tile_id": tile_id,
            "tile_key": tile_key,
            "kind": "suit",
            "suit": if tile_key.starts_with('w') {
                "characters"
            } else if tile_key.starts_with('t') {
                "bamboos"
            } else {
                "dots"
            },
            "rank": tile_key[1..].parse::<u8>().unwrap_or_default(),
            "name": tile_key,
        })
    }

    fn wind(tile_key: &str, tile_id: &str) -> serde_json::Value {
        json!({
            "tile_id": tile_id,
            "tile_key": tile_key,
            "kind": "wind",
            "suit": null,
            "rank": null,
            "name": tile_key,
        })
    }

    fn flower(tile_key: &str, tile_id: &str) -> serde_json::Value {
        json!({
            "tile_id": tile_id,
            "tile_key": tile_key,
            "kind": "flower",
            "suit": null,
            "rank": null,
            "name": tile_key,
        })
    }

    fn claim_window_room_state() -> RoomState {
        RoomState::from_room_value(&json!({
            "table_code": "ROOM2",
            "phase": "playing",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "continue_action": null,
            "seats": [
                {"seat_index": 0, "nickname": "P0", "reconnect_token": "t0", "player_session_id": 1, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null},
                {"seat_index": 1, "nickname": "P1", "reconnect_token": "t1", "player_session_id": 2, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null},
                {"seat_index": 2, "nickname": "P2", "reconnect_token": "t2", "player_session_id": 3, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null},
                {"seat_index": 3, "nickname": "P3", "reconnect_token": "t3", "player_session_id": 4, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null}
            ],
            "match_state": {
                "prevailing_wind": "east",
                "hand_number": 1,
                "dealer_seat": 0,
                "cumulative_scores": {"0": 0, "1": 0, "2": 0, "3": 0},
                "match_finished": false,
                "last_completed_round_id": null
            },
            "round_state": {
                "round_id": "east-1-dealer-0-claim",
                "dealer_seat": 0,
                "current_actor": 0,
                "wall": {
                    "tiles": [suit("w9", "w9#draw")],
                    "head_index": 0,
                    "tail_index": 0
                },
                "players": [
                    {
                        "seat": 0,
                        "concealed_tiles": [
                            suit("w3", "w3#discard"),
                            suit("w2", "w2#a"), suit("w4", "w4#a"), suit("t2", "t2#a"),
                            suit("t3", "t3#a"), suit("t4", "t4#a"), suit("b2", "b2#a"),
                            suit("b3", "b3#a"), suit("b4", "b4#a"), suit("w6", "w6#a"),
                            suit("w7", "w7#a"), suit("w8", "w8#a"), suit("b7", "b7#a"), suit("b8", "b8#a")
                        ],
                        "melds": [],
                        "flowers": [],
                        "discards": []
                    },
                    {
                        "seat": 1,
                        "concealed_tiles": [
                            suit("w1", "w1#1"), suit("w2", "w2#1"), suit("w4", "w4#1"),
                            suit("t1", "t1#1"), suit("t2", "t2#1"), suit("t3", "t3#1"),
                            suit("b1", "b1#1"), suit("b2", "b2#1"), suit("b3", "b3#1"),
                            suit("w5", "w5#1"), suit("w6", "w6#1"), suit("t6", "t6#1"), suit("b6", "b6#1")
                        ],
                        "melds": [],
                        "flowers": [],
                        "discards": []
                    },
                    {
                        "seat": 2,
                        "concealed_tiles": [
                            suit("w3", "w3#2a"), suit("w3", "w3#2b"),
                            suit("t1", "t1#2"), suit("t4", "t4#2"), suit("t7", "t7#2"),
                            suit("b1", "b1#2"), suit("b4", "b4#2"), suit("b7", "b7#2"),
                            suit("w9", "w9#2"), suit("t9", "t9#2"), suit("b9", "b9#2"), wind("south", "south#2"), wind("north", "north#2")
                        ],
                        "melds": [],
                        "flowers": [],
                        "discards": []
                    },
                    {
                        "seat": 3,
                        "concealed_tiles": [
                            suit("w1", "w1#3"), suit("w5", "w5#3"), suit("w7", "w7#3"),
                            suit("t3", "t3#3"), suit("t5", "t5#3"), suit("t7", "t7#3"),
                            suit("b3", "b3#3"), suit("b5", "b5#3"), suit("b7", "b7#3"),
                            suit("w9", "w9#3"), suit("t9", "t9#3"), suit("b9", "b9#3"), wind("north", "north#3")
                        ],
                        "melds": [],
                        "flowers": [],
                        "discards": []
                    }
                ],
                "last_discard": null,
                "pending_action": null,
                "phase": "playing",
                "settlement": null,
                "version": 1,
                "score_trackers": {"kong_entries": []},
                "last_action_context": {
                    "kind": "draw",
                    "seat": 0,
                    "tile_id": "w3#discard",
                    "from_kong_replacement": false,
                    "was_last_live_tile": false,
                    "was_last_discard": false
                },
                "round_wind": "east",
                "enforce_minimum_eight_fan": true,
                "restricted_discard_tile_key": null
            },
            "pending_timeout": {
                "kind": "active_turn",
                "seat_index": 0,
                "deadline_at": "2026-04-07T00:00:30Z",
                "drawn_tile_id": "w3#discard"
            }
        }))
        .expect("room should parse")
    }

    fn claim_window_priority_room_state() -> RoomState {
        RoomState::from_room_value(&json!({
            "table_code": "ROOM3",
            "phase": "playing",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "continue_action": null,
            "seats": [
                {"seat_index": 0, "nickname": "P0", "reconnect_token": "t0", "player_session_id": 1, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null},
                {"seat_index": 1, "nickname": "Bot 1", "reconnect_token": "t1", "player_session_id": 2, "connected": true, "ready": true, "is_bot": true, "seat_type": "bot", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null},
                {"seat_index": 2, "nickname": "P2", "reconnect_token": "t2", "player_session_id": 3, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null},
                {"seat_index": 3, "nickname": "P3", "reconnect_token": "t3", "player_session_id": 4, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null}
            ],
            "match_state": {
                "prevailing_wind": "east",
                "hand_number": 1,
                "dealer_seat": 0,
                "cumulative_scores": {"0": 0, "1": 0, "2": 0, "3": 0},
                "match_finished": false,
                "last_completed_round_id": null
            },
            "round_state": {
                "round_id": "east-1-dealer-0-priority",
                "dealer_seat": 0,
                "current_actor": 3,
                "wall": {
                    "tiles": [suit("w9", "w9#draw")],
                    "head_index": 0,
                    "tail_index": 0
                },
                "players": [
                    {
                        "seat": 0,
                        "concealed_tiles": [suit("w1", "w1#0")],
                        "melds": [],
                        "flowers": [],
                        "discards": []
                    },
                    {
                        "seat": 1,
                        "concealed_tiles": [
                            suit("w1", "w1#1a"), suit("w1", "w1#1b"),
                            suit("w2", "w2#1a"), suit("w2", "w2#1b"),
                            suit("w3", "w3#1a"),
                            suit("t4", "t4#1a"), suit("t4", "t4#1b"),
                            suit("t5", "t5#1a"), suit("t5", "t5#1b"),
                            suit("b6", "b6#1a"), suit("b6", "b6#1b"),
                            wind("red", "red#1a"), wind("red", "red#1b")
                        ],
                        "melds": [],
                        "flowers": [],
                        "discards": []
                    },
                    {
                        "seat": 2,
                        "concealed_tiles": [suit("w2", "w2#2")],
                        "melds": [],
                        "flowers": [],
                        "discards": []
                    },
                    {
                        "seat": 3,
                        "concealed_tiles": [suit("w3", "w3#discard")],
                        "melds": [],
                        "flowers": [],
                        "discards": [suit("w3", "w3#discard")]
                    }
                ],
                "last_discard": suit("w3", "w3#discard"),
                "pending_action": {
                    "type": "claim_window",
                    "discarder_seat": 3,
                    "claim_window": [
                        ["hu"],
                        ["hu"],
                        [],
                        []
                    ],
                    "responded_seats": [],
                    "claim_responses": []
                },
                "phase": "playing",
                "settlement": null,
                "version": 1,
                "score_trackers": {"kong_entries": []},
                "last_action_context": {
                    "kind": "discard",
                    "seat": 3,
                    "tile_id": "w3#discard",
                    "from_kong_replacement": false,
                    "was_last_live_tile": false,
                    "was_last_discard": false
                },
                "round_wind": "east",
                "enforce_minimum_eight_fan": true,
                "restricted_discard_tile_key": null
            },
            "pending_timeout": {
                "kind": "claim_window",
                "seat_index": 3,
                "deadline_at": "2026-04-07T00:00:30Z",
                "drawn_tile_id": null
            }
        }))
        .expect("room should parse")
    }

    #[test]
    fn claim_window_bot_waits_for_earlier_human_hu_response() {
        let mut room = claim_window_priority_room_state();

        assert!(
            next_bot_action_in_room_state(&room)
                .expect("bot lookup should succeed")
                .is_none()
        );

        let _ = try_handle_player_action_in_room_state(&mut room, 0, "pass", &[])
            .expect("pass should be handled")
            .expect("pass should succeed");

        let action = next_bot_action_in_room_state(&room)
            .expect("bot lookup should succeed")
            .expect("bot should act after earlier human response");
        assert_eq!(action.seat_index, 1);
        assert_eq!(action.action_type, "hu");
    }

    #[test]
    fn claim_window_bot_passes_low_fan_hu() {
        let mut room = claim_window_priority_room_state();
        let round = room.round_state.as_mut().expect("round should exist");
        round.players[1].concealed_tiles = serde_json::from_value(json!([
            suit("w1", "w1#1"),
            suit("w2", "w2#1"),
            suit("t4", "t4#1"),
            suit("t5", "t5#1"),
            suit("t6", "t6#1"),
            suit("b3", "b3#1"),
            suit("b4", "b4#1"),
            suit("b5", "b5#1"),
            suit("w6", "w6#1"),
            suit("w7", "w7#1"),
            suit("w8", "w8#1"),
            wind("red", "red#1a"),
            wind("red", "red#1b")
        ]))
        .expect("tiles should parse");
        if let Some(claim) = round.pending_action.as_mut() {
            if let crate::core::state::PendingAction::ClaimWindow(claim) = claim {
                claim.responded_seats.push(0);
            }
        }

        let action = next_bot_action_in_room_state(&room)
            .expect("bot lookup should succeed")
            .expect("bot should respond");

        assert_eq!(action.seat_index, 1);
        assert_eq!(action.action_type, "pass");
    }

    #[test]
    fn claim_timeout_advances_human_only_room_even_without_round_events() {
        let mut room = claim_window_room_state();
        let _ = try_handle_player_action_in_room_state(
            &mut room,
            0,
            "discard",
            &[String::from("w3#discard")],
        )
        .expect("discard should be handled")
        .expect("discard should succeed");

        let result =
            try_process_due_timeout_in_room_state(&mut room).expect("claim timeout should work");

        assert!(result.is_some());
        assert!(result.as_ref().is_some_and(Vec::is_empty));
        assert_eq!(
            room.round_state.as_ref().map(|round| round.current_actor),
            Some(1)
        );
        assert!(
            room.round_state
                .as_ref()
                .and_then(|round| round.pending_action.as_ref())
                .is_none()
        );
        assert_eq!(
            room.pending_timeout
                .as_ref()
                .map(|timeout| timeout.kind.as_str()),
            Some("active_turn")
        );
        assert_eq!(
            room.pending_timeout
                .as_ref()
                .map(|timeout| timeout.seat_index),
            Some(1)
        );
    }

    #[test]
    fn claim_timeout_still_advances_when_next_player_is_ready_hand() {
        let mut room = claim_window_room_state();
        room.round_state
            .as_mut()
            .and_then(|round| round.players.get_mut(1))
            .expect("seat 1 should exist")
            .is_ready_hand = true;

        let _ = try_handle_player_action_in_room_state(
            &mut room,
            0,
            "discard",
            &[String::from("w3#discard")],
        )
        .expect("discard should be handled")
        .expect("discard should succeed");

        let result = try_process_due_timeout_in_room_state(&mut room);

        assert!(
            result.is_ok(),
            "claim timeout should not fail for ready-hand seats"
        );
        assert!(result.expect("claim timeout should work").is_some());
        assert_eq!(
            room.round_state.as_ref().map(|round| round.current_actor),
            Some(1)
        );
        assert!(
            room.round_state
                .as_ref()
                .and_then(|round| round.pending_action.as_ref())
                .is_none()
        );
    }

    #[test]
    fn claim_timeout_can_finish_after_recorded_chow_response() {
        let mut room = claim_window_room_state();
        room.round_state
            .as_mut()
            .and_then(|round| round.players.get_mut(1))
            .expect("seat 1 should exist")
            .concealed_tiles = vec![
            crate::core::tile::Tile::from_value(&suit("w3", "w3#1extra"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("w2", "w2#1"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("w4", "w4#1"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("t1", "t1#1"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("t2", "t2#1"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("t3", "t3#1"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("b1", "b1#1"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("b2", "b2#1"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("b3", "b3#1"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("w5", "w5#1"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("w6", "w6#1"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("t6", "t6#1"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("b6", "b6#1"), "tile").expect("tile"),
        ];
        let _ = try_handle_player_action_in_room_state(
            &mut room,
            0,
            "discard",
            &[String::from("w3#discard")],
        )
        .expect("discard should be handled")
        .expect("discard should succeed");
        let _ = try_handle_player_action_in_room_state(
            &mut room,
            1,
            "chow",
            &[String::from("w2#1"), String::from("w4#1")],
        )
        .expect("chow should be handled")
        .expect("chow should succeed");

        let result =
            try_process_due_timeout_in_room_state(&mut room).expect("claim timeout should work");

        assert!(result.is_some());
        assert_eq!(
            room.round_state.as_ref().map(|round| round.current_actor),
            Some(1)
        );
        assert!(
            room.round_state
                .as_ref()
                .and_then(|round| round.pending_action.as_ref())
                .is_none()
        );
        assert_eq!(
            room.pending_timeout
                .as_ref()
                .map(|timeout| timeout.kind.as_str()),
            Some("active_turn")
        );
    }

    #[test]
    fn active_turn_timeout_after_chow_claim_skips_restricted_tile() {
        let mut room = claim_window_room_state();
        room.round_state
            .as_mut()
            .and_then(|round| round.players.get_mut(1))
            .expect("seat 1 should exist")
            .concealed_tiles = vec![
            crate::core::tile::Tile::from_value(&suit("w2", "w2#1"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("w4", "w4#1"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("t1", "t1#1"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("t2", "t2#1"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("t3", "t3#1"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("b1", "b1#1"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("b2", "b2#1"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("b3", "b3#1"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("w5", "w5#1"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("w6", "w6#1"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("t6", "t6#1"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("b6", "b6#1"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("w3", "w3#1extra"), "tile").expect("tile"),
        ];
        let _ = try_handle_player_action_in_room_state(
            &mut room,
            0,
            "discard",
            &[String::from("w3#discard")],
        )
        .expect("discard should be handled")
        .expect("discard should succeed");
        let _ = try_handle_player_action_in_room_state(
            &mut room,
            1,
            "chow",
            &[String::from("w2#1"), String::from("w4#1")],
        )
        .expect("chow should be handled")
        .expect("chow should succeed");
        let claim_timeout_result = try_process_due_timeout_in_room_state(&mut room)
            .expect("claim timeout should advance to the chow seat");
        assert!(claim_timeout_result.is_some());
        assert_eq!(
            room.pending_timeout
                .as_ref()
                .map(|timeout| timeout.kind.as_str()),
            Some("active_turn")
        );
        assert_eq!(
            room.pending_timeout
                .as_ref()
                .and_then(|timeout| timeout.drawn_tile_id.as_deref()),
            None
        );

        let result = try_process_due_timeout_in_room_state(&mut room)
            .expect("active turn timeout should resolve with a legal fallback discard");

        let emitted = result.expect("timeout should emit discard message");
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0]["payload"]["event_type"], "tile_discarded");
        assert_eq!(emitted[0]["payload"]["event"]["tile_id"], "b6#1");
        assert_eq!(
            room.round_state
                .as_ref()
                .and_then(|round| round.players.get(1))
                .map(|player| { player.discards.last().map(|tile| tile.tile_id.as_str()) }),
            Some(Some("b6#1"))
        );
        assert_eq!(
            room.round_state
                .as_ref()
                .and_then(|round| round.players.get(1))
                .map(|player| {
                    player
                        .concealed_tiles
                        .iter()
                        .any(|tile| tile.tile_id == "w3#1extra")
                }),
            Some(true)
        );
    }

    #[test]
    fn active_turn_timeout_after_pung_claim_skips_restricted_tile() {
        let mut room = claim_window_room_state();
        room.round_state
            .as_mut()
            .and_then(|round| round.players.get_mut(2))
            .expect("seat 2 should exist")
            .concealed_tiles = vec![
            crate::core::tile::Tile::from_value(&suit("w3", "w3#2a"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("w3", "w3#2b"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("t1", "t1#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("t4", "t4#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("t7", "t7#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("b1", "b1#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("b4", "b4#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("b7", "b7#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("w9", "w9#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("t9", "t9#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("b9", "b9#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&wind("south", "south#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("w3", "w3#2extra"), "tile").expect("tile"),
        ];
        let _ = try_handle_player_action_in_room_state(
            &mut room,
            0,
            "discard",
            &[String::from("w3#discard")],
        )
        .expect("discard should be handled")
        .expect("discard should succeed");
        let _ = try_handle_player_action_in_room_state(
            &mut room,
            2,
            "pung",
            &[String::from("w3#2a"), String::from("w3#2b")],
        )
        .expect("pung should be handled")
        .expect("pung should succeed");

        assert_eq!(
            room.pending_timeout
                .as_ref()
                .map(|timeout| timeout.kind.as_str()),
            Some("active_turn")
        );
        assert_eq!(
            room.round_state.as_ref().map(|round| round.current_actor),
            Some(2)
        );

        let emitted = try_process_due_timeout_in_room_state(&mut room)
            .expect("active turn timeout should resolve after pung claim")
            .expect("timeout should emit discard message");

        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0]["payload"]["event_type"], "tile_discarded");
        assert_ne!(emitted[0]["payload"]["event"]["tile_key"], "w3");
        assert_eq!(
            room.round_state
                .as_ref()
                .and_then(|round| round.players.get(2))
                .map(|player| {
                    player
                        .concealed_tiles
                        .iter()
                        .any(|tile| tile.tile_id == "w3#2extra")
                }),
            Some(true)
        );
    }

    #[test]
    fn active_turn_timeout_after_claim_kong_discards_replacement_tile() {
        let mut room = claim_window_room_state();
        room.round_state
            .as_mut()
            .and_then(|round| round.players.get_mut(2))
            .expect("seat 2 should exist")
            .concealed_tiles = vec![
            crate::core::tile::Tile::from_value(&suit("w3", "w3#2a"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("w3", "w3#2b"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("w3", "w3#2c"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("t1", "t1#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("t4", "t4#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("t7", "t7#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("b1", "b1#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("b4", "b4#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("b7", "b7#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("w9", "w9#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("t9", "t9#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("b9", "b9#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&wind("south", "south#2"), "tile").expect("tile"),
        ];
        let _ = try_handle_player_action_in_room_state(
            &mut room,
            0,
            "discard",
            &[String::from("w3#discard")],
        )
        .expect("discard should be handled")
        .expect("discard should succeed");
        let _ = try_handle_player_action_in_room_state(
            &mut room,
            2,
            "kong",
            &[
                String::from("w3#2a"),
                String::from("w3#2b"),
                String::from("w3#2c"),
            ],
        )
        .expect("kong should be handled")
        .expect("kong should succeed");

        assert_eq!(
            room.pending_timeout
                .as_ref()
                .and_then(|timeout| timeout.drawn_tile_id.as_deref()),
            Some("w9#draw")
        );

        let emitted = try_process_due_timeout_in_room_state(&mut room)
            .expect("active turn timeout should resolve after claim kong")
            .expect("timeout should emit discard message");

        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0]["payload"]["event_type"], "tile_discarded");
        assert_eq!(emitted[0]["payload"]["event"]["tile_id"], "w9#draw");
    }

    #[test]
    fn active_turn_timeout_after_pung_claim_ignores_stale_drawn_tile_id() {
        let mut room = claim_window_room_state();
        room.round_state
            .as_mut()
            .and_then(|round| round.players.get_mut(2))
            .expect("seat 2 should exist")
            .concealed_tiles = vec![
            crate::core::tile::Tile::from_value(&suit("w3", "w3#2a"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("w3", "w3#2b"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("t1", "t1#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("t4", "t4#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("t7", "t7#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("b1", "b1#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("b4", "b4#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("b7", "b7#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("w9", "w9#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("t9", "t9#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("b9", "b9#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&wind("south", "south#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("w3", "w3#2extra"), "tile").expect("tile"),
        ];
        let _ = try_handle_player_action_in_room_state(
            &mut room,
            0,
            "discard",
            &[String::from("w3#discard")],
        )
        .expect("discard should be handled")
        .expect("discard should succeed");
        let _ = try_handle_player_action_in_room_state(
            &mut room,
            2,
            "pung",
            &[String::from("w3#2a"), String::from("w3#2b")],
        )
        .expect("pung should be handled")
        .expect("pung should succeed");
        room.pending_timeout
            .as_mut()
            .expect("timeout should exist")
            .drawn_tile_id = Some("w3#discard".to_string());

        let emitted = try_process_due_timeout_in_room_state(&mut room)
            .expect("active turn timeout should recover from stale drawn tile")
            .expect("timeout should still emit discard message");

        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0]["payload"]["event_type"], "tile_discarded");
        assert_ne!(emitted[0]["payload"]["event"]["tile_key"], "w3");
    }

    #[test]
    fn active_turn_timeout_after_claim_kong_ignores_missing_drawn_tile_id() {
        let mut room = claim_window_room_state();
        room.round_state
            .as_mut()
            .and_then(|round| round.players.get_mut(2))
            .expect("seat 2 should exist")
            .concealed_tiles = vec![
            crate::core::tile::Tile::from_value(&suit("w3", "w3#2a"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("w3", "w3#2b"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("w3", "w3#2c"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("t1", "t1#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("t4", "t4#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("t7", "t7#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("b1", "b1#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("b4", "b4#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("b7", "b7#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("w9", "w9#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("t9", "t9#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&suit("b9", "b9#2"), "tile").expect("tile"),
            crate::core::tile::Tile::from_value(&wind("south", "south#2"), "tile").expect("tile"),
        ];
        let _ = try_handle_player_action_in_room_state(
            &mut room,
            0,
            "discard",
            &[String::from("w3#discard")],
        )
        .expect("discard should be handled")
        .expect("discard should succeed");
        let _ = try_handle_player_action_in_room_state(
            &mut room,
            2,
            "kong",
            &[
                String::from("w3#2a"),
                String::from("w3#2b"),
                String::from("w3#2c"),
            ],
        )
        .expect("kong should be handled")
        .expect("kong should succeed");
        room.pending_timeout
            .as_mut()
            .expect("timeout should exist")
            .drawn_tile_id = Some("missing#tile".to_string());

        let emitted = try_process_due_timeout_in_room_state(&mut room)
            .expect("active turn timeout should recover from missing drawn tile")
            .expect("timeout should still emit discard message");

        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0]["payload"]["event_type"], "tile_discarded");
        assert_eq!(emitted[0]["payload"]["event"]["tile_id"], "w9#draw");
    }

    fn ready_hand_auto_room_state(drawn_tile_key: &str, drawn_tile_id: &str) -> RoomState {
        let drawn_tile = if drawn_tile_key.starts_with('f') {
            flower(drawn_tile_key, drawn_tile_id)
        } else {
            suit(drawn_tile_key, drawn_tile_id)
        };
        RoomState::from_room_value(&json!({
            "table_code": "READY1",
            "phase": "playing",
            "mode": "normal",
            "continue_action": null,
            "seats": [
                {"seat_index": 0, "nickname": "P0", "reconnect_token": "t0", "player_session_id": 1, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null},
                {"seat_index": 1, "nickname": "P1", "reconnect_token": "t1", "player_session_id": 2, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null},
                {"seat_index": 2, "nickname": "P2", "reconnect_token": "t2", "player_session_id": 3, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null},
                {"seat_index": 3, "nickname": "P3", "reconnect_token": "t3", "player_session_id": 4, "connected": true, "ready": true, "is_bot": false, "seat_type": "human", "bot_persona": null, "bot_aggression": null, "disconnect_deadline_at": null}
            ],
            "match_state": {
                "prevailing_wind": "east",
                "hand_number": 1,
                "dealer_seat": 0,
                "cumulative_scores": {"0": 0, "1": 0, "2": 0, "3": 0},
                "match_finished": false,
                "last_completed_round_id": null
            },
            "round_state": {
                "round_id": "east-1-dealer-0-ready-auto",
                "dealer_seat": 0,
                "current_actor": 0,
                "wall": {
                    "tiles": [suit("b9", "b9#tail")],
                    "head_index": 0,
                    "tail_index": 0
                },
                "players": [
                    {
                        "seat": 0,
                        "is_ready_hand": true,
                        "concealed_tiles": [
                            suit("w1", "w1#0"),
                            suit("w2", "w2#1"),
                            suit("w3", "w3#2"),
                            suit("w4", "w4#3"),
                            suit("w5", "w5#4"),
                            suit("w6", "w6#5"),
                            suit("w7", "w7#6"),
                            suit("w8", "w8#7"),
                            suit("w9", "w9#8"),
                            suit("t1", "t1#9"),
                            suit("t2", "t2#10"),
                            suit("t3", "t3#11"),
                            suit("t4", "t4#12"),
                            drawn_tile
                        ],
                        "melds": [],
                        "flowers": [],
                        "discards": [suit("b9", "b9#discard")]
                    },
                    {
                        "seat": 1,
                        "concealed_tiles": [suit("b1", "b1#1")],
                        "melds": [],
                        "flowers": [],
                        "discards": []
                    },
                    {
                        "seat": 2,
                        "concealed_tiles": [suit("b2", "b2#2")],
                        "melds": [],
                        "flowers": [],
                        "discards": []
                    },
                    {
                        "seat": 3,
                        "concealed_tiles": [suit("b3", "b3#3")],
                        "melds": [],
                        "flowers": [],
                        "discards": []
                    }
                ],
                "last_discard": null,
                "pending_action": null,
                "phase": "playing",
                "settlement": null,
                "version": 1,
                "score_trackers": {"kong_entries": []},
                "last_action_context": {
                    "kind": "draw",
                    "seat": 0,
                    "tile_id": drawn_tile_id,
                    "from_kong_replacement": false,
                    "was_last_live_tile": false,
                    "was_last_discard": false
                },
                "round_wind": "east",
                "restricted_discard_tile_key": null
            },
            "pending_timeout": {
                "kind": "active_turn",
                "seat_index": 0,
                "deadline_at": "2026-04-21T12:00:30Z",
                "drawn_tile_id": drawn_tile_id
            }
        }))
        .expect("room should parse")
    }

    #[test]
    fn ready_hand_human_discards_drawn_tile_as_next_auto_action() {
        let room = ready_hand_auto_room_state("b9", "b9#draw");

        let action = next_bot_action_in_room_state(&room)
            .expect("ready-hand lookup should succeed")
            .expect("ready-hand player should auto discard");

        assert_eq!(action.seat_index, 0);
        assert_eq!(action.action_type, "discard");
        assert_eq!(action.tile_ids, vec!["b9#draw".to_string()]);
    }

    #[test]
    fn ready_hand_human_exposes_drawn_flower_as_next_auto_action() {
        let room = ready_hand_auto_room_state("f1", "f1#draw");

        let action = next_bot_action_in_room_state(&room)
            .expect("ready-hand lookup should succeed")
            .expect("ready-hand player should auto expose flower");

        assert_eq!(action.seat_index, 0);
        assert_eq!(action.action_type, "flower");
        assert_eq!(action.tile_ids, vec!["f1#draw".to_string()]);
    }

    #[test]
    fn ready_hand_human_waits_for_manual_hu_when_draw_is_winning_tile() {
        let room = ready_hand_auto_room_state("t4", "t4#draw");

        let action =
            next_bot_action_in_room_state(&room).expect("ready-hand lookup should succeed");

        assert!(
            action.is_none(),
            "winning draw should wait for the human player to click hu"
        );
    }

    #[test]
    fn ready_hand_bot_discards_low_fan_self_draw() {
        let mut room = ready_hand_auto_room_state("w3", "w3#draw");
        room.seats.get_mut(0).expect("seat should exist").is_bot = true;
        room.round_state
            .as_mut()
            .and_then(|round| round.players.get_mut(0))
            .expect("player should exist")
            .concealed_tiles = serde_json::from_value(json!([
            suit("w1", "w1#0"),
            suit("w2", "w2#1"),
            suit("w3", "w3#draw"),
            suit("t4", "t4#3"),
            suit("t5", "t5#4"),
            suit("t6", "t6#5"),
            suit("b3", "b3#6"),
            suit("b4", "b4#7"),
            suit("b5", "b5#8"),
            suit("w6", "w6#9"),
            suit("w7", "w7#10"),
            suit("w8", "w8#11"),
            wind("red", "red#12"),
            wind("red", "red#13")
        ]))
        .expect("tiles should parse");

        let action =
            next_bot_action_in_room_state(&room).expect("ready-hand lookup should succeed");

        assert!(
            action.is_some(),
            "bot should keep acting after refusing low fan hu"
        );
        assert_ne!(
            action.as_ref().map(|action| action.action_type.as_str()),
            Some("hu")
        );
    }

    #[test]
    fn ready_hand_human_waits_for_manual_add_kong_before_auto_discard() {
        let mut room = ready_hand_auto_room_state("w3", "w3#add");
        let round = room.round_state.as_mut().expect("round should exist");
        let player = round.players.get_mut(0).expect("player should exist");
        player.concealed_tiles = serde_json::from_value(json!([
            suit("w3", "w3#add"),
            suit("w2", "w2#a"),
            suit("w3", "w3#a"),
            suit("w4", "w4#a"),
            suit("t2", "t2#a"),
            suit("t3", "t3#a"),
            suit("t4", "t4#a"),
            suit("b2", "b2#a"),
            suit("b3", "b3#a"),
            suit("b4", "b4#a"),
            suit("w6", "w6#a"),
            suit("w7", "w7#a"),
            suit("w8", "w8#a"),
            suit("b7", "b7#a")
        ]))
        .expect("tiles should parse");
        player.melds = vec![vec!["w3".to_string(), "w3".to_string(), "w3".to_string()]];

        let action =
            next_bot_action_in_room_state(&room).expect("ready-hand lookup should succeed");

        assert!(
            action.is_none(),
            "ready-hand human should wait for manual add kong instead of auto discarding"
        );
    }

    #[test]
    fn ready_hand_human_timeout_auto_hu_when_draw_is_winning_tile() {
        let mut room = ready_hand_auto_room_state("t4", "t4#draw");

        let result = try_process_due_timeout_in_room_state(&mut room)
            .expect("ready-hand timeout should not fail");

        assert!(
            result.is_some(),
            "timeout should auto hu for a human ready-hand self draw"
        );
        let emitted = result.expect("timeout should emit hu message");
        assert_eq!(emitted[0]["payload"]["event_type"], "self_hu_declared");
        assert_eq!(
            room.pending_timeout
                .as_ref()
                .map(|timeout| timeout.kind.as_str()),
            None
        );
    }

    #[test]
    fn ready_hand_human_timeout_auto_exposes_drawn_flower() {
        let mut room = ready_hand_auto_room_state("f1", "f1#draw");

        let emitted = try_process_due_timeout_in_room_state(&mut room)
            .expect("ready-hand timeout should not fail")
            .expect("timeout should auto expose flower");

        assert_eq!(emitted[0]["payload"]["event_type"], "flower_exposed");
        assert_eq!(emitted[0]["payload"]["event"]["tile_id"], "f1#draw");
        assert_eq!(emitted[1]["payload"]["event_type"], "replacement_draw");
    }
}
