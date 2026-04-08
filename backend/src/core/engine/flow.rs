use crate::core::action::{GameCommand, PlayerAction};
use crate::core::state::RoomState;
use crate::rules::{
    skills,
    standard::{
        actions::{
            apply_claim_window_action_in_room_state, apply_discard_action_output_in_room_state,
            apply_rob_kong_pass_in_room_state, try_handle_self_kong_action_output_in_room_state,
        },
        flow::{
            apply_flower_action_output_in_room_state,
            apply_opening_flowers_pass_output_in_room_state,
        },
        win::apply_hu_action_output_in_room_state,
    },
};

use super::{EngineContext, EngineOutput, LocalPlayerActionKind, classify_local_player_action};

#[cfg(test)]
pub fn try_handle_command(
    room: &mut serde_json::Value,
    command: GameCommand,
) -> Option<Result<EngineOutput, String>> {
    let mut room_state = RoomState::from_room_value(room)
        .ok()
        .map(EngineContext::from_room_state)?
        .room;
    let result = try_handle_command_in_room_state(&mut room_state, command).ok()?;
    *room = room_state.to_room_value().ok()?;
    result
}

pub fn try_handle_command_in_room_state(
    room: &mut RoomState,
    command: GameCommand,
) -> Result<Option<Result<EngineOutput, String>>, String> {
    let context = EngineContext::from_room_state(room.clone());
    match command {
        GameCommand::PlayerAction { actor, action } => {
            Ok(try_handle_player_action_command(room, &context, actor, action))
        }
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
        (LocalPlayerActionKind::Hu, PlayerAction::Hu) => Some(apply_hu_action_output_in_room_state(room, seat_index)),
        (LocalPlayerActionKind::Flower, PlayerAction::Flower { tile_ids }) => Some(
            apply_flower_action_output_in_room_state(room, seat_index, &tile_ids),
        ),
        (LocalPlayerActionKind::Discard, PlayerAction::Discard { tile_id }) => {
            Some(apply_discard_action_output_in_room_state(room, seat_index, &tile_id))
        }
        (LocalPlayerActionKind::ClaimWindow, PlayerAction::Kong { tile_ids }) => Some(
            apply_claim_window_action_in_room_state(room, seat_index, "kong", &tile_ids),
        ),
        (LocalPlayerActionKind::SelfKong, PlayerAction::Kong { tile_ids }) => {
            try_handle_self_kong_action_output_in_room_state(room, seat_index, &tile_ids)
                .ok()
                .flatten()
        }
        (LocalPlayerActionKind::ClaimWindow, PlayerAction::Pass) => {
            let declined_hu =
                crate::rules::standard::win::hu_action_hint_in_room_state(room, seat_index).is_some();
            Some(
                apply_claim_window_action_in_room_state(room, seat_index, "pass", &[]).and_then(|mut output| {
                    if declined_hu {
                        let events = skills::decline_hu_events(&context.room, seat_index)?;
                        output
                            .emitted_messages
                            .extend(skills::apply_passive_skill_events_to_room_in_room_state(room, &events)?);
                        output.events.extend(events);
                    }
                    Ok(output)
                }),
            )
        }
        (LocalPlayerActionKind::RobKongPass, PlayerAction::Pass) => {
            let declined_hu =
                crate::rules::standard::win::hu_action_hint_in_room_state(room, seat_index).is_some();
            Some(
                apply_rob_kong_pass_in_room_state(room, seat_index).and_then(|mut output| {
                    if declined_hu {
                        let events = skills::decline_hu_events(&context.room, seat_index)?;
                        output
                            .emitted_messages
                            .extend(skills::apply_passive_skill_events_to_room_in_room_state(room, &events)?);
                        output.events.extend(events);
                    }
                    Ok(output)
                }),
            )
        }
        (LocalPlayerActionKind::OpeningFlowersPass, PlayerAction::Pass) => Some(
            apply_opening_flowers_pass_output_in_room_state(room, seat_index),
        ),
        (LocalPlayerActionKind::ClaimWindow, PlayerAction::Chow { tile_ids }) => Some(
            apply_claim_window_action_in_room_state(room, seat_index, "chow", &tile_ids),
        ),
        (LocalPlayerActionKind::ClaimWindow, PlayerAction::Pung { tile_ids }) => Some(
            apply_claim_window_action_in_room_state(room, seat_index, "pung", &tile_ids),
        ),
        (
            LocalPlayerActionKind::ActivateSkill,
            PlayerAction::ActivateSkill {
                skill_id,
                target,
                tile_ids,
            },
        ) => Some(
            skills::activate_skill(&context.room, seat_index, &skill_id, target, &tile_ids)
                .and_then(|events| {
                    let emitted_messages = skills::apply_skill_events_to_room_in_room_state(
                        room, seat_index, &skill_id, &events,
                    )?;
                    Ok(EngineOutput {
                        events,
                        emitted_messages,
                    })
                }),
        ),
        _ => None,
    }
}
