use rand::SeedableRng;
use rand::seq::SliceRandom;
use serde_json::{Value, json};

use crate::core::state::{
    ClaimResponse, ClaimWindowAction, LastActionContext, PendingAction, PendingTimeout,
    PlayerRoundState, RobKongWindowAction, RoomState, RoundScoreTrackers, RoundSettlement,
    RoundState, RuleRuntimeState, WallState,
};
use crate::core::tile::Tile;

const MAX_SEATS: usize = 4;
const PENDING_TIMEOUT_SECONDS: i64 = 15;

#[derive(Debug, Clone)]
pub struct PlannedFlowerAction {
    pub flower_tile: Tile,
    pub replacement_tile: Tile,
    pub last_action_context: LastActionContext,
}

#[derive(Debug, Clone)]
pub struct PlannedClaimWindowResponse {
    pub pending_action: PendingAction,
    pub unresolved_seats: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct PlannedDiscardAction {
    pub discarded_tile: Tile,
    pub continuation: PlannedDiscardContinuation,
}

#[derive(Debug, Clone)]
pub struct PlannedDiscardContinuation {
    pub current_actor: usize,
    pub pending_action: Option<PendingAction>,
    pub drawn_tile: Option<Tile>,
    pub last_action_context: LastActionContext,
    pub needs_exhaustive_draw: bool,
}

#[derive(Debug, Clone)]
pub struct PlannedClaimWindowContinuation {
    pub outcome: PlannedClaimWindowOutcome,
}

#[derive(Debug, Clone)]
pub struct PlannedSettlementToMatch {
    pub cumulative_scores: std::collections::BTreeMap<usize, i64>,
    pub round_id: String,
}

#[derive(Debug, Clone)]
pub enum PlannedClaimWindowOutcome {
    ExhaustiveDraw,
    AdvanceTurn {
        current_actor: usize,
        drawn_tile: Tile,
        last_action_context: LastActionContext,
    },
}

impl PlannedDiscardAction {}

impl PlannedClaimWindowContinuation {
    pub fn needs_exhaustive_draw(&self) -> bool {
        matches!(self.outcome, PlannedClaimWindowOutcome::ExhaustiveDraw)
    }
}

pub fn plan_round_start_payload(
    dealer_seat: usize,
    round_wind: &str,
    round_id: String,
    _seed: u64,
) -> (RoundState, PendingTimeout) {
    let mut wall_tiles = full_tile_set();
    // 使用操作系统级别的最高熵真随机种子初始化随机数生成器。
    // 传入的 _seed: u64 仍保留在函数签名中以维持 API 兼容性并用于 round_id 生成，
    // 但洗牌时改用从操作系统直接获取随机性，以确保最大的随机安全性。
    let mut rng = rand::rngs::StdRng::from_os_rng();
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
            is_ready_hand: false,
            concealed_tiles,
            melds: Vec::new(),
            display_melds: Vec::new(),
            flowers: Vec::new(),
            discards: Vec::new(),
        });
    }

    let current_actor = dealer_seat;

    let draw_tile = wall_tiles[head_index].clone();
    head_index += 1;
    players[current_actor]
        .concealed_tiles
        .push(draw_tile.clone());
    let (tail_index, last_action_context) = resolve_starting_flowers(
        &mut players,
        &wall_tiles,
        143,
        dealer_seat,
        LastActionContext {
            kind: "draw".to_string(),
            seat: current_actor,
            tile_id: Some(draw_tile.tile_id.clone()),
            from_kong_replacement: false,
            was_last_live_tile: false,
            was_last_discard: false,
        },
    );

    let round_state = RoundState {
        round_id,
        dealer_seat,
        round_wind: round_wind.to_string(),
        current_actor,
        phase: "playing".to_string(),
        wall: WallState {
            tiles: wall_tiles,
            head_index,
            tail_index,
        },
        players,
        discard_history: Vec::new(),
        last_discard: None,
        pending_action: None,
        settlement: None,
        version: 1,
        score_trackers: RoundScoreTrackers {
            kong_entries: Vec::new(),
        },
        last_action_context,
        rule_state: RuleRuntimeState {},
        restricted_discard_tile_key: None,
    };

    let pending_timeout = PendingTimeout {
        kind: "active_turn".to_string(),
        seat_index: current_actor,
        deadline_at: Some(deadline_iso()),
        drawn_tile_id: round_state.last_action_context.tile_id.clone(),
        extended_with_extra: false,
    };

    (round_state, pending_timeout)
}

