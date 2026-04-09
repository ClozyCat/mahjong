use crate::core::action::PlayerAction;
use crate::core::ids::Seat;
use crate::core::state::PendingAction;

use super::command::EngineContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalPlayerActionKind {
    Hu,
    Flower,
    Discard,
    ClaimWindow,
    SelfKong,
    RobKongPass,
    OpeningFlowersPass,
    SkillDraftSelection,
    ActivateSkill,
}

pub fn classify_local_player_action(
    context: &EngineContext,
    actor: Seat,
    action: &PlayerAction,
) -> Option<LocalPlayerActionKind> {
    match action {
        PlayerAction::Hu => Some(LocalPlayerActionKind::Hu),
        PlayerAction::Flower { .. } => Some(LocalPlayerActionKind::Flower),
        PlayerAction::Discard { tile_id } => discard_supported_locally(context, actor, tile_id)
            .then_some(LocalPlayerActionKind::Discard),
        PlayerAction::Chow { .. } => claim_window_action_supported(context, actor, "chow")
            .then_some(LocalPlayerActionKind::ClaimWindow),
        PlayerAction::Pung { .. } => claim_window_action_supported(context, actor, "pung")
            .then_some(LocalPlayerActionKind::ClaimWindow),
        PlayerAction::Kong { .. } => {
            if claim_window_action_supported(context, actor, "kong") {
                Some(LocalPlayerActionKind::ClaimWindow)
            } else if self_kong_supported(context, actor) {
                Some(LocalPlayerActionKind::SelfKong)
            } else {
                None
            }
        }
        PlayerAction::Pass => {
            if claim_window_action_supported(context, actor, "pass") {
                Some(LocalPlayerActionKind::ClaimWindow)
            } else if rob_kong_pass_supported(context, actor) {
                Some(LocalPlayerActionKind::RobKongPass)
            } else if opening_flowers_pass_supported(context, actor) {
                Some(LocalPlayerActionKind::OpeningFlowersPass)
            } else {
                None
            }
        }
        PlayerAction::SelectSkill { .. } | PlayerAction::DeclineSkillSelection => {
            skill_draft_selection_supported(context, actor)
                .then_some(LocalPlayerActionKind::SkillDraftSelection)
        }
        PlayerAction::ActivateSkill { .. } => Some(LocalPlayerActionKind::ActivateSkill),
    }
}

pub fn discard_supported_locally(context: &EngineContext, actor: Seat, tile_id: &str) -> bool {
    if context.room.phase != "playing" {
        return false;
    }
    let Some(round) = context.room.round_state.as_ref() else {
        return false;
    };
    if round.current_actor != actor || round.pending_action.is_some() {
        return false;
    }
    if round.wall.live_tiles_remaining() == 0 {
        return false;
    }
    let Some(player) = round.players.get(actor) else {
        return false;
    };
    let Some(tile) = player
        .concealed_tiles
        .iter()
        .find(|tile| tile.tile_id == tile_id)
    else {
        return false;
    };
    match round.restricted_discard_tile_key.as_deref() {
        Some(restricted) => tile.tile_key != restricted,
        None => true,
    }
}

fn claim_window_action_supported(context: &EngineContext, actor: Seat, action_type: &str) -> bool {
    let Some(round) = context.room.round_state.as_ref() else {
        return false;
    };
    let Some(PendingAction::ClaimWindow(claim)) = round.pending_action.as_ref() else {
        return false;
    };
    let Some(allowed_claims) = claim.claim_window.get(actor) else {
        return false;
    };
    if allowed_claims.is_empty() || claim.responded_seats.contains(&actor) {
        return false;
    }
    action_type == "pass"
        || allowed_claims
            .iter()
            .any(|claim_type| claim_type == action_type)
}

fn rob_kong_pass_supported(context: &EngineContext, actor: Seat) -> bool {
    let Some(round) = context.room.round_state.as_ref() else {
        return false;
    };
    let Some(PendingAction::RobKongWindow(rob)) = round.pending_action.as_ref() else {
        return false;
    };
    rob.offered_hu_seats.contains(&actor) && !rob.responded_seats.contains(&actor)
}

fn opening_flowers_pass_supported(context: &EngineContext, actor: Seat) -> bool {
    if context.room.phase != "playing" {
        return false;
    }
    let Some(round) = context.room.round_state.as_ref() else {
        return false;
    };
    if round.current_actor != actor {
        return false;
    }
    matches!(round.pending_action, Some(PendingAction::OpeningFlowers(_)))
        && round
            .players
            .get(actor)
            .map(|player| {
                player
                    .concealed_tiles
                    .iter()
                    .all(|tile| tile.kind != "flower")
            })
            .unwrap_or(false)
}

fn skill_draft_selection_supported(context: &EngineContext, actor: Seat) -> bool {
    if context.room.phase != "playing" {
        return false;
    }
    if context
        .room
        .pending_timeout
        .as_ref()
        .map(|timeout| timeout.kind.as_str())
        != Some("skill_draft")
    {
        return false;
    }
    context
        .room
        .round_state
        .as_ref()
        .and_then(|round| round.skill_draft.as_ref())
        .and_then(|draft| draft.offers_by_seat.get(&actor))
        .is_some_and(|offer| offer.status == crate::core::state::SkillDraftStatus::Pending)
}

