use serde_json::{Value, json};

use crate::core::engine::planner::plan_settlement_to_match;
use crate::core::engine::reducer::{LegacyRoomMutation, apply_legacy_room_mutations};
use crate::core::state::PendingAction;
use crate::room_scoring::RoomScoringCache;
use crate::scoring::{
    Decomposition as ScoringDecomposition, EvaluationInput as ScoringEvaluationInput,
    KongEntry as ScoringKongEntry, TimingFeatures as ScoringTimingFeatures,
    decompose_winning_hand_with_melds as scoring_decompose_winning_hand_with_melds,
    evaluate_fans as scoring_evaluate_fans, extract_hand_features as scoring_extract_hand_features,
};

use super::runtime::{current_actor, project_room_state, round_event_message};

const MAX_SEATS: usize = 4;
const WIND_ORDER: [&str; 4] = ["east", "south", "west", "north"];

struct PreparedWinEvaluation {
    concealed_tile_keys: Vec<String>,
    meld_tile_key_groups: Vec<Vec<String>>,
    open_meld_tile_key_groups: Vec<Vec<String>>,
    meld_open_flags: Vec<bool>,
    decompositions: Vec<ScoringDecomposition>,
    kong_entries: Vec<ScoringKongEntry>,
}

pub fn compute_hu_settlement(
    room: &Value,
    winner_seat: usize,
    hu_context: &str,
) -> Result<Value, String> {
    let state = project_room_state(room)?;
    if state.phase != "playing" {
        return Err("round_not_ready".to_string());
    }
    let round = state
        .round_state
        .as_ref()
        .ok_or_else(|| "round_not_ready".to_string())?;

    let discarder_seat = if hu_context == "self_draw" {
        if round.current_actor != winner_seat {
            return Err("invalid_action".to_string());
        }
        None
    } else {
        match round.pending_action.as_ref() {
            Some(PendingAction::ClaimWindow(claim)) => {
                if !claim
                    .claim_window
                    .get(winner_seat)
                    .is_some_and(|claims| claims.iter().any(|claim_type| claim_type == "hu"))
                {
                    return Err("invalid_action".to_string());
                }
                Some(claim.discarder_seat)
            }
            Some(PendingAction::RobKongWindow(rob)) => {
                if !rob.offered_hu_seats.contains(&winner_seat) {
                    return Err("invalid_action".to_string());
                }
                Some(rob.actor_seat)
            }
            _ => return Err("invalid_action".to_string()),
        }
    };

    let incoming_tile = if hu_context == "self_draw" {
        None
    } else {
        round
            .last_discard
            .as_ref()
            .map(|tile| tile.tile_key.as_str())
    };

    let fan_result = fan_result_for_win(room, winner_seat, incoming_tile, discarder_seat)?;
    let flower_count = round
        .players
        .get(winner_seat)
        .map(|player| player.flowers.len())
        .unwrap_or(0);
    let enforce_minimum_eight_fan = round.rule_state.enforce_minimum_eight_fan;

    Ok(json!({
        "provisional": true,
        "win_type": hu_context,
        "winner_seat": winner_seat,
        "discarder_seat": discarder_seat,
        "display_win_label": if !enforce_minimum_eight_fan && fan_result.fan_total < 8 { Value::String("灞佸拰".to_string()) } else { Value::Null },
        "fan_total": fan_result.fan_total,
        "fan_keys": fan_result.fan_keys,
        "fan_breakdown": Value::Array(
            fan_result
                .fan_breakdown
                .iter()
                .map(|entry| json!({ "fan_key": entry.fan_key, "fan_value": entry.fan_value }))
                .collect()
        ),
        "score_delta": fan_result.score_delta_json(),
        "flower_count": flower_count,
        "kong_score_detail": fan_result.kong_score_detail_json(),
    }))
}

