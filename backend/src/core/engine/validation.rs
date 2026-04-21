use crate::core::action::PlayerAction;
use crate::core::ids::Seat;
use crate::core::state::PendingAction;
use crate::rules::standard::ready_hand::can_declare_ready_hand_with_tile_id;

use super::command::EngineContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalPlayerActionKind {
    Hu,
    Flower,
    Discard,
    ReadyHand,
    ClaimWindow,
    SelfKong,
    RobKongPass,
}

pub fn classify_local_player_action(
    context: &EngineContext,
    actor: Seat,
    action: &PlayerAction,
) -> Option<LocalPlayerActionKind> {
    match action {
        PlayerAction::Hu => Some(LocalPlayerActionKind::Hu),
        PlayerAction::Flower { .. } => flower_supported_locally(context, actor)
            .then_some(LocalPlayerActionKind::Flower),
        PlayerAction::Discard { tile_id } => discard_supported_locally(context, actor, tile_id)
            .then_some(LocalPlayerActionKind::Discard),
        PlayerAction::ReadyHand { tile_id } => ready_hand_supported_locally(context, actor, tile_id)
            .then_some(LocalPlayerActionKind::ReadyHand),
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
            } else {
                None
            }
        }
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
    let Some(player) = round.players.get(actor) else {
        return false;
    };
    if player.is_ready_hand {
        return false;
    }
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

fn ready_hand_supported_locally(context: &EngineContext, actor: Seat, tile_id: &str) -> bool {
    can_declare_ready_hand_with_tile_id(&context.room, actor, tile_id)
}

fn flower_supported_locally(context: &EngineContext, actor: Seat) -> bool {
    if context.room.phase != "playing" {
        return false;
    }
    let Some(round) = context.room.round_state.as_ref() else {
        return false;
    };
    if round.current_actor != actor || round.pending_action.is_some() {
        return false;
    }
    !round.players.get(actor).is_some_and(|player| player.is_ready_hand)
}

fn claim_window_action_supported(context: &EngineContext, actor: Seat, action_type: &str) -> bool {
    let Some(round) = context.room.round_state.as_ref() else {
        return false;
    };
    let Some(PendingAction::ClaimWindow(claim)) = round.pending_action.as_ref() else {
        return false;
    };
    let is_ready_hand = round.players.get(actor).is_some_and(|player| player.is_ready_hand);
    let Some(allowed_claims) = claim.claim_window.get(actor) else {
        return false;
    };
    if allowed_claims.is_empty() || claim.responded_seats.contains(&actor) {
        return false;
    }
    if is_ready_hand {
        return matches!(action_type, "hu" | "kong")
            && allowed_claims
                .iter()
                .any(|claim_type| claim_type == action_type);
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
    if round.players.get(actor).is_some_and(|player| player.is_ready_hand) {
        return false;
    }
    let Some(PendingAction::RobKongWindow(rob)) = round.pending_action.as_ref() else {
        return false;
    };
    rob.offered_hu_seats.contains(&actor) && !rob.responded_seats.contains(&actor)
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
                        "discards": []
                    },
                    {
                        "seat": 1,
                        "concealed_tiles": [],
                        "melds": [],
                        "flowers": [],
                        "discards": []
                    }
                ],
                "last_discard": null,
                "pending_action": null,
                "settlement": null,
                "version": 1,
                "score_trackers": {
                    "kong_entries": []
                },
                "last_action_context": {
                    "kind": "draw",
                    "seat": 0,
                    "tile_id": "east#discard",
                    "from_kong_replacement": false,
                    "was_last_live_tile": false,
                    "was_last_discard": false
                },
                "restricted_discard_tile_key": null
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
    fn allows_discard_after_last_live_tile_is_drawn() {
        let mut room = base_room();
        room["round_state"]["wall"]["head_index"] = json!(1);
        room["round_state"]["wall"]["tail_index"] = json!(0);
        let context = context(room);

        assert!(discard_supported_locally(&context, 0, "east#discard"));
    }

    #[test]
    fn rejects_manual_discard_after_ready_hand_declaration() {
        let mut room = base_room();
        room["round_state"]["players"][0]["is_ready_hand"] = json!(true);
        let context = context(room);

        assert!(!discard_supported_locally(&context, 0, "east#discard"));
        assert_eq!(
            classify_local_player_action(
                &context,
                0,
                &PlayerAction::Discard {
                    tile_id: "east#discard".to_string(),
                }
            ),
            None
        );
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

    #[test]
    fn allows_claim_window_kong_after_ready_hand_declaration() {
        let mut room = base_room();
        room["round_state"]["players"][1]["is_ready_hand"] = json!(true);
        room["round_state"]["players"][1]["concealed_tiles"] = json!([
            tile("w3#a", "w3", "suit"),
            tile("w3#b", "w3", "suit"),
            tile("w3#c", "w3", "suit")
        ]);
        room["round_state"]["pending_action"] = json!({
            "type": "claim_window",
            "discarder_seat": 0,
            "claim_window": [[], ["kong", "hu"], [], []],
            "responded_seats": [],
            "claim_responses": []
        });
        room["pending_timeout"]["kind"] = json!("claim_window");
        let context = context(room);

        assert_eq!(
            classify_local_player_action(
                &context,
                1,
                &PlayerAction::Kong {
                    tile_ids: vec![
                        "w3#a".to_string(),
                        "w3#b".to_string(),
                        "w3#c".to_string(),
                    ],
                }
            ),
            Some(LocalPlayerActionKind::ClaimWindow)
        );
    }

    #[test]
    fn allows_self_kong_after_ready_hand_declaration() {
        let mut room = base_room();
        room["round_state"]["players"][0]["is_ready_hand"] = json!(true);
        room["round_state"]["players"][0]["concealed_tiles"] = json!([
            tile("w3#0", "w3", "suit"),
            tile("w3#1", "w3", "suit"),
            tile("w3#2", "w3", "suit"),
            tile("w3#3", "w3", "suit")
        ]);
        let context = context(room);

        assert_eq!(
            classify_local_player_action(
                &context,
                0,
                &PlayerAction::Kong {
                    tile_ids: vec![
                        "w3#0".to_string(),
                        "w3#1".to_string(),
                        "w3#2".to_string(),
                        "w3#3".to_string(),
                    ],
                }
            ),
            Some(LocalPlayerActionKind::SelfKong)
        );
    }
}
