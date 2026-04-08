use serde_json::Value;

use crate::core::action::{GameCommand, PlayerAction};
use crate::rules::{
    skills,
    standard::{
        actions::{
            apply_claim_window_action, apply_discard_action, apply_rob_kong_pass,
            try_handle_self_kong_action,
        },
        flow::{apply_flower_action, apply_opening_flowers_pass},
        win::{apply_hu_settlement, compute_hu_settlement, hu_action_hint},
    },
};

use super::{EngineContext, EngineOutput, LocalPlayerActionKind, classify_local_player_action};

pub fn try_handle_command(
    room: &mut Value,
    command: GameCommand,
) -> Option<Result<EngineOutput, String>> {
    let context = EngineContext::from_legacy_room(room).ok()?;
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
            Some(apply_hu_action(room, seat_index).map(EngineOutput::from_emitted_messages))
        }
        (LocalPlayerActionKind::Flower, PlayerAction::Flower { tile_ids }) => Some(
            apply_flower_action(room, seat_index, &tile_ids)
                .map(EngineOutput::from_emitted_messages),
        ),
        (LocalPlayerActionKind::Discard, PlayerAction::Discard { tile_id }) => Some(
            apply_discard_action(room, seat_index, &tile_id)
                .map(EngineOutput::from_emitted_messages),
        ),
        (LocalPlayerActionKind::ClaimWindow, PlayerAction::Kong { tile_ids }) => Some(
            apply_claim_window_action(room, seat_index, "kong", &tile_ids)
                .map(EngineOutput::from_emitted_messages),
        ),
        (LocalPlayerActionKind::SelfKong, PlayerAction::Kong { tile_ids }) => {
            try_handle_self_kong_action(room, seat_index, &tile_ids)
                .map(|result| result.map(EngineOutput::from_emitted_messages))
        }
        (LocalPlayerActionKind::ClaimWindow, PlayerAction::Pass) => {
            let declined_hu = hu_action_hint(room, seat_index).is_some();
            Some(
                apply_claim_window_action(room, seat_index, "pass", &[]).and_then(
                    |mut emitted_messages| {
                        if declined_hu {
                            let events = skills::decline_hu_events(&context.room, seat_index)?;
                            emitted_messages.extend(
                                skills::apply_passive_skill_events_to_legacy_room(room, &events)?,
                            );
                        }
                        Ok(EngineOutput::from_emitted_messages(emitted_messages))
                    },
                ),
            )
        }
        (LocalPlayerActionKind::RobKongPass, PlayerAction::Pass) => {
            let declined_hu = hu_action_hint(room, seat_index).is_some();
            Some(
                apply_rob_kong_pass(room, seat_index).and_then(|mut emitted_messages| {
                    if declined_hu {
                        let events = skills::decline_hu_events(&context.room, seat_index)?;
                        emitted_messages.extend(skills::apply_passive_skill_events_to_legacy_room(
                            room, &events,
                        )?);
                    }
                    Ok(EngineOutput::from_emitted_messages(emitted_messages))
                }),
            )
        }
        (LocalPlayerActionKind::OpeningFlowersPass, PlayerAction::Pass) => Some(
            apply_opening_flowers_pass(room, seat_index).map(EngineOutput::from_emitted_messages),
        ),
        (LocalPlayerActionKind::ClaimWindow, PlayerAction::Chow { tile_ids }) => Some(
            apply_claim_window_action(room, seat_index, "chow", &tile_ids)
                .map(EngineOutput::from_emitted_messages),
        ),
        (LocalPlayerActionKind::ClaimWindow, PlayerAction::Pung { tile_ids }) => Some(
            apply_claim_window_action(room, seat_index, "pung", &tile_ids)
                .map(EngineOutput::from_emitted_messages),
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
                    let emitted_messages = skills::apply_skill_events_to_legacy_room(
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

fn apply_hu_action(room: &mut Value, seat_index: usize) -> Result<Vec<Value>, String> {
    let Some(hu_context) = hu_action_hint(room, seat_index) else {
        return Err("invalid_action".to_string());
    };
    let settlement = compute_hu_settlement(room, seat_index, hu_context)?;
    apply_hu_settlement(room, seat_index, hu_context, settlement)
}
