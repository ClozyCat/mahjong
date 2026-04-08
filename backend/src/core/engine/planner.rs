use rand::SeedableRng;
use rand::seq::SliceRandom;
use serde_json::{Value, json};

use crate::core::engine::reducer::LegacyRoomMutation;
use crate::core::state::effect::EffectState;
use crate::core::state::{
    ClaimWindowAction, LastActionContext, OpeningFlowersAction, PendingAction, PendingTimeout,
    PlayerRoundState, RobKongWindowAction, RoomState, RoundScoreTrackers, RoundSettlement,
    RoundState, RuleRuntimeState, WallState,
};
use crate::core::tile::Tile;

const MAX_SEATS: usize = 4;

#[derive(Debug, Clone)]
pub struct PlannedFlowerAction {
    pub mutations: Vec<LegacyRoomMutation>,
    pub flower_tile: Tile,
    pub replacement_tile: Tile,
}

#[derive(Debug, Clone)]
pub struct PlannedClaimWindowResponse {
    pub mutations: Vec<LegacyRoomMutation>,
    pub unresolved_seats: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct PlannedDiscardAction {
    pub discard_mutations: Vec<LegacyRoomMutation>,
    pub followup_mutations: Vec<LegacyRoomMutation>,
    pub discarded_tile: Tile,
    pub needs_exhaustive_draw: bool,
}

#[derive(Debug, Clone)]
pub struct PlannedClaimWindowContinuation {
    pub mutations: Vec<LegacyRoomMutation>,
    pub needs_exhaustive_draw: bool,
}

pub fn plan_round_start_payload(
    dealer_seat: usize,
    round_wind: &str,
    round_id: String,
    enforce_minimum_eight_fan: bool,
    seed: u64,
) -> (RoundState, PendingTimeout) {
    let mut wall_tiles = full_tile_set();
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    wall_tiles.shuffle(&mut rng);

    let mut head_index = 0usize;
    let mut players = Vec::with_capacity(MAX_SEATS);
    for seat in 0..MAX_SEATS {
        let mut concealed_tiles = Vec::with_capacity(13);
        for _ in 0..13 {
            concealed_tiles.push(wall_tiles[head_index].clone());
            head_index += 1;
        }
        players.push(PlayerRoundState {
            seat,
            concealed_tiles,
            melds: Vec::new(),
            flowers: Vec::new(),
            discards: Vec::new(),
            skill_loadout: Default::default(),
        });
    }

    let current_actor = dealer_seat;
    let mut pending_action = None;
    let opening_completed = if any_concealed_flower(&players) {
        pending_action = Some(PendingAction::OpeningFlowers(OpeningFlowersAction {
            dealer_seat,
        }));
        false
    } else {
        true
    };

    let draw_tile = wall_tiles[head_index].clone();
    head_index += 1;
    players[current_actor]
        .concealed_tiles
        .push(draw_tile.clone());

    if opening_completed {
        pending_action = None;
    }

    let round_state = RoundState {
        round_id,
        dealer_seat,
        round_wind: round_wind.to_string(),
        current_actor,
        phase: "playing".to_string(),
        wall: WallState {
            tiles: wall_tiles,
            head_index,
            tail_index: 143,
        },
        players,
        last_discard: None,
        pending_action,
        settlement: None,
        version: 1,
        score_trackers: RoundScoreTrackers {
            kong_entries: Vec::new(),
            opening_flowers_completed: opening_completed,
        },
        last_action_context: LastActionContext {
            kind: "draw".to_string(),
            seat: current_actor,
            tile_id: Some(draw_tile.tile_id.clone()),
            from_kong_replacement: false,
            was_last_live_tile: false,
            was_last_discard: false,
        },
        rule_state: RuleRuntimeState {
            enforce_minimum_eight_fan,
        },
        effect_state: EffectState::default(),
        restricted_discard_tile_key: None,
        skill_trackers: Default::default(),
    };

    let pending_timeout = if opening_completed {
        PendingTimeout {
            kind: "active_turn".to_string(),
            seat_index: current_actor,
            deadline_at: Some(deadline_iso()),
            drawn_tile_id: Some(draw_tile.tile_id.clone()),
        }
    } else {
        PendingTimeout {
            kind: "opening_flowers".to_string(),
            seat_index: current_actor,
            deadline_at: Some(deadline_iso()),
            drawn_tile_id: player_first_flower_tile_id_from_player(
                &round_state.players[current_actor],
            ),
        }
    };

    (round_state, pending_timeout)
}

pub fn compute_pending_timeout_value(
    state: &RoomState,
    deadline_at: String,
) -> Option<PendingTimeout> {
    if state.phase != "playing" {
        return None;
    }
    let Some(round) = state.round_state.as_ref() else {
        return None;
    };
    match round.pending_action.as_ref() {
        Some(PendingAction::OpeningFlowers(_)) => Some(PendingTimeout {
            kind: "opening_flowers".to_string(),
            seat_index: round.current_actor,
            deadline_at: Some(deadline_at),
            drawn_tile_id: player_first_flower_tile_id(state, round.current_actor),
        }),
        Some(PendingAction::ClaimWindow(claim)) => Some(PendingTimeout {
            kind: "claim_window".to_string(),
            seat_index: claim.discarder_seat,
            deadline_at: Some(deadline_at),
            drawn_tile_id: None,
        }),
        Some(PendingAction::RobKongWindow(rob)) => Some(PendingTimeout {
            kind: "claim_window".to_string(),
            seat_index: rob.actor_seat,
            deadline_at: Some(deadline_at),
            drawn_tile_id: None,
        }),
        _ => Some(PendingTimeout {
            kind: "active_turn".to_string(),
            seat_index: round.current_actor,
            deadline_at: Some(deadline_at),
            drawn_tile_id: active_turn_drawn_tile_id(state, round.current_actor),
        }),
    }
}

pub fn plan_advance_opening_flowers(
    state: &RoomState,
    seat_index: usize,
) -> Vec<LegacyRoomMutation> {
    let dealer_seat = state
        .round_state
        .as_ref()
        .map(|round| round.dealer_seat)
        .unwrap_or(0);
    let next_seat = (seat_index + 1) % MAX_SEATS;
    let seat_has_flower = player_has_concealed_flower(state, seat_index);

    if seat_has_flower {
        return vec![
            LegacyRoomMutation::SetRoundCurrentActor { seat_index },
            LegacyRoomMutation::SetRoundPendingAction {
                pending_action: Some(PendingAction::OpeningFlowers(OpeningFlowersAction {
                    dealer_seat,
                })),
            },
        ];
    }
    if next_seat == dealer_seat {
        let mut trackers = serde_json::to_value(
            state
                .round_state
                .as_ref()
                .map(|round| round.score_trackers.clone())
                .unwrap_or_default(),
        )
        .unwrap_or_else(|_| json!({}))
        .as_object()
        .cloned()
        .unwrap_or_default();
        trackers.insert("opening_flowers_completed".to_string(), Value::Bool(true));
        return vec![
            LegacyRoomMutation::SetRoundCurrentActor {
                seat_index: dealer_seat,
            },
            LegacyRoomMutation::SetRoundPendingAction {
                pending_action: None,
            },
            LegacyRoomMutation::SetRoundScoreTrackers {
                score_trackers: crate::core::state::RoundScoreTrackers::from_legacy_value(Some(
                    &Value::Object(trackers),
                )),
            },
        ];
    }
    vec![
        LegacyRoomMutation::SetRoundCurrentActor {
            seat_index: next_seat,
        },
        LegacyRoomMutation::SetRoundPendingAction {
            pending_action: Some(PendingAction::OpeningFlowers(OpeningFlowersAction {
                dealer_seat,
            })),
        },
    ]
}

pub fn plan_flower_action(
    state: &RoomState,
    seat_index: usize,
    tile_id: &str,
) -> Result<PlannedFlowerAction, String> {
    let round = state
        .round_state
        .as_ref()
        .ok_or_else(|| "round_not_ready".to_string())?;
    let opening_flowers_mode =
        matches!(round.pending_action, Some(PendingAction::OpeningFlowers(_)));

    let flower_tile = round
        .players
        .get(seat_index)
        .and_then(|player| {
            player
                .concealed_tiles
                .iter()
                .find(|tile| tile.tile_id == tile_id)
        })
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;
    if flower_tile.kind != "flower" {
        return Err("invalid_action".to_string());
    }

    let replacement_tile =
        replacement_tile_from_tail(state).ok_or_else(|| "round_not_ready".to_string())?;

    let mut mutations = vec![
        LegacyRoomMutation::RemovePlayerConcealedTileById {
            seat_index,
            tile_id: tile_id.to_string(),
        },
        LegacyRoomMutation::PushPlayerFlower {
            seat_index,
            tile: flower_tile.clone(),
        },
        LegacyRoomMutation::RetreatWallTail,
        LegacyRoomMutation::PushPlayerConcealedTile {
            seat_index,
            tile: replacement_tile.clone(),
        },
        LegacyRoomMutation::SetRoundLastActionContext {
            context: LastActionContext {
                kind: "replacement_draw".to_string(),
                seat: seat_index,
                tile_id: Some(replacement_tile.tile_id.clone()),
                from_kong_replacement: false,
                was_last_live_tile: false,
                was_last_discard: false,
            },
        },
        LegacyRoomMutation::IncrementRoundVersion,
    ];

    if opening_flowers_mode {
        mutations.extend(plan_advance_opening_flowers(state, seat_index));
    }

    Ok(PlannedFlowerAction {
        mutations,
        flower_tile,
        replacement_tile,
    })
}

pub fn plan_claim_window_response(
    state: &RoomState,
    seat_index: usize,
    action_type: &str,
    tile_ids: &[String],
) -> Result<PlannedClaimWindowResponse, String> {
    let round = state
        .round_state
        .as_ref()
        .ok_or_else(|| "invalid_action".to_string())?;
    let claim = match round.pending_action.as_ref() {
        Some(PendingAction::ClaimWindow(claim)) => claim,
        _ => return Err("invalid_action".to_string()),
    };

    let allowed_claims = claim
        .claim_window
        .get(seat_index)
        .cloned()
        .unwrap_or_default();
    if allowed_claims.is_empty() {
        return Err("invalid_action".to_string());
    }
    if claim.responded_seats.contains(&seat_index) {
        return Err("invalid_action".to_string());
    }
    if action_type != "pass"
        && !allowed_claims
            .iter()
            .any(|claim_type| claim_type == action_type)
    {
        return Err("invalid_action".to_string());
    }

    let discarder_seat = claim.discarder_seat;
    let mut responded_seats = claim.responded_seats.clone();
    responded_seats.push(seat_index);

    let mut claim_responses = claim.claim_responses.clone();
    if action_type != "pass" {
        claim_responses.push(json!({
            "seat": seat_index,
            "type": action_type,
            "tiles": tile_ids,
        }));
        if let Some(winning_claim) = resolve_claims(&claim_responses, discarder_seat) {
            for (other_seat, claims) in claim.claim_window.iter().enumerate() {
                if claims.is_empty() || responded_seats.contains(&other_seat) {
                    continue;
                }
                if !seat_can_beat_recorded_claim(other_seat, claims, &winning_claim, discarder_seat)
                {
                    responded_seats.push(other_seat);
                }
            }
        }
    }

    let unresolved_seats = offered_claim_seats(&claim.claim_window)
        .into_iter()
        .filter(|offered_seat| !responded_seats.contains(offered_seat))
        .collect();

    Ok(PlannedClaimWindowResponse {
        mutations: vec![
            LegacyRoomMutation::SetRoundPendingAction {
                pending_action: Some(PendingAction::ClaimWindow(ClaimWindowAction {
                    discarder_seat,
                    claim_window: claim.claim_window.clone(),
                    responded_seats,
                    claim_responses,
                })),
            },
            LegacyRoomMutation::IncrementRoundVersion,
        ],
        unresolved_seats,
    })
}

pub fn plan_discard_action(
    state: &RoomState,
    seat_index: usize,
    tile_id: &str,
    claim_window: Vec<Vec<String>>,
    previous_was_last_live_tile: bool,
) -> Result<PlannedDiscardAction, String> {
    let round = state
        .round_state
        .as_ref()
        .ok_or_else(|| "round_not_ready".to_string())?;

    let discarded_tile = round
        .players
        .get(seat_index)
        .and_then(|player| {
            player
                .concealed_tiles
                .iter()
                .find(|tile| tile.tile_id == tile_id)
        })
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;

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
            tile: Some(discarded_tile.clone()),
        },
    ];

    let has_claim = claim_window.iter().any(|claims| !claims.is_empty());
    let next_actor = (seat_index + 1) % MAX_SEATS;
    let drawn_tile = if has_claim {
        None
    } else {
        peek_draw_for_turn(state)
    };
    let needs_exhaustive_draw = !has_claim && drawn_tile.is_none();
    let was_last_live_tile = if has_claim {
        false
    } else {
        live_tiles_remaining_after_head_draw(state) <= 1
    };

    let mut followup_mutations = vec![
        LegacyRoomMutation::SetRoundPendingAction {
            pending_action: if has_claim {
                Some(PendingAction::ClaimWindow(ClaimWindowAction {
                    discarder_seat: seat_index,
                    claim_window,
                    responded_seats: vec![],
                    claim_responses: vec![],
                }))
            } else {
                None
            },
        },
        LegacyRoomMutation::SetRoundRestrictedDiscardTileKey { tile_key: None },
        LegacyRoomMutation::SetRoundLastActionContext {
            context: if has_claim {
                LastActionContext {
                    kind: "discard".to_string(),
                    seat: seat_index,
                    tile_id: Some(discarded_tile.tile_id.clone()),
                    from_kong_replacement: false,
                    was_last_live_tile: false,
                    was_last_discard: previous_was_last_live_tile,
                }
            } else {
                LastActionContext {
                    kind: "draw".to_string(),
                    seat: next_actor,
                    tile_id: drawn_tile.as_ref().map(|tile| tile.tile_id.clone()),
                    from_kong_replacement: false,
                    was_last_live_tile,
                    was_last_discard: false,
                }
            },
        },
        LegacyRoomMutation::IncrementRoundVersion,
        LegacyRoomMutation::SetRoundCurrentActor {
            seat_index: if has_claim { seat_index } else { next_actor },
        },
    ];
    if let Some(tile) = drawn_tile {
        followup_mutations.insert(0, LegacyRoomMutation::AdvanceWallHead);
        followup_mutations.insert(
            1,
            LegacyRoomMutation::PushPlayerConcealedTile {
                seat_index: next_actor,
                tile,
            },
        );
    }

    Ok(PlannedDiscardAction {
        discard_mutations,
        followup_mutations,
        discarded_tile,
        needs_exhaustive_draw,
    })
}