pub fn apply_hu_settlement(
    room: &mut Value,
    winner_seat: usize,
    hu_context: &str,
    settlement: Value,
) -> Result<Vec<Value>, String> {
    let round_id = room
        .get("round_state")
        .and_then(|round| round.get("round_id"))
        .cloned()
        .unwrap_or(Value::Null);
    let winning_tile_id = room
        .get("round_state")
        .and_then(|round| round.get("last_action_context"))
        .and_then(|context| context.get("tile_id"))
        .cloned()
        .unwrap_or(Value::Null);
    let discarded_tile = room
        .get("round_state")
        .and_then(|round| round.get("last_discard"))
        .cloned()
        .unwrap_or(Value::Null);
    let mut mutations = vec![
        LegacyRoomMutation::SetRoomField {
            key: "phase".to_string(),
            value: Value::String("settlement".to_string()),
        },
        LegacyRoomMutation::SetRoomField {
            key: "pending_timeout".to_string(),
            value: Value::Null,
        },
        LegacyRoomMutation::SetRoundField {
            key: "phase".to_string(),
            value: Value::String("settlement".to_string()),
        },
        LegacyRoomMutation::SetRoundPendingAction {
            pending_action: Value::Null,
        },
        LegacyRoomMutation::SetRoundField {
            key: "settlement".to_string(),
            value: settlement.clone(),
        },
        LegacyRoomMutation::SetRoundCurrentActor {
            seat_index: winner_seat,
        },
        LegacyRoomMutation::IncrementRoundVersion,
    ];
    if let Ok(state) = project_room_state(room) {
        mutations.extend(plan_settlement_to_match(&state, &settlement));
    }
    apply_legacy_room_mutations(room, &mutations)?;

    let first_event = if hu_context == "self_draw" {
        round_event_message(
            "self_hu_declared",
            json!({
                "type": "self_hu_declared",
                "seat": winner_seat,
                "tile_id": winning_tile_id,
            }),
        )
    } else {
        round_event_message(
            "claim_made",
            json!({
                "type": "claim_made",
                "seat": winner_seat,
                "claim_type": "hu",
                "tile_id": discarded_tile.get("tile_id").cloned().unwrap_or(Value::Null),
            }),
        )
    };
    Ok(vec![
        first_event,
        round_event_message(
            "settlement_ready",
            json!({
                "type": "settlement_ready",
                "round_id": round_id,
            }),
        ),
    ])
}

pub fn hu_action_hint(room: &Value, seat_index: usize) -> Option<&'static str> {
    if room.get("phase").and_then(Value::as_str) != Some("playing") {
        return None;
    }
    let pending_action = room.get("round_state")?.get("pending_action");
    if pending_action.is_none() || pending_action.is_some_and(Value::is_null) {
        return (current_actor(room) == Some(seat_index)).then_some("self_draw");
    }
    let pending_action = pending_action?;
    match pending_action.get("type").and_then(Value::as_str) {
        Some("claim_window") if claim_window_offers_claim(pending_action, seat_index, "hu") => {
            Some("discard")
        }
        Some("rob_kong_window") if rob_kong_window_offers_seat(pending_action, seat_index) => {
            Some("discard")
        }
        Some("claim_window") | Some("rob_kong_window") => None,
        _ => None,
    }
}

pub fn claim_window_offers_claim(
    pending_action: &Value,
    seat_index: usize,
    claim_type: &str,
) -> bool {
    json_array_contains_str(
        pending_action
            .get("claim_window")
            .and_then(Value::as_array)
            .and_then(|claim_window| claim_window.get(seat_index))
            .and_then(Value::as_array),
        claim_type,
    )
}

pub fn rob_kong_window_offers_seat(pending_action: &Value, seat_index: usize) -> bool {
    json_array_contains_seat(
        pending_action
            .get("offered_hu_seats")
            .and_then(Value::as_array),
        seat_index,
    )
}

pub fn can_declare_hu_with_cache(
    room: &Value,
    cache: &RoomScoringCache,
    seat_index: usize,
    incoming_tile: Option<&str>,
    discarder_seat: Option<usize>,
) -> bool {
    if room
        .get("round_state")
        .and_then(|round| round.get("pending_action"))
        .and_then(|pending| pending.get("type"))
        .and_then(Value::as_str)
        == Some("opening_flowers")
    {
        return false;
    }

    if let Ok(fan_result) =
        fan_result_for_win_with_cache(room, cache, seat_index, incoming_tile, discarder_seat)
    {
        let enforce_minimum_eight_fan = room
            .get("round_state")
            .and_then(|round| round.get("enforce_minimum_eight_fan"))
            .and_then(Value::as_bool)
            .or_else(|| {
                room.get("enforce_minimum_eight_fan")
                    .and_then(Value::as_bool)
            })
            .unwrap_or(true);
        return !enforce_minimum_eight_fan || fan_result.minimum_qualifying_fan_total >= 8;
    }
    false
}

