use serde_json::Value;

use crate::core::action::{GameCommand, PlayerAction};
use crate::core::state::RoomState;
use crate::rules::{
    skills,
    standard::{
        actions::{
            apply_claim_window_action, apply_discard_action_output, apply_rob_kong_pass,
            try_handle_self_kong_action_output,
        },
        flow::{apply_flower_action_output, apply_opening_flowers_pass_output},
        win::{apply_hu_settlement_output, compute_hu_settlement, hu_action_hint},
    },
};

use super::{EngineContext, EngineOutput, LocalPlayerActionKind, classify_local_player_action};

pub fn try_handle_command(
    room: &mut Value,
    command: GameCommand,
) -> Option<Result<EngineOutput, String>> {
    let context = RoomState::from_room_value(room)
        .ok()
        .map(EngineContext::from_room_state)?;
    match command {
        GameCommand::PlayerAction { actor, action } => {
            try_handle_player_action_command(room, &context, actor, action)
        }
        _ => None,
    }
}

fn try_handle_player_action_command(
    room: &mut Value,
    context: &EngineContext,
    seat_index: usize,
    action: PlayerAction,
) -> Option<Result<EngineOutput, String>> {
    let action_kind = classify_local_player_action(context, seat_index, &action)?;
    match (action_kind, action) {
        (LocalPlayerActionKind::Hu, PlayerAction::Hu) => {
            Some(apply_hu_action(room, seat_index))
        }
        (LocalPlayerActionKind::Flower, PlayerAction::Flower { tile_ids }) => Some(
            apply_flower_action_output(room, seat_index, &tile_ids),
        ),
        (LocalPlayerActionKind::Discard, PlayerAction::Discard { tile_id }) => {
            Some(apply_discard_action_output(room, seat_index, &tile_id))
        }
        (LocalPlayerActionKind::ClaimWindow, PlayerAction::Kong { tile_ids }) => Some(
            apply_claim_window_action(room, seat_index, "kong", &tile_ids),
        ),
        (LocalPlayerActionKind::SelfKong, PlayerAction::Kong { tile_ids }) => {
            try_handle_self_kong_action_output(room, seat_index, &tile_ids)
        }
        (LocalPlayerActionKind::ClaimWindow, PlayerAction::Pass) => {
            let declined_hu = hu_action_hint(room, seat_index).is_some();
            Some(
                apply_claim_window_action(room, seat_index, "pass", &[]).and_then(|mut output| {
                    if declined_hu {
                        let events = skills::decline_hu_events(&context.room, seat_index)?;
                        output.emitted_messages.extend(
                            skills::apply_passive_skill_events_to_room(room, &events)?,
                        );
                        output.events.extend(events);
                    }
                    Ok(output)
                }),
            )
        }
        (LocalPlayerActionKind::RobKongPass, PlayerAction::Pass) => {
            let declined_hu = hu_action_hint(room, seat_index).is_some();
            Some(
                apply_rob_kong_pass(room, seat_index).and_then(|mut output| {
                    if declined_hu {
                        let events = skills::decline_hu_events(&context.room, seat_index)?;
                        output.emitted_messages.extend(
                            skills::apply_passive_skill_events_to_room(room, &events)?,
                        );
                        output.events.extend(events);
                    }
                    Ok(output)
                }),
            )
        }
        (LocalPlayerActionKind::OpeningFlowersPass, PlayerAction::Pass) => Some(
            apply_opening_flowers_pass_output(room, seat_index),
        ),
        (LocalPlayerActionKind::ClaimWindow, PlayerAction::Chow { tile_ids }) => Some(
            apply_claim_window_action(room, seat_index, "chow", &tile_ids),
        ),
        (LocalPlayerActionKind::ClaimWindow, PlayerAction::Pung { tile_ids }) => Some(
            apply_claim_window_action(room, seat_index, "pung", &tile_ids),
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
                    let emitted_messages = skills::apply_skill_events_to_room(
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

fn apply_hu_action(room: &mut Value, seat_index: usize) -> Result<EngineOutput, String> {
    let Some(hu_context) = hu_action_hint(room, seat_index) else {
        return Err("invalid_action".to_string());
    };
    let settlement = compute_hu_settlement(room, seat_index, hu_context)?;
    apply_hu_settlement_output(room, seat_index, hu_context, settlement)
}
