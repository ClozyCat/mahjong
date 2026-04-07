use serde_json::{Value, json};
use std::collections::HashSet;

use crate::core::engine::planner::{
    plan_claim_window_continuation_without_winner, plan_claim_window_response, plan_discard_action,
    resolve_claims,
};
use crate::core::engine::reducer::{LegacyRoomMutation, apply_legacy_room_mutations};

use super::meld::{
    SelfKongCandidate, SelfKongKind, available_self_kongs, claim_window_options_after_discard,
    is_valid_chow_sequence_by_keys, resolve_self_kong_selection, seats_with_hu_candidate_for_tile,
};
use super::runtime::{
    current_actor, is_last_live_tile_point, pending_timeout_kind, player_concealed_tile,
    player_concealed_tiles_slice, project_room_state, replacement_tile_from_tail,
    round_event_message, sync_pending_timeout,
};
use super::settlement::settle_exhaustive_draw;
use super::win::{apply_hu_settlement, compute_hu_settlement};

const MAX_SEATS: usize = 4;

pub fn try_handle_self_kong_action(
    room: &mut Value,
    seat_index: usize,
    tile_ids: &[String],
) -> Option<Result<Vec<Value>, String>> {
    let candidates = available_self_kongs(room, seat_index);
    if candidates.is_empty() {
        return Some(Err("invalid_action".to_string()));
    }
    let selection = resolve_self_kong_selection(&candidates, tile_ids);
    let Some(selection) = selection else {
        return Some(Err("invalid_action".to_string()));
    };
    replacement_tile_from_tail(room)?;
    if selection.kind == SelfKongKind::Add {
        let offered_hu_seats =
            seats_with_hu_candidate_for_tile(room, seat_index, &selection.tile_key);
        if !offered_hu_seats.is_empty() {
            return Some(start_rob_kong_window(
                room,
                seat_index,
                &selection,
                offered_hu_seats,
            ));
        }
    }
    Some(apply_self_kong_action(room, seat_index, &selection))
}

pub fn apply_claim_window_action(
    room: &mut Value,
    seat_index: usize,
    action_type: &str,
    tile_ids: &[String],
) -> Result<Vec<Value>, String> {
    if matches!(action_type, "chow" | "pung" | "kong") {
        validate_claim_selection(room, seat_index, action_type, tile_ids)?;
    }
    let state = project_room_state(room)?;
    let plan = plan_claim_window_response(&state, seat_index, action_type, tile_ids)?;
    apply_legacy_room_mutations(room, &plan.mutations)?;
    if !plan.unresolved_seats.is_empty() {
        sync_pending_timeout(room);
        return Ok(vec![]);
    }
    resolve_recorded_claims_local(room)
}

