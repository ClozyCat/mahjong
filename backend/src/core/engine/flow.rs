use crate::core::action::{GameCommand, PlayerAction};
use crate::core::state::RoomState;
use crate::rules::standard::{
    actions::{
        apply_claim_window_action_in_room_state, apply_discard_action_output_in_room_state,
        apply_player_multiplier_selection_in_room_state,
        apply_ready_hand_action_output_in_room_state, apply_rob_kong_hu_in_room_state,
        apply_rob_kong_pass_in_room_state, try_handle_self_kong_action_output_in_room_state,
    },
    flow::apply_flower_action_output_in_room_state,
    win::apply_hu_action_output_in_room_state,
};

use super::{EngineContext, EngineOutput, LocalPlayerActionKind, classify_local_player_action};

pub fn try_handle_command_in_room_state(
    room: &mut RoomState,
    command: GameCommand,
) -> Result<Option<Result<EngineOutput, String>>, String> {
    let context = EngineContext::from_room_state(room.clone());
    match command {
        GameCommand::PlayerAction { actor, action } => Ok(try_handle_player_action_command(
            room, &context, actor, action,
        )),
        _ => Ok(None),
    }
}

pub fn try_handle_player_action_in_room_state(
    room: &mut RoomState,
    seat_index: usize,
    action_type: &str,
    tile_ids: &[String],
) -> Result<Option<Result<EngineOutput, String>>, String> {
    let Some(command) = super::parse_player_command(seat_index, action_type, tile_ids) else {
        return Ok(None);
    };
    match command {
        Ok(command) => try_handle_command_in_room_state(room, command),
        Err(reason) => Ok(Some(Err(reason))),
    }
}

fn try_handle_player_action_command(
    room: &mut RoomState,
    context: &EngineContext,
    seat_index: usize,
    action: PlayerAction,
) -> Option<Result<EngineOutput, String>> {
    let action_kind = classify_local_player_action(context, seat_index, &action)?;
    match (action_kind, action) {
        (LocalPlayerActionKind::Hu, PlayerAction::Hu) => {
            let pending_action = context
                .room
                .round_state
                .as_ref()
                .and_then(|round| round.pending_action.as_ref());
            match pending_action {
                Some(crate::core::state::PendingAction::ClaimWindow(_)) => Some(
                    apply_claim_window_action_in_room_state(room, seat_index, "hu", &[]),
                ),
                Some(crate::core::state::PendingAction::RobKongWindow(_)) => {
                    Some(apply_rob_kong_hu_in_room_state(room, seat_index))
                }
                _ => Some(apply_hu_action_output_in_room_state(room, seat_index)),
            }
        }
        (LocalPlayerActionKind::Flower, PlayerAction::Flower { tile_ids }) => Some(
            apply_flower_action_output_in_room_state(room, seat_index, &tile_ids),
        ),
        (LocalPlayerActionKind::Discard, PlayerAction::Discard { tile_id }) => Some(
            apply_discard_action_output_in_room_state(room, seat_index, &tile_id),
        ),
        (LocalPlayerActionKind::ReadyHand, PlayerAction::ReadyHand { tile_id }) => Some(
            apply_ready_hand_action_output_in_room_state(room, seat_index, &tile_id),
        ),
        (LocalPlayerActionKind::ClaimWindow, PlayerAction::Kong { tile_ids }) => Some(
            apply_claim_window_action_in_room_state(room, seat_index, "kong", &tile_ids),
        ),
        (LocalPlayerActionKind::SelfKong, PlayerAction::Kong { tile_ids }) => {
            try_handle_self_kong_action_output_in_room_state(room, seat_index, &tile_ids)
                .ok()
                .flatten()
        }
        (LocalPlayerActionKind::ActiveTurnPass, PlayerAction::Pass) => {
            let tile_id = active_turn_pass_tile_id(context, seat_index)
                .ok_or_else(|| "invalid_action".to_string());
            Some(tile_id.and_then(|tile_id| {
                match active_turn_tile_kind(context, seat_index, &tile_id).as_deref() {
                    Some("flower") => {
                        apply_flower_action_output_in_room_state(room, seat_index, &[tile_id])
                    }
                    _ => apply_discard_action_output_in_room_state(room, seat_index, &tile_id),
                }
            }))
        }
        (LocalPlayerActionKind::ClaimWindow, PlayerAction::Pass) => Some(
            apply_claim_window_action_in_room_state(room, seat_index, "pass", &[]),
        ),
        (LocalPlayerActionKind::RobKongPass, PlayerAction::Pass) => {
            Some(apply_rob_kong_pass_in_room_state(room, seat_index))
        }
        (LocalPlayerActionKind::ClaimWindow, PlayerAction::Chow { tile_ids }) => Some(
            apply_claim_window_action_in_room_state(room, seat_index, "chow", &tile_ids),
        ),
        (LocalPlayerActionKind::ClaimWindow, PlayerAction::Pung { tile_ids }) => Some(
            apply_claim_window_action_in_room_state(room, seat_index, "pung", &tile_ids),
        ),
        (
            LocalPlayerActionKind::PlayerMultiplierSelection,
            PlayerAction::SelectMultiplier { multiplier },
        ) => Some(apply_player_multiplier_selection_in_room_state(
            room, seat_index, multiplier,
        )),
        _ => None,
    }
}

fn active_turn_pass_tile_id(context: &EngineContext, seat_index: usize) -> Option<String> {
    let round = context.room.round_state.as_ref()?;
    let restricted_tile_key = round.restricted_discard_tile_key.as_deref();
    let player = round.players.get(seat_index)?;

    if let Some(drawn_tile_id) = context
        .room
        .pending_timeout
        .as_ref()
        .and_then(|timeout| timeout.drawn_tile_id.as_deref())
    {
        let drawn_tile = player
            .concealed_tiles
            .iter()
            .find(|tile| tile.tile_id == drawn_tile_id);
        if let Some(drawn_tile) = drawn_tile
            && Some(drawn_tile.tile_key.as_str()) != restricted_tile_key
        {
            return Some(drawn_tile.tile_id.clone());
        }
    }

    player
        .concealed_tiles
        .iter()
        .rev()
        .find(|tile| Some(tile.tile_key.as_str()) != restricted_tile_key)
        .map(|tile| tile.tile_id.clone())
}

fn active_turn_tile_kind(
    context: &EngineContext,
    seat_index: usize,
    tile_id: &str,
) -> Option<String> {
    context
        .room
        .round_state
        .as_ref()?
        .players
        .get(seat_index)?
        .concealed_tiles
        .iter()
        .find(|tile| tile.tile_id == tile_id)
        .map(|tile| tile.kind.clone())
}