pub fn plan_settlement_to_match(
    state: &RoomState,
    settlement: &RoundSettlement,
) -> Vec<LegacyRoomMutation> {
    let Some(round) = state.round_state.as_ref() else {
        return Vec::new();
    };
    let Some(match_state) = state.match_state.as_ref() else {
        return Vec::new();
    };
    if match_state.last_completed_round_id.as_deref() == Some(round.round_id.as_str()) {
        return Vec::new();
    }

    let mut cumulative_scores = std::collections::BTreeMap::new();
    for seat_index in 0..MAX_SEATS {
        let current = match_state
            .cumulative_scores
            .get(&seat_index)
            .copied()
            .unwrap_or(0);
        let delta = settlement
            .score_delta
            .total_delta_by_seat
            .get(&seat_index)
            .copied()
            .unwrap_or(0);
        cumulative_scores.insert(seat_index, current + delta);
    }

    vec![
        LegacyRoomMutation::SetMatchCumulativeScores { cumulative_scores },
        LegacyRoomMutation::SetMatchLastCompletedRoundId {
            round_id: Some(round.round_id.clone()),
        },
    ]
}

pub fn resolve_claims(claim_requests: &[Value], discarder_seat: usize) -> Option<Value> {
    let next_player = (discarder_seat + 1) % MAX_SEATS;
    let mut candidates = claim_requests
        .iter()
        .filter(|request| {
            let claim_type = request.get("type").and_then(Value::as_str);
            if !matches!(claim_type, Some("chow" | "pung" | "kong" | "hu")) {
                return false;
            }
            if claim_type == Some("chow")
                && request
                    .get("seat")
                    .and_then(Value::as_u64)
                    .map(|seat| seat as usize)
                    != Some(next_player)
            {
                return false;
            }
            true
        })
        .cloned()
        .collect::<Vec<_>>();
    candidates.sort_by_key(|request| {
        let claim_priority = match request.get("type").and_then(Value::as_str) {
            Some("hu") => 3_i32,
            Some("kong") | Some("pung") => 2,
            Some("chow") => 1,
            _ => 0,
        };
        let seat = request
            .get("seat")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(0);
        let mut distance = (seat + MAX_SEATS - discarder_seat) % MAX_SEATS;
        if distance == 0 {
            distance = MAX_SEATS;
        }
        (-claim_priority, distance as i32)
    });
    candidates.into_iter().next()
}