pub fn apply_discard_action(
    room: &mut Value,
    seat_index: usize,
    tile_id: &str,
) -> Result<Vec<Value>, String> {
    if room.get("phase").and_then(Value::as_str) != Some("playing") {
        return Err("round_not_ready".to_string());
    }
    if current_actor(room) != Some(seat_index) {
        return Err("not_your_turn".to_string());
    }
    if room
        .get("round_state")
        .and_then(|round| round.get("pending_action"))
        .filter(|value| !value.is_null())
        .is_some()
    {
        return Err("invalid_action".to_string());
    }

    let restricted_discard_tile_key = room
        .get("round_state")
        .and_then(|round| round.get("restricted_discard_tile_key"))
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let discarded_tile = player_concealed_tile(room, seat_index, tile_id)
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;
    if let Some(restricted) = restricted_discard_tile_key.as_deref() {
        if discarded_tile.get("tile_key").and_then(Value::as_str) == Some(restricted) {
            return Err("invalid_action".to_string());
        }
    }
    let state = project_room_state(room)?;
    let discard_mutations = vec![
        LegacyRoomMutation::RemovePlayerConcealedTileById {
            seat_index,
            tile_id: tile_id.to_string(),
        },
        LegacyRoomMutation::PushPlayerDiscard {
            seat_index,
            tile: discarded_tile.clone(),
        },
        LegacyRoomMutation::SetRoundLastDiscard {
            tile: discarded_tile.clone(),
        },
    ];
    let mut simulated = room.clone();
    apply_legacy_room_mutations(&mut simulated, &discard_mutations)?;

    let previous_was_last_live_tile = room
        .get("round_state")
        .and_then(|round| round.get("last_action_context"))
        .and_then(|context| context.get("was_last_live_tile"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let claim_window = claim_window_options_after_discard(
        &simulated,
        seat_index,
        discarded_tile
            .get("tile_key")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    let plan = plan_discard_action(
        &state,
        seat_index,
        tile_id,
        claim_window,
        previous_was_last_live_tile,
    )?;
    apply_legacy_room_mutations(room, &plan.discard_mutations)?;
    if plan.needs_exhaustive_draw {
        let mut messages = vec![round_event_message(
            "tile_discarded",
            json!({
                "type": "tile_discarded",
                "seat": seat_index,
                "tile_id": plan.discarded_tile.tile_id,
            }),
        )];
        messages.extend(settle_exhaustive_draw(room));
        return Ok(messages);
    }
    apply_legacy_room_mutations(room, &plan.followup_mutations)?;
    sync_pending_timeout(room);
    Ok(vec![round_event_message(
        "tile_discarded",
        json!({
            "type": "tile_discarded",
            "seat": seat_index,
            "tile_id": plan.discarded_tile.tile_id,
        }),
    )])
}

pub fn can_resolve_discard_locally(room: &Value, seat_index: usize, tile_id: &str) -> bool {
    if room.get("phase").and_then(Value::as_str) != Some("playing") {
        return false;
    }
    if current_actor(room) != Some(seat_index) {
        return false;
    }
    if room
        .get("round_state")
        .and_then(|round| round.get("pending_action"))
        .filter(|value| !value.is_null())
        .is_some()
    {
        return false;
    }
    if wall_live_tiles_remaining(room) <= 0 {
        return false;
    }

    let Some(discarded_tile) = player_concealed_tile(room, seat_index, tile_id) else {
        return false;
    };
    if let Some(restricted) = room
        .get("round_state")
        .and_then(|round| round.get("restricted_discard_tile_key"))
        .and_then(Value::as_str)
    {
        if discarded_tile.get("tile_key").and_then(Value::as_str) == Some(restricted) {
            return false;
        }
    }

    discarded_tile.get("tile_id").is_some()
}

pub fn claim_window_supported_locally(room: &Value) -> bool {
    room.get("round_state")
        .and_then(|round| round.get("pending_action"))
        .and_then(|pending| pending.get("type"))
        .and_then(Value::as_str)
        == Some("claim_window")
}

pub fn rob_kong_window_supported_locally(room: &Value) -> bool {
    room.get("round_state")
        .and_then(|round| round.get("pending_action"))
        .and_then(|pending| pending.get("type"))
        .and_then(Value::as_str)
        == Some("rob_kong_window")
}

pub fn can_resolve_claim_window_timeout_locally(room: &Value) -> bool {
    pending_timeout_kind(room) == Some("claim_window") && claim_window_supported_locally(room)
}

pub fn resolve_claim_window_timeout(room: &mut Value) -> Result<Vec<Value>, String> {
    let pending_action = room
        .get("round_state")
        .and_then(|round| round.get("pending_action"))
        .cloned()
        .unwrap_or(Value::Null);
    let discarder_seat = pending_action
        .get("discarder_seat")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .ok_or_else(|| "invalid_action".to_string())?;
    let claim_window = pending_action
        .get("claim_window")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut responded = pending_action
        .get("responded_seats")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let unresolved: Vec<Value> = claim_window
        .iter()
        .enumerate()
        .filter(|(_, claims)| {
            claims
                .as_array()
                .map(|items| !items.is_empty())
                .unwrap_or(false)
        })
        .filter(|(seat_index, _)| {
            !responded.iter().any(|value| {
                value
                    .as_u64()
                    .map(|seat| seat as usize == *seat_index)
                    .unwrap_or(false)
            })
        })
        .map(|(seat_index, _)| Value::Number((seat_index as u64).into()))
        .collect();
    responded.extend(unresolved.iter().cloned());
    apply_legacy_room_mutations(
        room,
        &[
            LegacyRoomMutation::SetRoundPendingAction {
                pending_action: json!({
                    "type": "claim_window",
                    "discarder_seat": discarder_seat,
                    "claim_window": claim_window,
                    "responded_seats": responded,
                    "claim_responses": pending_action
                        .get("claim_responses")
                        .cloned()
                        .unwrap_or_else(|| Value::Array(vec![])),
                }),
            },
            LegacyRoomMutation::IncrementRoundVersion,
        ],
    )?;

    let mut messages = vec![round_event_message(
        "claim_auto_passed",
        json!({
            "type": "claim_auto_passed",
            "discarder_seat": discarder_seat,
            "seats": unresolved,
        }),
    )];
    messages.extend(resolve_recorded_claims_local(room)?);
    Ok(messages)
}

pub fn can_resolve_rob_kong_timeout_locally(room: &Value) -> bool {
    pending_timeout_kind(room) == Some("claim_window") && rob_kong_window_supported_locally(room)
}

pub fn apply_rob_kong_pass(room: &mut Value, seat_index: usize) -> Result<Vec<Value>, String> {
    let pending = room
        .get("round_state")
        .and_then(|round| round.get("pending_action"))
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;
    if pending.get("type").and_then(Value::as_str) != Some("rob_kong_window") {
        return Err("invalid_action".to_string());
    }
    let offered = pending
        .get("offered_hu_seats")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !offered.iter().any(|value| {
        value
            .as_u64()
            .map(|seat| seat as usize == seat_index)
            .unwrap_or(false)
    }) {
        return Err("invalid_action".to_string());
    }
    let responded = pending
        .get("responded_seats")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if responded.iter().any(|value| {
        value
            .as_u64()
            .map(|seat| seat as usize == seat_index)
            .unwrap_or(false)
    }) {
        return Err("invalid_action".to_string());
    }

    let mut next_responded = responded;
    next_responded.push(Value::Number((seat_index as u64).into()));
    apply_legacy_room_mutations(
        room,
        &[
            LegacyRoomMutation::SetRoundPendingAction {
                pending_action: json!({
                    "type": "rob_kong_window",
                    "actor_seat": pending.get("actor_seat").cloned().unwrap_or(Value::Null),
                    "tile_id": pending.get("tile_id").cloned().unwrap_or(Value::Null),
                    "tile_key": pending.get("tile_key").cloned().unwrap_or(Value::Null),
                    "meld_index": pending.get("meld_index").cloned().unwrap_or(Value::Null),
                    "offered_hu_seats": offered,
                    "responded_seats": next_responded.clone(),
                }),
            },
            LegacyRoomMutation::IncrementRoundVersion,
        ],
    )?;

    let unresolved = offered
        .iter()
        .filter_map(|value| value.as_u64().map(|seat| seat as usize))
        .filter(|offered_seat| {
            !next_responded.iter().any(|value| {
                value
                    .as_u64()
                    .map(|seat| seat as usize == *offered_seat)
                    .unwrap_or(false)
            })
        })
        .collect::<Vec<_>>();
    if !unresolved.is_empty() {
        sync_pending_timeout(room);
        return Ok(vec![]);
    }
    complete_add_kong_after_passes(room)
}

pub fn resolve_rob_kong_timeout(room: &mut Value) -> Result<Vec<Value>, String> {
    let pending = room
        .get("round_state")
        .and_then(|round| round.get("pending_action"))
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;
    if pending.get("type").and_then(Value::as_str) != Some("rob_kong_window") {
        return Err("invalid_action".to_string());
    }
    let actor_seat = pending
        .get("actor_seat")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .ok_or_else(|| "invalid_action".to_string())?;
    let offered = pending
        .get("offered_hu_seats")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let responded = pending
        .get("responded_seats")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let unresolved: Vec<Value> = offered
        .iter()
        .filter(|seat| !responded.iter().any(|value| value == *seat))
        .cloned()
        .collect();
    let mut next_responded = responded;
    next_responded.extend(unresolved.iter().cloned());
    apply_legacy_room_mutations(
        room,
        &[
            LegacyRoomMutation::SetRoundPendingAction {
                pending_action: json!({
                    "type": "rob_kong_window",
                    "actor_seat": actor_seat,
                    "tile_id": pending.get("tile_id").cloned().unwrap_or(Value::Null),
                    "tile_key": pending.get("tile_key").cloned().unwrap_or(Value::Null),
                    "meld_index": pending.get("meld_index").cloned().unwrap_or(Value::Null),
                    "offered_hu_seats": offered,
                    "responded_seats": next_responded,
                }),
            },
            LegacyRoomMutation::IncrementRoundVersion,
        ],
    )?;
    let mut messages = vec![round_event_message(
        "rob_kong_auto_passed",
        json!({
            "type": "rob_kong_auto_passed",
            "actor_seat": actor_seat,
            "seats": unresolved,
        }),
    )];
    messages.extend(complete_add_kong_after_passes(room)?);
    Ok(messages)
}

fn apply_self_kong_action(
    room: &mut Value,
    seat_index: usize,
    selection: &SelfKongCandidate,
) -> Result<Vec<Value>, String> {
    if room.get("phase").and_then(Value::as_str) != Some("playing") {
        return Err("round_not_ready".to_string());
    }
    if current_actor(room) != Some(seat_index) {
        return Err("not_your_turn".to_string());
    }
    if room
        .get("round_state")
        .and_then(|round| round.get("pending_action"))
        .filter(|value| !value.is_null())
        .is_some()
    {
        return Err("invalid_action".to_string());
    }
    if is_last_live_tile_point(room) {
        return Err("invalid_action".to_string());
    }

    let replacement_tile =
        replacement_tile_from_tail(room).ok_or_else(|| "invalid_action".to_string())?;
    complete_self_kong(room, seat_index, selection, replacement_tile)
}

fn complete_self_kong(
    room: &mut Value,
    seat_index: usize,
    selection: &SelfKongCandidate,
    replacement_tile: Value,
) -> Result<Vec<Value>, String> {
    let plan =
        plan_self_kong_completion(room, seat_index, selection, replacement_tile.clone(), false)?;
    apply_legacy_room_mutations(room, &plan.mutations)?;
    sync_pending_timeout(room);
    Ok(plan.events)
}

fn start_rob_kong_window(
    room: &mut Value,
    seat_index: usize,
    selection: &SelfKongCandidate,
    offered_hu_seats: Vec<usize>,
) -> Result<Vec<Value>, String> {
    let selected_tile = player_concealed_tile(
        room,
        seat_index,
        selection
            .tile_ids
            .first()
            .map(String::as_str)
            .unwrap_or_default(),
    )
    .cloned()
    .ok_or_else(|| "invalid_action".to_string())?;
    apply_legacy_room_mutations(
        room,
        &[
            LegacyRoomMutation::SetRoundLastDiscard {
                tile: selected_tile.clone(),
            },
            LegacyRoomMutation::SetRoundPendingAction {
                pending_action: json!({
                    "type": "rob_kong_window",
                    "actor_seat": seat_index,
                    "tile_id": selected_tile.get("tile_id").cloned().unwrap_or(Value::Null),
                    "tile_key": selected_tile.get("tile_key").cloned().unwrap_or(Value::Null),
                    "meld_index": selection.meld_index,
                    "offered_hu_seats": offered_hu_seats,
                    "responded_seats": [],
                }),
            },
            LegacyRoomMutation::IncrementRoundVersion,
        ],
    )?;
    sync_pending_timeout(room);
    Ok(vec![round_event_message(
        "self_kong_declared",
        json!({
            "type": "self_kong_declared",
            "seat": seat_index,
            "kong_type": "add_kong",
            "tile_key": selection.tile_key,
            "tile_ids": selection.tile_ids,
        }),
    )])
}

struct SelfKongPlan {
    mutations: Vec<LegacyRoomMutation>,
    events: Vec<Value>,
}

fn plan_self_kong_completion(
    room: &Value,
    seat_index: usize,
    selection: &SelfKongCandidate,
    replacement_tile: Value,
    clear_pending_action: bool,
) -> Result<SelfKongPlan, String> {
    for tile_id in &selection.tile_ids {
        player_concealed_tile(room, seat_index, tile_id)
            .ok_or_else(|| "invalid_action".to_string())?;
    }

    let mut mutations = selection
        .tile_ids
        .iter()
        .cloned()
        .map(
            |tile_id| LegacyRoomMutation::RemovePlayerConcealedTileById {
                seat_index,
                tile_id,
            },
        )
        .collect::<Vec<_>>();

    match selection.kind {
        SelfKongKind::Concealed => {
            mutations.push(LegacyRoomMutation::PushPlayerMeld {
                seat_index,
                meld: Value::Array(vec![
                    Value::String(selection.tile_key.clone()),
                    Value::String(selection.tile_key.clone()),
                    Value::String(selection.tile_key.clone()),
                    Value::String(selection.tile_key.clone()),
                ]),
            });
        }
        SelfKongKind::Add => {
            let meld_index = selection
                .meld_index
                .ok_or_else(|| "invalid_action".to_string())?;
            mutations.push(LegacyRoomMutation::AppendTileToPlayerMeld {
                seat_index,
                meld_index,
                tile: Value::String(selection.tile_key.clone()),
            });
        }
    }

    mutations.push(LegacyRoomMutation::PushPlayerConcealedTile {
        seat_index,
        tile: replacement_tile.clone(),
    });
    mutations.push(LegacyRoomMutation::RetreatWallTail);
    mutations.push(LegacyRoomMutation::AppendRoundKongEntry {
        kong_type: match selection.kind {
            SelfKongKind::Concealed => "concealed_kong".to_string(),
            SelfKongKind::Add => "add_kong".to_string(),
        },
        actor_seat: seat_index,
        payer_seats: (0..MAX_SEATS)
            .filter(|other| *other != seat_index)
            .collect(),
        tile_key: Value::String(selection.tile_key.clone()),
    });
    mutations.push(LegacyRoomMutation::SetRoundLastActionContext {
        context: json!({
            "kind": "replacement_draw",
            "seat": seat_index,
            "tile_id": replacement_tile.get("tile_id").cloned().unwrap_or(Value::Null),
            "from_kong_replacement": true,
            "was_last_live_tile": false,
            "was_last_discard": false,
        }),
    });
    if clear_pending_action {
        mutations.push(LegacyRoomMutation::SetRoundPendingAction {
            pending_action: Value::Null,
        });
    }
    mutations.push(LegacyRoomMutation::IncrementRoundVersion);

    Ok(SelfKongPlan {
        mutations,
        events: vec![
            round_event_message(
                "self_kong_declared",
                json!({
                    "type": "self_kong_declared",
                    "seat": seat_index,
                    "kong_type": match selection.kind {
                        SelfKongKind::Concealed => "concealed_kong",
                        SelfKongKind::Add => "add_kong",
                    },
                    "tile_key": selection.tile_key,
                    "tile_ids": selection.tile_ids,
                }),
            ),
            round_event_message(
                "replacement_draw",
                json!({
                    "type": "replacement_draw",
                    "seat": seat_index,
                    "tile_id": replacement_tile.get("tile_id").cloned().unwrap_or(Value::Null),
                }),
            ),
        ],
    })
}

fn resolve_recorded_claims_local(room: &mut Value) -> Result<Vec<Value>, String> {
    let pending_action = room
        .get("round_state")
        .and_then(|round| round.get("pending_action"))
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;
    if pending_action.get("type").and_then(Value::as_str) != Some("claim_window") {
        return Err("invalid_action".to_string());
    }
    let discarder_seat = pending_action
        .get("discarder_seat")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .ok_or_else(|| "invalid_action".to_string())?;
    let claim_responses = pending_action
        .get("claim_responses")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    if let Some(winner) = resolve_claims(&claim_responses, discarder_seat) {
        let winner_seat = winner
            .get("seat")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .ok_or_else(|| "invalid_action".to_string())?;
        let claim_type = winner
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| "invalid_action".to_string())?;
        let tiles = winner
            .get("tiles")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|value| value.as_str().map(ToString::to_string))
            .collect::<Vec<_>>();
        return apply_selected_claim(room, winner_seat, claim_type, &tiles);
    }

    let state = project_room_state(room)?;
    let plan = plan_claim_window_continuation_without_winner(&state, discarder_seat)?;
    if plan.needs_exhaustive_draw {
        return Ok(settle_exhaustive_draw(room));
    }
    apply_legacy_room_mutations(room, &plan.mutations)?;
    sync_pending_timeout(room);
    Ok(vec![])
}

fn validate_claim_selection(
    room: &Value,
    seat_index: usize,
    action_type: &str,
    tile_ids: &[String],
) -> Result<(), String> {
    let last_discard = room
        .get("round_state")
        .and_then(|round| round.get("last_discard"))
        .ok_or_else(|| "invalid_action".to_string())?;
    let last_discard_tile_key = last_discard
        .get("tile_key")
        .and_then(Value::as_str)
        .ok_or_else(|| "invalid_action".to_string())?;
    let expected = match action_type {
        "chow" | "pung" => 2,
        "kong" => 3,
        _ => return Err("invalid_action".to_string()),
    };
    if tile_ids.len() != expected {
        return Err("invalid_action".to_string());
    }
    let player_tiles = player_concealed_tiles_slice(room, seat_index)
        .ok_or_else(|| "invalid_action".to_string())?;
    let mut claimed_tile_keys = Vec::with_capacity(tile_ids.len());
    let mut used_indices = HashSet::with_capacity(tile_ids.len());
    for tile_id in tile_ids {
        let Some((index, tile)) = player_tiles.iter().enumerate().find(|(index, tile)| {
            !used_indices.contains(index)
                && tile.get("tile_id").and_then(Value::as_str) == Some(tile_id.as_str())
        }) else {
            return Err("invalid_action".to_string());
        };
        used_indices.insert(index);
        claimed_tile_keys.push(
            tile.get("tile_key")
                .and_then(Value::as_str)
                .ok_or_else(|| "invalid_action".to_string())?,
        );
    }

    if (action_type == "pung" || action_type == "kong")
        && claimed_tile_keys
            .iter()
            .any(|tile_key| *tile_key != last_discard_tile_key)
    {
        return Err("invalid_action".to_string());
    }
    if action_type == "chow"
        && !is_valid_chow_sequence_by_keys(last_discard_tile_key, &claimed_tile_keys)
    {
        return Err("invalid_action".to_string());
    }
    Ok(())
}

fn apply_selected_claim(
    room: &mut Value,
    seat_index: usize,
    action_type: &str,
    tile_ids: &[String],
) -> Result<Vec<Value>, String> {
    if action_type == "hu" {
        let settlement = compute_hu_settlement(room, seat_index, "discard")?;
        return apply_hu_settlement(room, seat_index, "discard", settlement);
    }
    let plan = plan_selected_claim(room, seat_index, action_type, tile_ids)?;
    apply_legacy_room_mutations(room, &plan.mutations)?;
    sync_pending_timeout(room);
    Ok(plan.events)
}

struct SelectedClaimPlan {
    mutations: Vec<LegacyRoomMutation>,
    events: Vec<Value>,
}

fn plan_selected_claim(
    room: &Value,
    seat_index: usize,
    action_type: &str,
    tile_ids: &[String],
) -> Result<SelectedClaimPlan, String> {
    if action_type != "chow" && action_type != "pung" && action_type != "kong" {
        return Err("invalid_action".to_string());
    }
    validate_claim_selection(room, seat_index, action_type, tile_ids)?;
    let discarder_seat = room
        .get("round_state")
        .and_then(|round| round.get("pending_action"))
        .and_then(|pending| pending.get("discarder_seat"))
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .ok_or_else(|| "invalid_action".to_string())?;
    let last_discard = room
        .get("round_state")
        .and_then(|round| round.get("last_discard"))
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;
    let restricted_tile_key = last_discard.get("tile_key").cloned().unwrap_or(Value::Null);

    if !discarder_latest_discard_matches(room, discarder_seat, &last_discard) {
        return Err("invalid_action".to_string());
    }
    let claimed_tiles = selected_player_tiles(room, seat_index, tile_ids)?;
    let meld = claim_meld_value(action_type, &last_discard, &claimed_tiles);

    let mut mutations = tile_ids
        .iter()
        .cloned()
        .map(
            |tile_id| LegacyRoomMutation::RemovePlayerConcealedTileById {
                seat_index,
                tile_id,
            },
        )
        .collect::<Vec<_>>();
    mutations.push(LegacyRoomMutation::PopPlayerDiscardLast {
        seat_index: discarder_seat,
    });
    mutations.push(LegacyRoomMutation::PushPlayerMeld { seat_index, meld });

    let mut events = vec![round_event_message(
        "claim_made",
        json!({
            "type": "claim_made",
            "seat": seat_index,
            "claim_type": action_type,
            "tile_id": last_discard.get("tile_id").cloned().unwrap_or(Value::Null),
        }),
    )];

    if action_type == "kong" {
        let replacement_tile =
            replacement_tile_from_tail(room).ok_or_else(|| "invalid_action".to_string())?;
        mutations.push(LegacyRoomMutation::RetreatWallTail);
        mutations.push(LegacyRoomMutation::PushPlayerConcealedTile {
            seat_index,
            tile: replacement_tile.clone(),
        });
        mutations.push(LegacyRoomMutation::AppendRoundKongEntry {
            kong_type: "exposed_kong".to_string(),
            actor_seat: seat_index,
            payer_seats: vec![discarder_seat],
            tile_key: last_discard.get("tile_key").cloned().unwrap_or(Value::Null),
        });
        mutations.push(LegacyRoomMutation::SetRoundLastActionContext {
            context: json!({
                "kind": "replacement_draw",
                "seat": seat_index,
                "tile_id": replacement_tile.get("tile_id").cloned().unwrap_or(Value::Null),
                "from_kong_replacement": true,
                "was_last_live_tile": false,
                "was_last_discard": false,
            }),
        });
        events.push(round_event_message(
            "replacement_draw",
            json!({
                "type": "replacement_draw",
                "seat": seat_index,
                "tile_id": replacement_tile.get("tile_id").cloned().unwrap_or(Value::Null),
            }),
        ));
    }

    mutations.push(LegacyRoomMutation::SetRoundCurrentActor { seat_index });
    mutations.push(LegacyRoomMutation::SetRoundLastDiscard { tile: Value::Null });
    mutations.push(LegacyRoomMutation::SetRoundPendingAction {
        pending_action: Value::Null,
    });
    mutations.push(LegacyRoomMutation::SetRoundRestrictedDiscardTileKey {
        tile_key: restricted_tile_key,
    });
    mutations.push(LegacyRoomMutation::IncrementRoundVersion);

    Ok(SelectedClaimPlan { mutations, events })
}

fn claim_meld_value(action_type: &str, last_discard: &Value, claimed_tiles: &[Value]) -> Value {
    if action_type == "chow" {
        let mut tiles = claimed_tiles
            .iter()
            .map(|tile| tile.get("tile_key").cloned().unwrap_or(Value::Null))
            .collect::<Vec<_>>();
        tiles.push(last_discard.get("tile_key").cloned().unwrap_or(Value::Null));
        tiles.sort_by(|left, right| {
            left.as_str()
                .unwrap_or("")
                .cmp(right.as_str().unwrap_or(""))
        });
        return Value::Array(tiles);
    }
    let mut tiles = vec![last_discard.get("tile_key").cloned().unwrap_or(Value::Null)];
    tiles.extend(
        claimed_tiles
            .iter()
            .map(|tile| tile.get("tile_key").cloned().unwrap_or(Value::Null)),
    );
    Value::Array(tiles)
}

fn selected_player_tiles(
    room: &Value,
    seat_index: usize,
    tile_ids: &[String],
) -> Result<Vec<Value>, String> {
    let player_tiles = player_concealed_tiles_slice(room, seat_index)
        .ok_or_else(|| "invalid_action".to_string())?;
    let mut selected = Vec::with_capacity(tile_ids.len());
    let mut used_indices = HashSet::with_capacity(tile_ids.len());
    for tile_id in tile_ids {
        let Some((index, tile)) = player_tiles.iter().enumerate().find(|(index, tile)| {
            !used_indices.contains(index)
                && tile.get("tile_id").and_then(Value::as_str) == Some(tile_id.as_str())
        }) else {
            return Err("invalid_action".to_string());
        };
        used_indices.insert(index);
        selected.push(tile.clone());
    }
    Ok(selected)
}

fn discarder_latest_discard_matches(
    room: &Value,
    discarder_seat: usize,
    last_discard: &Value,
) -> bool {
    room.get("round_state")
        .and_then(|round| round.get("players"))
        .and_then(Value::as_array)
        .and_then(|players| players.get(discarder_seat))
        .and_then(|player| player.get("discards"))
        .and_then(Value::as_array)
        .and_then(|discards| discards.last())
        .map(|tile| tile.get("tile_id") == last_discard.get("tile_id"))
        .unwrap_or(false)
}

fn complete_add_kong_after_passes(room: &mut Value) -> Result<Vec<Value>, String> {
    let pending = room
        .get("round_state")
        .and_then(|round| round.get("pending_action"))
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;
    let actor_seat = pending
        .get("actor_seat")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .ok_or_else(|| "invalid_action".to_string())?;
    let tile_id = pending
        .get("tile_id")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| "invalid_action".to_string())?;
    let tile_key = pending
        .get("tile_key")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| "invalid_action".to_string())?;
    let meld_index = pending
        .get("meld_index")
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    let replacement_tile =
        replacement_tile_from_tail(room).ok_or_else(|| "invalid_action".to_string())?;
    let selection = SelfKongCandidate {
        kind: SelfKongKind::Add,
        tile_ids: vec![tile_id],
        tile_key,
        meld_index,
    };
    let plan = plan_self_kong_completion(room, actor_seat, &selection, replacement_tile, true)?;
    apply_legacy_room_mutations(room, &plan.mutations)?;
    sync_pending_timeout(room);
    Ok(plan.events)
}

fn wall_live_tiles_remaining(room: &Value) -> i64 {
    room.get("round_state")
        .and_then(|round| round.get("wall"))
        .and_then(|wall| {
            let head = wall.get("head_index")?.as_i64()?;
            let tail = wall.get("tail_index")?.as_i64()?;
            Some((tail - head + 1).max(0))
        })
        .unwrap_or(0)
}