fn json_array_contains_seat(values: Option<&Vec<Value>>, seat_index: usize) -> bool {
    values.is_some_and(|items| {
        items.iter().any(|value| {
            value
                .as_u64()
                .map(|seat| seat as usize == seat_index)
                .unwrap_or(false)
        })
    })
}

fn json_array_contains_str(values: Option<&Vec<Value>>, needle: &str) -> bool {
    values.is_some_and(|items| items.iter().any(|value| value.as_str() == Some(needle)))
}

fn fan_result_for_win(
    room: &Value,
    winner_seat: usize,
    incoming_tile: Option<&str>,
    discarder_seat: Option<usize>,
) -> Result<crate::scoring::FanResult, String> {
    let cache = RoomScoringCache::from_room(room);
    fan_result_for_win_with_cache(room, &cache, winner_seat, incoming_tile, discarder_seat)
}

fn fan_result_for_win_with_cache(
    room: &Value,
    cache: &RoomScoringCache,
    winner_seat: usize,
    incoming_tile: Option<&str>,
    discarder_seat: Option<usize>,
) -> Result<crate::scoring::FanResult, String> {
    let PreparedWinEvaluation {
        concealed_tile_keys,
        meld_tile_key_groups,
        open_meld_tile_key_groups,
        meld_open_flags,
        decompositions,
        kong_entries,
    } = prepare_win_evaluation(cache, winner_seat, incoming_tile)?;

    let win_type = if incoming_tile.is_none() {
        "self_draw"
    } else {
        "discard"
    }
    .to_string();
    let features = scoring_extract_hand_features(
        &concealed_tile_keys,
        &meld_tile_key_groups,
        Some(&meld_open_flags),
        incoming_tile,
        Some(&seat_wind_key(winner_seat, cache.dealer_seat)),
        cache.round_wind.as_deref(),
        Some(&decompositions),
    );

    let player_tile_keys =
        player_tile_keys_from_parts(&concealed_tile_keys, &meld_tile_key_groups, incoming_tile);

    Ok(scoring_evaluate_fans(ScoringEvaluationInput {
        win_type,
        winner_seat: Some(winner_seat),
        discarder_seat,
        flower_count: cache
            .player(winner_seat)
            .map(|player| player.flower_count)
            .unwrap_or(0),
        seat_count: cache.seat_count,
        features,
        timing: timing_features_for_win(room, incoming_tile.is_none()),
        kong_entries,
        tile_keys: player_tile_keys,
        visible_tile_keys: cache.visible_tile_keys.clone(),
        concealed_tile_keys,
        meld_tile_key_groups,
        open_meld_tile_key_groups,
        incoming_tile: incoming_tile.map(ToString::to_string),
        decompositions,
    }))
}

fn prepare_win_evaluation(
    cache: &RoomScoringCache,
    winner_seat: usize,
    incoming_tile: Option<&str>,
) -> Result<PreparedWinEvaluation, String> {
    let player = cache
        .player(winner_seat)
        .ok_or_else(|| "invalid_action".to_string())?;
    let concealed_tile_keys = player.concealed_tile_keys.clone();
    let meld_tile_key_groups = player.meld_tile_key_groups.clone();

    let mut effective_concealed_tile_keys =
        Vec::with_capacity(concealed_tile_keys.len() + usize::from(incoming_tile.is_some()));
    effective_concealed_tile_keys.extend(concealed_tile_keys.iter().cloned());
    if let Some(tile_key) = incoming_tile {
        effective_concealed_tile_keys.push(tile_key.to_string());
    }

    let decompositions = scoring_decompose_winning_hand_with_melds(
        &effective_concealed_tile_keys,
        &meld_tile_key_groups,
    );
    if decompositions.is_empty() {
        return Err("invalid_action".to_string());
    }

    let kong_entries = cache.kong_entries.clone();
    let (open_meld_tile_key_groups, meld_open_flags) =
        classify_meld_groups(winner_seat, &meld_tile_key_groups, &kong_entries);

    Ok(PreparedWinEvaluation {
        concealed_tile_keys,
        meld_tile_key_groups,
        open_meld_tile_key_groups,
        meld_open_flags,
        decompositions,
        kong_entries,
    })
}