pub fn plan_claim_window_continuation_without_winner(
    state: &RoomState,
    discarder_seat: usize,
) -> Result<PlannedClaimWindowContinuation, String> {
    let next_actor = (discarder_seat + 1) % MAX_SEATS;
    let Some(drawn_tile) = peek_draw_for_turn(state) else {
        return Ok(PlannedClaimWindowContinuation {
            mutations: Vec::new(),
            needs_exhaustive_draw: true,
        });
    };
    let was_last_live_tile = live_tiles_remaining_after_head_draw(state) <= 1;

    Ok(PlannedClaimWindowContinuation {
        mutations: vec![
            LegacyRoomMutation::AdvanceWallHead,
            LegacyRoomMutation::PushPlayerConcealedTile {
                seat_index: next_actor,
                tile: drawn_tile.clone(),
            },
            LegacyRoomMutation::SetRoundPendingAction {
                pending_action: None,
            },
            LegacyRoomMutation::SetRoundCurrentActor {
                seat_index: next_actor,
            },
            LegacyRoomMutation::SetRoundLastActionContext {
                context: LastActionContext {
                    kind: "draw".to_string(),
                    seat: next_actor,
                    tile_id: Some(drawn_tile.tile_id.clone()),
                    from_kong_replacement: false,
                    was_last_live_tile,
                    was_last_discard: false,
                },
            },
            LegacyRoomMutation::IncrementRoundVersion,
        ],
        needs_exhaustive_draw: false,
    })
}