fn resolve_starting_flowers(
    players: &mut [PlayerRoundState],
    wall_tiles: &[Tile],
    initial_tail_index: usize,
    dealer_seat: usize,
    initial_dealer_context: LastActionContext,
) -> (usize, LastActionContext) {
    let mut tail_index = initial_tail_index;
    let mut dealer_context = initial_dealer_context;

    for seat_offset in 0..MAX_SEATS {
        let seat_index = (dealer_seat + seat_offset) % MAX_SEATS;

        loop {
            let Some(player) = players.get_mut(seat_index) else {
                break;
            };
            let Some(flower_index) = player
                .concealed_tiles
                .iter()
                .position(|tile| tile.kind == "flower")
            else {
                break;
            };
            let Some(replacement_tile) = wall_tiles.get(tail_index).cloned() else {
                break;
            };

            let flower_tile = player.concealed_tiles.remove(flower_index);
            player.flowers.push(flower_tile);
            player.concealed_tiles.push(replacement_tile.clone());
            tail_index = tail_index.saturating_sub(1);

            if seat_index == dealer_seat {
                dealer_context = LastActionContext {
                    kind: "replacement_draw".to_string(),
                    seat: seat_index,
                    tile_id: Some(replacement_tile.tile_id),
                    from_kong_replacement: false,
                    was_last_live_tile: false,
                    was_last_discard: false,
                };
            }
        }
    }

    (tail_index, dealer_context)
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
        Some(PendingAction::ClaimWindow(claim)) => Some(PendingTimeout {
            kind: "claim_window".to_string(),
            seat_index: claim.discarder_seat,
            deadline_at: Some(deadline_at),
            drawn_tile_id: None,
            extended_with_extra: false,
        }),
        Some(PendingAction::RobKongWindow(rob)) => Some(PendingTimeout {
            kind: "claim_window".to_string(),
            seat_index: rob.actor_seat,
            deadline_at: Some(deadline_at),
            drawn_tile_id: None,
            extended_with_extra: false,
        }),
        _ => Some(PendingTimeout {
            kind: "active_turn".to_string(),
            seat_index: round.current_actor,
            deadline_at: Some(deadline_at),
            drawn_tile_id: active_turn_drawn_tile_id(state, round.current_actor),
            extended_with_extra: false,
        }),
    }
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

    let last_action_context = LastActionContext {
        kind: "replacement_draw".to_string(),
        seat: seat_index,
        tile_id: Some(replacement_tile.tile_id.clone()),
        from_kong_replacement: false,
        was_last_live_tile: false,
        was_last_discard: false,
    };

    Ok(PlannedFlowerAction {
        flower_tile,
        replacement_tile,
        last_action_context,
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
        claim_responses.push(ClaimResponse {
            seat: seat_index,
            action_type: action_type.to_string(),
            tiles: tile_ids.to_vec(),
        });
        let winning_hu_claims = resolve_hu_claims(&claim_responses, discarder_seat);
        if !winning_hu_claims.is_empty() {
            for (other_seat, claims) in claim.claim_window.iter().enumerate() {
                if claims.is_empty() || responded_seats.contains(&other_seat) {
                    continue;
                }
                if !claims.iter().any(|claim_type| claim_type == "hu") {
                    responded_seats.push(other_seat);
                }
            }
        } else if let Some(winning_claim) = resolve_claims(&claim_responses, discarder_seat) {
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
        pending_action: PendingAction::ClaimWindow(ClaimWindowAction {
            discarder_seat,
            claim_window: claim.claim_window.clone(),
            responded_seats,
            claim_responses,
        }),
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
        live_tiles_remaining_after_head_draw(state) == 0
    };
    let pending_action = if has_claim {
        Some(PendingAction::ClaimWindow(ClaimWindowAction {
            discarder_seat: seat_index,
            claim_window,
            responded_seats: vec![],
            claim_responses: vec![],
        }))
    } else {
        None
    };
    let last_action_context = if has_claim {
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
    };

    Ok(PlannedDiscardAction {
        discarded_tile,
        continuation: PlannedDiscardContinuation {
            current_actor: if has_claim { seat_index } else { next_actor },
            pending_action,
            drawn_tile,
            last_action_context,
            needs_exhaustive_draw,
        },
    })
}

pub fn plan_settlement_to_match(
    state: &RoomState,
    settlement: &RoundSettlement,
) -> Option<PlannedSettlementToMatch> {
    let Some(round) = state.round_state.as_ref() else {
        return None;
    };
    let Some(match_state) = state.match_state.as_ref() else {
        return None;
    };
    if match_state.last_completed_round_id.as_deref() == Some(round.round_id.as_str()) {
        return None;
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

    Some(PlannedSettlementToMatch {
        cumulative_scores,
        round_id: round.round_id.clone(),
    })
}

pub fn resolve_claims(
    claim_requests: &[ClaimResponse],
    discarder_seat: usize,
) -> Option<ClaimResponse> {
    let next_player = (discarder_seat + 1) % MAX_SEATS;
    let mut candidates = claim_requests
        .iter()
        .filter(|request| {
            let claim_type = request.action_type.as_str();
            if !matches!(claim_type, "chow" | "pung" | "kong" | "hu") {
                return false;
            }
            if claim_type == "chow" && request.seat != next_player {
                return false;
            }
            true
        })
        .cloned()
        .collect::<Vec<_>>();
    candidates.sort_by_key(|request| {
        let claim_priority = match request.action_type.as_str() {
            "hu" => 3_i32,
            "kong" | "pung" => 2,
            "chow" => 1,
            _ => 0,
        };
        let mut distance = (request.seat + MAX_SEATS - discarder_seat) % MAX_SEATS;
        if distance == 0 {
            distance = MAX_SEATS;
        }
        (-claim_priority, distance as i32)
    });
    candidates.into_iter().next()
}

pub fn resolve_hu_claims(
    claim_requests: &[ClaimResponse],
    discarder_seat: usize,
) -> Vec<ClaimResponse> {
    let mut candidates = claim_requests
        .iter()
        .filter(|request| request.action_type == "hu")
        .cloned()
        .collect::<Vec<_>>();
    candidates.sort_by_key(|request| {
        let mut distance = (request.seat + MAX_SEATS - discarder_seat) % MAX_SEATS;
        if distance == 0 {
            distance = MAX_SEATS;
        }
        distance
    });
    candidates
}

pub fn plan_claim_window_continuation_without_winner(
    state: &RoomState,
    discarder_seat: usize,
) -> Result<PlannedClaimWindowContinuation, String> {
    let next_actor = (discarder_seat + 1) % MAX_SEATS;
    let Some(drawn_tile) = peek_draw_for_turn(state) else {
        return Ok(PlannedClaimWindowContinuation {
            outcome: PlannedClaimWindowOutcome::ExhaustiveDraw,
        });
    };
    let was_last_live_tile = live_tiles_remaining_after_head_draw(state) == 0;

    Ok(PlannedClaimWindowContinuation {
        outcome: PlannedClaimWindowOutcome::AdvanceTurn {
            current_actor: next_actor,
            drawn_tile: drawn_tile.clone(),
            last_action_context: LastActionContext {
                kind: "draw".to_string(),
                seat: next_actor,
                tile_id: Some(drawn_tile.tile_id.clone()),
                from_kong_replacement: false,
                was_last_live_tile,
                was_last_discard: false,
            },
        },
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

fn seat_can_beat_recorded_claim(
    seat_index: usize,
    claims: &[String],
    winning_claim: &ClaimResponse,
    discarder_seat: usize,
) -> bool {
    claims.iter().any(|claim| {
        let candidate = ClaimResponse {
            seat: seat_index,
            action_type: claim.clone(),
            tiles: Vec::new(),
        };
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
    if round.wall.head_index > round.wall.tail_index {
        return None;
    }
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
    (chrono::Utc::now() + chrono::TimeDelta::seconds(PENDING_TIMEOUT_SECONDS))
        .to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
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

    use chrono::{TimeDelta, Utc};
    use serde_json::json;

    use super::{
        compute_pending_timeout_value, plan_claim_window_continuation_without_winner,
        plan_claim_window_response, plan_discard_action, plan_flower_action,
        plan_round_start_payload,
    };
    use crate::core::state::RoomState;

    #[test]
    fn round_start_payload_starts_directly_in_active_turn() {
        for seed in 0..64 {
            let (round, timeout) =
                plan_round_start_payload(2, "east", format!("round-{seed}"), seed);

            assert_eq!(round.dealer_seat, 2);
            assert_eq!(round.current_actor, 2);
            assert_eq!(round.players.len(), 4);
            assert_eq!(round.wall.head_index, 53);
            assert_eq!(round.players[2].concealed_tiles.len(), 14, "seed {seed}");
            assert_eq!(round.players[0].concealed_tiles.len(), 13, "seed {seed}");
            assert!(timeout.deadline_at.is_some());
            assert!(round.pending_action.is_none(), "seed {seed}");
            assert_eq!(timeout.kind, "active_turn", "seed {seed}");
            assert_eq!(
                round.last_action_context.tile_id, timeout.drawn_tile_id,
                "seed {seed}"
            );
        }
    }

    #[test]
    fn round_start_payload_sets_future_timeout_deadline() {
        let (_round, timeout) = plan_round_start_payload(2, "east", "round-future".to_string(), 13);

        let deadline = timeout
            .deadline_at
            .as_deref()
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&Utc))
            .expect("round start should set an RFC3339 timeout");

        assert!(deadline > Utc::now());
        assert!(deadline <= Utc::now() + TimeDelta::seconds(super::PENDING_TIMEOUT_SECONDS + 1));
    }

    #[test]
    fn compute_pending_timeout_uses_typed_room_state() {
        let (mut round, _timeout) = plan_round_start_payload(0, "east", "round-2".to_string(), 11);
        round.pending_action = None;
        let state = RoomState {
            table_code: "ROOM42".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            owner_user_id: None,
            multiplier: 1,
            minimum_hu_fan: crate::core::state::room::default_minimum_hu_fan(),
            seats: Vec::new(),
            match_state: Some(crate::core::state::MatchState {
                prevailing_wind: "east".to_string(),
                hand_number: 1,
                dealer_seat: 0,
                cumulative_scores: BTreeMap::new(),
                match_finished: false,
                last_completed_round_id: None,
                statistics: Default::default(),
                extra_time_pool: Default::default(),
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
        let room = RoomState::from_room_value(&json!({
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
                "pending_action": null,
                "phase": "playing",
                "settlement": null,
                "version": 1,
                "score_trackers": {
                    "kong_entries": []
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
            "continue_action": null
        }))
        .expect("room should parse");

        let plan = plan_flower_action(&room, 0, "f1#hand").expect("flower action should plan");

        assert_eq!(plan.flower_tile.tile_id, "f1#hand");
        assert_eq!(plan.replacement_tile.tile_id, "f1#0");
        assert_eq!(plan.last_action_context.kind, "replacement_draw");
    }

    #[test]
    fn claim_window_response_plan_uses_typed_pending_action() {
        let room = RoomState::from_room_value(&json!({
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
                    "kong_entries": []
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
            "continue_action": null
        }))
        .expect("room should parse");

        let plan =
            plan_claim_window_response(&room, 1, "pung", &["w3#a".to_string(), "w3#b".to_string()])
                .expect("claim window response should plan");

        assert!(plan.unresolved_seats.is_empty());
        let crate::core::state::PendingAction::ClaimWindow(pending_action) = &plan.pending_action
        else {
            panic!("expected claim window pending action");
        };
        assert_eq!(pending_action.responded_seats, vec![1, 2]);
        assert_eq!(pending_action.claim_responses[0].action_type, "pung");
    }

    #[test]
    fn claim_window_continuation_without_winner_draws_next_actor() {
        let room = RoomState::from_room_value(&json!({
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
                    "kong_entries": []
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
            "continue_action": null
        }))
        .expect("room should parse");

        let plan = plan_claim_window_continuation_without_winner(&room, 0)
            .expect("continuation should plan");

        assert!(!plan.needs_exhaustive_draw());
        let super::PlannedClaimWindowOutcome::AdvanceTurn {
            current_actor,
            drawn_tile,
            last_action_context,
        } = &plan.outcome
        else {
            panic!("expected advance turn outcome");
        };
        assert_eq!(*current_actor, 1);
        assert_eq!(drawn_tile.tile_id, "w1#0");
        assert_eq!(last_action_context.kind, "draw");
    }

    #[test]
    fn claim_window_continuation_marks_last_live_tile_only_for_final_draw() {
        let room = RoomState::from_room_value(&json!({
            "table_code": "ROOM42",
            "phase": "playing",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "seats": [],
            "match_state": null,
            "round_state": {
                "round_id": "round-5b",
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
                    }, {
                        "tile_id": "w2#0",
                        "tile_key": "w2",
                        "kind": "suit",
                        "suit": "characters",
                        "rank": 2,
                        "name": "Character 2"
                    }],
                    "head_index": 0,
                    "tail_index": 1
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
                    "kong_entries": []
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
            "continue_action": null
        }))
        .expect("room should parse");

        let plan = plan_claim_window_continuation_without_winner(&room, 0)
            .expect("continuation should plan");

        let super::PlannedClaimWindowOutcome::AdvanceTurn {
            last_action_context,
            ..
        } = &plan.outcome
        else {
            panic!("expected advance turn outcome");
        };
        assert!(!last_action_context.was_last_live_tile);
    }

    #[test]
    fn discard_plan_exposes_typed_continuation_before_mutation_adaptation() {
        let room = RoomState::from_room_value(&json!({
            "table_code": "ROOM42",
            "phase": "playing",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "seats": [],
            "match_state": null,
            "round_state": {
                "round_id": "round-discard",
                "dealer_seat": 0,
                "current_actor": 0,
                "wall": {
                    "tiles": [{
                        "tile_id": "w4#draw",
                        "tile_key": "w4",
                        "kind": "suit",
                        "suit": "characters",
                        "rank": 4,
                        "name": "Character 4"
                    }],
                    "head_index": 0,
                    "tail_index": 0
                },
                "players": [{
                    "seat": 0,
                    "concealed_tiles": [{
                        "tile_id": "w3#discard",
                        "tile_key": "w3",
                        "kind": "suit",
                        "suit": "characters",
                        "rank": 3,
                        "name": "Character 3"
                    }],
                    "melds": [],
                    "flowers": [],
                    "discards": []
                }, {
                    "seat": 1,
                    "concealed_tiles": [],
                    "melds": [],
                    "flowers": [],
                    "discards": []
                }, {
                    "seat": 2,
                    "concealed_tiles": [],
                    "melds": [],
                    "flowers": [],
                    "discards": []
                }, {
                    "seat": 3,
                    "concealed_tiles": [],
                    "melds": [],
                    "flowers": [],
                    "discards": []
                }],
                "last_discard": null,
                "pending_action": null,
                "phase": "playing",
                "settlement": null,
                "version": 1,
                "score_trackers": {
                    "kong_entries": []
                },
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
            "pending_timeout": null,
            "continue_action": null
        }))
        .expect("room should parse");

        let plan = plan_discard_action(
            &room,
            0,
            "w3#discard",
            vec![vec![], vec![], vec![], vec![]],
            false,
        )
        .expect("discard action should plan");

        assert_eq!(plan.discarded_tile.tile_id, "w3#discard");
        assert_eq!(plan.continuation.current_actor, 1);
        assert!(plan.continuation.pending_action.is_none());
        assert_eq!(
            plan.continuation
                .drawn_tile
                .as_ref()
                .map(|tile| tile.tile_id.as_str()),
            Some("w4#draw")
        );
        assert_eq!(plan.continuation.last_action_context.kind, "draw");
        assert!(!plan.continuation.needs_exhaustive_draw);
    }

    #[test]
    fn discard_plan_does_not_mark_second_to_last_draw_as_last_live_tile() {
        let room = RoomState::from_room_value(&json!({
            "table_code": "ROOM42",
            "phase": "playing",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "seats": [],
            "match_state": null,
            "round_state": {
                "round_id": "round-discard-boundary",
                "dealer_seat": 0,
                "current_actor": 0,
                "wall": {
                    "tiles": [{
                        "tile_id": "w4#draw",
                        "tile_key": "w4",
                        "kind": "suit",
                        "suit": "characters",
                        "rank": 4,
                        "name": "Character 4"
                    }, {
                        "tile_id": "w5#later",
                        "tile_key": "w5",
                        "kind": "suit",
                        "suit": "characters",
                        "rank": 5,
                        "name": "Character 5"
                    }],
                    "head_index": 0,
                    "tail_index": 1
                },
                "players": [{
                    "seat": 0,
                    "concealed_tiles": [{
                        "tile_id": "w3#discard",
                        "tile_key": "w3",
                        "kind": "suit",
                        "suit": "characters",
                        "rank": 3,
                        "name": "Character 3"
                    }],
                    "melds": [],
                    "flowers": [],
                    "discards": []
                }, {
                    "seat": 1,
                    "concealed_tiles": [],
                    "melds": [],
                    "flowers": [],
                    "discards": []
                }, {
                    "seat": 2,
                    "concealed_tiles": [],
                    "melds": [],
                    "flowers": [],
                    "discards": []
                }, {
                    "seat": 3,
                    "concealed_tiles": [],
                    "melds": [],
                    "flowers": [],
                    "discards": []
                }],
                "last_discard": null,
                "pending_action": null,
                "phase": "playing",
                "settlement": null,
                "version": 1,
                "score_trackers": {
                    "kong_entries": []
                },
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
            "pending_timeout": null,
            "continue_action": null
        }))
        .expect("room should parse");

        let plan = plan_discard_action(
            &room,
            0,
            "w3#discard",
            vec![vec![], vec![], vec![], vec![]],
            false,
        )
        .expect("discard action should plan");

        assert!(!plan.continuation.last_action_context.was_last_live_tile);
    }
}