fn seat_wind_key(seat_index: usize, dealer_seat: usize) -> String {
    WIND_ORDER[(seat_index + MAX_SEATS - dealer_seat) % MAX_SEATS].to_string()
}

fn player_tile_keys_from_parts(
    concealed_tile_keys: &[String],
    meld_tile_key_groups: &[Vec<String>],
    incoming_tile: Option<&str>,
) -> Vec<String> {
    let meld_tile_count = meld_tile_key_groups
        .iter()
        .map(|meld| {
            if meld.len() == 4 && meld.iter().all(|tile_key| tile_key == &meld[0]) {
                3
            } else {
                meld.len()
            }
        })
        .sum::<usize>();
    let mut tile_keys = Vec::with_capacity(
        concealed_tile_keys.len() + meld_tile_count + usize::from(incoming_tile.is_some()),
    );
    tile_keys.extend(concealed_tile_keys.iter().cloned());
    for meld in meld_tile_key_groups {
        if meld.len() == 4 && meld.iter().all(|tile_key| tile_key == &meld[0]) {
            tile_keys.extend(meld.iter().take(3).cloned());
        } else {
            tile_keys.extend(meld.iter().cloned());
        }
    }
    if let Some(tile_key) = incoming_tile {
        tile_keys.push(tile_key.to_string());
    }
    tile_keys
}

fn classify_meld_groups(
    seat_index: usize,
    meld_tile_key_groups: &[Vec<String>],
    kong_entries: &[ScoringKongEntry],
) -> (Vec<Vec<String>>, Vec<bool>) {
    let mut open_meld_tile_key_groups = Vec::new();
    let mut meld_open_flags = Vec::with_capacity(meld_tile_key_groups.len());
    for meld in meld_tile_key_groups {
        let is_open = meld_is_open_with_entries(seat_index, meld, kong_entries);
        meld_open_flags.push(is_open);
        if is_open {
            open_meld_tile_key_groups.push(meld.clone());
        }
    }
    (open_meld_tile_key_groups, meld_open_flags)
}

fn meld_is_open_with_entries(
    seat_index: usize,
    meld: &[String],
    kong_entries: &[ScoringKongEntry],
) -> bool {
    if meld.len() != 4 || !meld.iter().all(|tile_key| tile_key == &meld[0]) {
        return true;
    }

    let tile_key = meld[0].as_str();
    for entry in kong_entries.iter().rev() {
        if entry.actor_seat != seat_index {
            continue;
        }
        if entry
            .tile_key
            .as_deref()
            .is_some_and(|value| value != tile_key)
        {
            continue;
        }
        return entry.kong_type != "concealed_kong";
    }
    true
}

fn timing_features_for_win(room: &Value, self_draw: bool) -> ScoringTimingFeatures {
    let context = room
        .get("round_state")
        .and_then(|round| round.get("last_action_context"))
        .cloned()
        .unwrap_or(Value::Null);
    if self_draw {
        let is_replacement = context
            .get("from_kong_replacement")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        return ScoringTimingFeatures {
            gang_shang_hua: is_replacement,
            hai_di_lao_yue: !is_replacement
                && context.get("kind").and_then(Value::as_str) == Some("draw")
                && context
                    .get("was_last_live_tile")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            he_di_lao_yu: false,
            robbing_the_kong: false,
        };
    }

    ScoringTimingFeatures {
        gang_shang_hua: false,
        hai_di_lao_yue: false,
        he_di_lao_yu: context.get("kind").and_then(Value::as_str) == Some("discard")
            && context
                .get("was_last_discard")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        robbing_the_kong: room
            .get("round_state")
            .and_then(|round| round.get("pending_action"))
            .and_then(|pending| pending.get("type"))
            .and_then(Value::as_str)
            == Some("rob_kong_window"),
    }
}