fn active_turn_drawn_tile_id(state: &RoomState, seat_index: usize) -> Option<String> {
    let round = state.round_state.as_ref()?;
    let action_kind = round.last_action_context.kind.as_str();
    if action_kind != "draw" && action_kind != "replacement_draw" {
        return None;
    }
    if round.last_action_context.seat != seat_index {
        return None;
    }
    let tile_id = round.last_action_context.tile_id.clone()?;
    let exists = round
        .players
        .get(seat_index)
        .map(|player| {
            player
                .concealed_tiles
                .iter()
                .any(|tile| tile.tile_id == tile_id)
        })
        .unwrap_or(false);
    if exists { Some(tile_id) } else { None }
}

fn player_first_flower_tile_id(state: &RoomState, seat_index: usize) -> Option<String> {
    state
        .round_state
        .as_ref()?
        .players
        .get(seat_index)?
        .concealed_tiles
        .iter()
        .find(|tile| tile.kind == "flower")
        .map(|tile| tile.tile_id.clone())
}

fn player_first_flower_tile_id_from_player(player: &PlayerRoundState) -> Option<String> {
    player
        .concealed_tiles
        .iter()
        .find(|tile| tile.kind == "flower")
        .map(|tile| tile.tile_id.clone())
}

fn player_has_concealed_flower(state: &RoomState, seat_index: usize) -> bool {
    state
        .round_state
        .as_ref()
        .and_then(|round| round.players.get(seat_index))
        .map(|player| {
            player
                .concealed_tiles
                .iter()
                .any(|tile| tile.kind == "flower")
        })
        .unwrap_or(false)
}