fn self_kong_supported(context: &EngineContext, actor: Seat) -> bool {
    context.room.phase == "playing"
        && context.current_actor() == Some(actor)
        && context
            .room
            .pending_timeout
            .as_ref()
            .map(|timeout| timeout.kind.as_str())
            == Some("active_turn")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{LocalPlayerActionKind, classify_local_player_action, discard_supported_locally};
    use crate::core::action::PlayerAction;
    use crate::core::engine::EngineContext;
    use crate::core::state::RoomState;

    fn context(room: serde_json::Value) -> EngineContext {
        EngineContext::from_room_state(
            RoomState::from_room_value(&room).expect("room should parse"),
        )
    }

    fn tile(tile_id: &str, tile_key: &str, kind: &str) -> serde_json::Value {
        json!({
            "tile_id": tile_id,
            "tile_key": tile_key,
            "kind": kind,
            "suit": null,
            "rank": null,
            "name": tile_key,
        })
    }

    fn base_room() -> serde_json::Value {
        json!({
            "table_code": "ROOM42",
            "phase": "playing",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "seats": [],
            "match_state": null,
            "round_state": {
                "round_id": "east-1",
                "dealer_seat": 0,
                "round_wind": "east",
                "current_actor": 0,
                "phase": "playing",
                "wall": {
                    "tiles": [tile("draw#0", "w1", "suit"), tile("tail#0", "w9", "suit")],
                    "head_index": 0,
                    "tail_index": 1
                },
                "players": [
                    {
                        "seat": 0,
                        "concealed_tiles": [tile("east#discard", "east", "wind"), tile("w3#0", "w3", "suit")],
                        "melds": [],
                        "flowers": [],
                        "discards": [],
                        "skill_loadout": {"equipped": []}
                    },
                    {
                        "seat": 1,
                        "concealed_tiles": [],
                        "melds": [],
                        "flowers": [],
                        "discards": [],
                        "skill_loadout": {"equipped": []}
                    }
                ],
                "last_discard": null,
                "pending_action": null,
                "settlement": null,
                "version": 1,
                "score_trackers": {
                    "kong_entries": [],
                    "opening_flowers_completed": true
                },
                "last_action_context": {
                    "kind": "draw",
                    "seat": 0,
                    "tile_id": "east#discard",
                    "from_kong_replacement": false,
                    "was_last_live_tile": false,
                    "was_last_discard": false
                },
                "effect_state": null,
                "restricted_discard_tile_key": null,
                "skill_trackers": null
            },
            "pending_timeout": {
                "kind": "active_turn",
                "seat_index": 0,
                "deadline_at": null,
                "drawn_tile_id": "east#discard"
            },
            "continue_action": null
        })
    }

    #[test]
    fn classifies_local_discard_from_typed_state() {
        let context = context(base_room());
        let action = PlayerAction::Discard {
            tile_id: "east#discard".to_string(),
        };

        assert!(discard_supported_locally(&context, 0, "east#discard"));
        assert_eq!(
            classify_local_player_action(&context, 0, &action),
            Some(LocalPlayerActionKind::Discard)
        );
    }

    #[test]
    fn rejects_restricted_discard_in_validation() {
        let mut room = base_room();
        room["round_state"]["restricted_discard_tile_key"] = json!("east");
        let context = context(room);

        assert!(!discard_supported_locally(&context, 0, "east#discard"));
    }

    #[test]
    fn classifies_rob_kong_pass_only_for_offered_seat() {
        let mut room = base_room();
        room["round_state"]["pending_action"] = json!({
            "type": "rob_kong_window",
            "actor_seat": 0,
            "tile_id": "w3#0",
            "tile_key": "w3",
            "meld_index": 0,
            "offered_hu_seats": [1],
            "responded_seats": []
        });
        room["pending_timeout"]["kind"] = json!("claim_window");
        let context = context(room);

        assert_eq!(
            classify_local_player_action(&context, 1, &PlayerAction::Pass),
            Some(LocalPlayerActionKind::RobKongPass)
        );
        assert_eq!(
            classify_local_player_action(&context, 0, &PlayerAction::Pass),
            None
        );
    }

    #[test]
    fn opening_flowers_pass_requires_no_remaining_flower() {
        let mut room = base_room();
        room["round_state"]["pending_action"] = json!({
            "type": "opening_flowers",
            "dealer_seat": 0
        });
        room["pending_timeout"]["kind"] = json!("opening_flowers");
        let context_without_flower = context(room.clone());
        assert_eq!(
            classify_local_player_action(&context_without_flower, 0, &PlayerAction::Pass),
            Some(LocalPlayerActionKind::OpeningFlowersPass)
        );

        room["round_state"]["players"][0]["concealed_tiles"] =
            json!([tile("f1#0", "f1", "flower")]);
        let context_with_flower = context(room);
        assert_eq!(
            classify_local_player_action(&context_with_flower, 0, &PlayerAction::Pass),
            None
        );
    }

    #[test]
    fn claim_window_validation_requires_unresponded_offered_action() {
        let mut room = base_room();
        room["round_state"]["pending_action"] = json!({
            "type": "claim_window",
            "discarder_seat": 0,
            "claim_window": [[], ["pung", "hu"], [], []],
            "responded_seats": [1],
            "claim_responses": []
        });
        room["pending_timeout"]["kind"] = json!("claim_window");
        let context = context(room);

        assert_eq!(
            classify_local_player_action(
                &context,
                1,
                &PlayerAction::Pung {
                    tile_ids: vec!["w3#a".to_string(), "w3#b".to_string()],
                }
            ),
            None
        );
    }
}