fn seat_can_beat_recorded_claim(
    seat_index: usize,
    claims: &[String],
    winning_claim: &Value,
    discarder_seat: usize,
) -> bool {
    claims.iter().any(|claim| {
        let candidate = json!({
            "seat": seat_index,
            "type": claim,
        });
        resolve_claims(&[winning_claim.clone(), candidate.clone()], discarder_seat)
            == Some(candidate)
    })
}

fn replacement_tile_from_tail(state: &RoomState) -> Option<Tile> {
    let round = state.round_state.as_ref()?;
    round.wall.tiles.get(round.wall.tail_index).cloned()
}

fn peek_draw_for_turn(state: &RoomState) -> Option<Tile> {
    let round = state.round_state.as_ref()?;
    round.wall.tiles.get(round.wall.head_index).cloned()
}

fn live_tiles_remaining_after_head_draw(state: &RoomState) -> usize {
    let round = match state.round_state.as_ref() {
        Some(round) => round,
        None => return 0,
    };
    round
        .wall
        .tail_index
        .checked_sub(round.wall.head_index + 1)
        .map(|distance| distance + 1)
        .unwrap_or(0)
}

fn offered_claim_seats(claim_window: &[Vec<String>]) -> Vec<usize> {
    claim_window
        .iter()
        .enumerate()
        .filter_map(|(seat_index, claims)| (!claims.is_empty()).then_some(seat_index))
        .collect()
}

fn any_concealed_flower(players: &[PlayerRoundState]) -> bool {
    players.iter().any(|player| {
        player
            .concealed_tiles
            .iter()
            .any(|tile| tile.kind == "flower")
    })
}

fn deadline_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

fn full_tile_set() -> Vec<Tile> {
    let mut tiles = Vec::new();
    for (suit_key, suit_name, prefix) in [
        ("characters", "Character", "w"),
        ("bamboos", "Bamboo", "t"),
        ("dots", "Dot", "b"),
    ] {
        for rank in 1..=9 {
            for copy_index in 0..4 {
                tiles.push(Tile {
                    tile_id: format!("{prefix}{rank}#{copy_index}"),
                    tile_key: format!("{prefix}{rank}"),
                    kind: "suit".to_string(),
                    suit: Some(suit_key.to_string()),
                    rank: Some(rank),
                    name: Some(format!("{suit_name} {rank}")),
                });
            }
        }
    }
    for (tile_key, name, kind) in [
        ("east", "East Wind", "wind"),
        ("south", "South Wind", "wind"),
        ("west", "West Wind", "wind"),
        ("north", "North Wind", "wind"),
        ("red", "Red Dragon", "dragon"),
        ("green", "Green Dragon", "dragon"),
        ("white", "White Dragon", "dragon"),
    ] {
        for copy_index in 0..4 {
            tiles.push(Tile {
                tile_id: format!("{tile_key}#{copy_index}"),
                tile_key: tile_key.to_string(),
                kind: kind.to_string(),
                suit: None,
                rank: None,
                name: Some(name.to_string()),
            });
        }
    }
    for (tile_key, name) in [
        ("f1", "Spring Flower"),
        ("f2", "Summer Flower"),
        ("f3", "Autumn Flower"),
        ("f4", "Winter Flower"),
        ("f5", "Plum Flower"),
        ("f6", "Orchid Flower"),
        ("f7", "Chrysanthemum Flower"),
        ("f8", "Bamboo Flower"),
    ] {
        tiles.push(Tile {
            tile_id: format!("{tile_key}#0"),
            tile_key: tile_key.to_string(),
            kind: "flower".to_string(),
            suit: None,
            rank: None,
            name: Some(name.to_string()),
        });
    }
    tiles
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::{
        compute_pending_timeout_value, plan_claim_window_continuation_without_winner,
        plan_claim_window_response, plan_flower_action, plan_round_start_payload,
    };
    use crate::core::state::RoomState;

    #[test]
    fn round_start_payload_builds_typed_round_state() {
        let (round, timeout) = plan_round_start_payload(2, "east", "round-1".to_string(), true, 7);

        assert_eq!(round.dealer_seat, 2);
        assert_eq!(round.current_actor, 2);
        assert_eq!(round.players.len(), 4);
        assert_eq!(round.wall.head_index, 53);
        assert_eq!(round.wall.tail_index, 143);
        assert_eq!(round.players[2].concealed_tiles.len(), 14);
        assert_eq!(round.players[0].concealed_tiles.len(), 13);
        assert!(timeout.deadline_at.is_some());

        match round.pending_action {
            Some(crate::core::state::PendingAction::OpeningFlowers(_)) => {
                assert_eq!(timeout.kind, "opening_flowers");
                assert_eq!(
                    timeout.drawn_tile_id,
                    super::player_first_flower_tile_id_from_player(
                        &round.players[round.current_actor]
                    )
                );
            }
            None => {
                assert_eq!(timeout.kind, "active_turn");
                assert_eq!(round.last_action_context.tile_id, timeout.drawn_tile_id);
            }
            Some(other) => panic!("unexpected pending action at round start: {other:?}"),
        }
    }

    #[test]
    fn compute_pending_timeout_uses_typed_room_state() {
        let (mut round, _timeout) =
            plan_round_start_payload(0, "east", "round-2".to_string(), true, 11);
        round.pending_action = None;
        round.score_trackers.opening_flowers_completed = true;

        let state = RoomState {
            table_code: "ROOM42".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            test_mode: false,
            enforce_minimum_eight_fan: true,
            seats: Vec::new(),
            match_state: Some(crate::core::state::MatchState {
                prevailing_wind: "east".to_string(),
                hand_number: 1,
                dealer_seat: 0,
                cumulative_scores: BTreeMap::new(),
                match_finished: false,
                last_completed_round_id: None,
                skill_trackers: Default::default(),
            }),
            round_state: Some(round.clone()),
            pending_timeout: None,
            continue_action: None,
        };

        let timeout = compute_pending_timeout_value(&state, "2026-04-07T00:00:00Z".to_string())
            .expect("playing room should produce a timeout");

        assert_eq!(timeout.kind, "active_turn");
        assert_eq!(timeout.seat_index, round.current_actor);
        assert_eq!(timeout.drawn_tile_id, round.last_action_context.tile_id);
        assert_eq!(timeout.deadline_at.as_deref(), Some("2026-04-07T00:00:00Z"));
    }

    #[test]
    fn flower_action_plan_uses_typed_room_state() {
        let room = RoomState::from_legacy_value(&json!({
            "table_code": "ROOM42",
            "phase": "playing",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "seats": [],
            "match_state": null,
            "round_state": {
                "round_id": "round-3",
                "dealer_seat": 0,
                "current_actor": 0,
                "wall": {
                    "tiles": [
                        {
                            "tile_id": "w1#0",
                            "tile_key": "w1",
                            "kind": "suit",
                            "suit": "characters",
                            "rank": 1,
                            "name": "Character 1"
                        },
                        {
                            "tile_id": "f1#0",
                            "tile_key": "f1",
                            "kind": "flower",
                            "suit": null,
                            "rank": null,
                            "name": "Spring Flower"
                        }
                    ],
                    "head_index": 0,
                    "tail_index": 1
                },
                "players": [{
                    "seat": 0,
                    "concealed_tiles": [{
                        "tile_id": "f1#hand",
                        "tile_key": "f1",
                        "kind": "flower",
                        "suit": null,
                        "rank": null,
                        "name": "Spring Flower"
                    }],
                    "melds": [],
                    "flowers": [],
                    "discards": []
                }],
                "last_discard": null,
                "pending_action": {
                    "type": "opening_flowers",
                    "dealer_seat": 0
                },
                "phase": "playing",
                "settlement": null,
                "version": 1,
                "score_trackers": {
                    "kong_entries": [],
                    "opening_flowers_completed": false
                },
                "last_action_context": {
                    "kind": "draw",
                    "seat": 0,
                    "tile_id": "f1#hand",
                    "from_kong_replacement": false,
                    "was_last_live_tile": false,
                    "was_last_discard": false
                },
                "round_wind": "east",
                "enforce_minimum_eight_fan": true,
                "restricted_discard_tile_key": null
            },
            "pending_timeout": null,
            "start_next_round_confirmed_seats": [],
            "restart_match_confirmed_seats": [],
            "continue_action_auto_advance_deadline_at": null
        }))
        .expect("legacy room should parse");

        let plan = plan_flower_action(&room, 0, "f1#hand").expect("flower action should plan");

        assert_eq!(plan.flower_tile.tile_id, "f1#hand");
        assert_eq!(plan.replacement_tile.tile_id, "f1#0");
        assert!(plan.mutations.len() >= 6);
    }

    #[test]
    fn claim_window_response_plan_uses_typed_pending_action() {
        let room = RoomState::from_legacy_value(&json!({
            "table_code": "ROOM42",
            "phase": "playing",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "seats": [],
            "match_state": null,
            "round_state": {
                "round_id": "round-4",
                "dealer_seat": 0,
                "current_actor": 0,
                "wall": {
                    "tiles": [],
                    "head_index": 0,
                    "tail_index": 0
                },
                "players": [],
                "last_discard": {
                    "tile_id": "w3#discard",
                    "tile_key": "w3",
                    "kind": "suit",
                    "suit": "characters",
                    "rank": 3,
                    "name": "Character 3"
                },
                "pending_action": {
                    "type": "claim_window",
                    "discarder_seat": 0,
                    "claim_window": [
                        [],
                        ["pung"],
                        ["chow"],
                        []
                    ],
                    "responded_seats": [],
                    "claim_responses": []
                },
                "phase": "playing",
                "settlement": null,
                "version": 1,
                "score_trackers": {
                    "kong_entries": [],
                    "opening_flowers_completed": true
                },
                "last_action_context": {
                    "kind": "discard",
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
            "pending_timeout": null,
            "start_next_round_confirmed_seats": [],
            "restart_match_confirmed_seats": [],
            "continue_action_auto_advance_deadline_at": null
        }))
        .expect("legacy room should parse");

        let plan =
            plan_claim_window_response(&room, 1, "pung", &["w3#a".to_string(), "w3#b".to_string()])
                .expect("claim window response should plan");

        assert!(plan.unresolved_seats.is_empty());
        assert_eq!(plan.mutations.len(), 2);
        match &plan.mutations[0] {
            crate::core::engine::reducer::LegacyRoomMutation::SetRoundPendingAction {
                pending_action,
            } => {
                let Some(crate::core::state::PendingAction::ClaimWindow(pending_action)) =
                    pending_action.as_ref()
                else {
                    panic!("expected claim window pending action");
                };
                assert_eq!(pending_action.responded_seats, vec![1, 2]);
                assert_eq!(pending_action.claim_responses[0]["type"], json!("pung"));
            }
            other => panic!("unexpected first mutation: {other:?}"),
        }
    }

    #[test]
    fn claim_window_continuation_without_winner_draws_next_actor() {
        let room = RoomState::from_legacy_value(&json!({
            "table_code": "ROOM42",
            "phase": "playing",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "seats": [],
            "match_state": null,
            "round_state": {
                "round_id": "round-5",
                "dealer_seat": 0,
                "current_actor": 0,
                "wall": {
                    "tiles": [{
                        "tile_id": "w1#0",
                        "tile_key": "w1",
                        "kind": "suit",
                        "suit": "characters",
                        "rank": 1,
                        "name": "Character 1"
                    }],
                    "head_index": 0,
                    "tail_index": 0
                },
                "players": [],
                "last_discard": {
                    "tile_id": "w3#discard",
                    "tile_key": "w3",
                    "kind": "suit",
                    "suit": "characters",
                    "rank": 3,
                    "name": "Character 3"
                },
                "pending_action": {
                    "type": "claim_window",
                    "discarder_seat": 0,
                    "claim_window": [[], [], [], []],
                    "responded_seats": [1, 2, 3],
                    "claim_responses": []
                },
                "phase": "playing",
                "settlement": null,
                "version": 1,
                "score_trackers": {
                    "kong_entries": [],
                    "opening_flowers_completed": true
                },
                "last_action_context": {
                    "kind": "discard",
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
            "pending_timeout": null,
            "start_next_round_confirmed_seats": [],
            "restart_match_confirmed_seats": [],
            "continue_action_auto_advance_deadline_at": null
        }))
        .expect("legacy room should parse");

        let plan = plan_claim_window_continuation_without_winner(&room, 0)
            .expect("continuation should plan");

        assert!(!plan.needs_exhaustive_draw);
        assert_eq!(plan.mutations.len(), 6);
        match &plan.mutations[1] {
            crate::core::engine::reducer::LegacyRoomMutation::PushPlayerConcealedTile {
                seat_index,
                tile,
            } => {
                assert_eq!(*seat_index, 1);
                assert_eq!(tile.tile_id, "w1#0");
            }
            other => panic!("unexpected mutation: {other:?}"),
        }
    }
}
