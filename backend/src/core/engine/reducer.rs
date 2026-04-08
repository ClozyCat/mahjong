use serde_json::{Value, json};

use crate::core::state::{
    ContinueActionState, EffectState, LastActionContext, MatchSkillTrackers, MatchState,
    PendingAction, PendingTimeout, RoomState, RoundScoreTrackers, RoundSettlement,
    RoundSkillTrackers, RoundState,
};
use crate::core::tile::Tile;

#[derive(Debug, Clone)]
pub enum LegacyRoomMutation {
    RemovePlayerConcealedTileById {
        seat_index: usize,
        tile_id: String,
    },
    ReplacePlayerConcealedTileById {
        seat_index: usize,
        tile_id: String,
        tile: Tile,
    },
    PushPlayerDiscard {
        seat_index: usize,
        tile: Tile,
    },
    PushPlayerConcealedTile {
        seat_index: usize,
        tile: Tile,
    },
    PushPlayerMeld {
        seat_index: usize,
        meld: Vec<String>,
    },
    PushPlayerFlower {
        seat_index: usize,
        tile: Tile,
    },
    AppendTileToPlayerMeld {
        seat_index: usize,
        meld_index: usize,
        tile_key: String,
    },
    RemovePlayerMeldAt {
        seat_index: usize,
        meld_index: usize,
    },
    PopPlayerDiscardLast {
        seat_index: usize,
    },
    AdvanceWallHead,
    RetreatWallTail,
    SetRoundLastDiscard {
        tile: Option<Tile>,
    },
    SetRoundPendingAction {
        pending_action: Option<PendingAction>,
    },
    SetRoundRestrictedDiscardTileKey {
        tile_key: Option<String>,
    },
    SetRoundLastActionContext {
        context: LastActionContext,
    },
    SetRoundCurrentActor {
        seat_index: usize,
    },
    SetRoundScoreTrackers {
        score_trackers: RoundScoreTrackers,
    },
    SetRoundSkillTrackers {
        trackers: RoundSkillTrackers,
    },
    SetRoomPhase {
        phase: String,
    },
    SetRoomPendingTimeout {
        pending_timeout: Option<PendingTimeout>,
    },
    SetRoomMatchState {
        match_state: Option<MatchState>,
    },
    SetRoomRoundState {
        round_state: Option<RoundState>,
    },
    SetContinueActionAutoAdvanceDeadline {
        deadline_at: Option<String>,
    },
    SetStartNextRoundConfirmedSeats {
        seats: Vec<usize>,
    },
    AddStartNextRoundConfirmedSeat {
        seat_index: usize,
    },
    SetRestartMatchConfirmedSeats {
        seats: Vec<usize>,
    },
    AddRestartMatchConfirmedSeat {
        seat_index: usize,
    },
    SetRoundPhase {
        phase: String,
    },
    SetRoundSettlement {
        settlement: Option<RoundSettlement>,
    },
    SetMatchPrevailingWind {
        prevailing_wind: String,
    },
    SetMatchHandNumber {
        hand_number: u32,
    },
    SetMatchDealerSeat {
        dealer_seat: usize,
    },
    SetMatchFinished {
        match_finished: bool,
    },
    SetMatchCumulativeScores {
        cumulative_scores: std::collections::BTreeMap<usize, i64>,
    },
    SetMatchLastCompletedRoundId {
        round_id: Option<String>,
    },
    SetMatchSkillTrackers {
        trackers: MatchSkillTrackers,
    },
    IncrementRoundVersion,
    AppendRoundKongEntry {
        kong_type: String,
        actor_seat: usize,
        payer_seats: Vec<usize>,
        tile_key: Option<String>,
    },
}

pub fn apply_legacy_room_mutations(
    room: &mut Value,
    mutations: &[LegacyRoomMutation],
) -> Result<(), String> {
    if room.is_object() && room.get("table_code").is_some() {
        let mut state = RoomState::from_legacy_value(room).map_err(|error| error.to_string())?;
        apply_legacy_room_mutations_to_state(&mut state, mutations)?;
        *room = state.to_legacy_value().map_err(|error| error.to_string())?;
        return Ok(());
    }
    apply_legacy_room_mutations_to_value(room, mutations)
}

fn apply_legacy_room_mutations_to_value(
    room: &mut Value,
    mutations: &[LegacyRoomMutation],
) -> Result<(), String> {
    for mutation in mutations {
        apply_legacy_room_mutation_to_value(room, mutation)?;
    }
    Ok(())
}

fn apply_legacy_room_mutations_to_state(
    room: &mut RoomState,
    mutations: &[LegacyRoomMutation],
) -> Result<(), String> {
    for mutation in mutations {
        apply_legacy_room_mutation_to_state(room, mutation)?;
    }
    refresh_continue_action_state(room);
    Ok(())
}

fn apply_legacy_room_mutation_to_value(
    room: &mut Value,
    mutation: &LegacyRoomMutation,
) -> Result<(), String> {
    match mutation {
        LegacyRoomMutation::RemovePlayerConcealedTileById {
            seat_index,
            tile_id,
        } => {
            let concealed_tiles = player_zone_mut(room, *seat_index, "concealed_tiles")?;
            let tile_index = concealed_tiles
                .iter()
                .position(|tile| {
                    tile.get("tile_id").and_then(Value::as_str) == Some(tile_id.as_str())
                })
                .ok_or_else(|| "invalid_action".to_string())?;
            concealed_tiles.remove(tile_index);
            Ok(())
        }
        LegacyRoomMutation::ReplacePlayerConcealedTileById {
            seat_index,
            tile_id,
            tile,
        } => {
            let concealed_tiles = player_zone_mut(room, *seat_index, "concealed_tiles")?;
            let tile_index = concealed_tiles
                .iter()
                .position(|current| {
                    current.get("tile_id").and_then(Value::as_str) == Some(tile_id.as_str())
                })
                .ok_or_else(|| "invalid_action".to_string())?;
            concealed_tiles[tile_index] = serde_json::to_value(tile).unwrap_or(Value::Null);
            Ok(())
        }
        LegacyRoomMutation::PushPlayerDiscard { seat_index, tile } => {
            player_zone_mut(room, *seat_index, "discards")?
                .push(serde_json::to_value(tile).unwrap_or(Value::Null));
            Ok(())
        }
        LegacyRoomMutation::PushPlayerConcealedTile { seat_index, tile } => {
            player_zone_mut(room, *seat_index, "concealed_tiles")?
                .push(serde_json::to_value(tile).unwrap_or(Value::Null));
            Ok(())
        }
        LegacyRoomMutation::PushPlayerMeld { seat_index, meld } => {
            player_zone_mut(room, *seat_index, "melds")?
                .push(serde_json::to_value(meld).unwrap_or(Value::Array(vec![])));
            Ok(())
        }
        LegacyRoomMutation::PushPlayerFlower { seat_index, tile } => {
            player_zone_mut(room, *seat_index, "flowers")?
                .push(serde_json::to_value(tile).unwrap_or(Value::Null));
            Ok(())
        }
        LegacyRoomMutation::AppendTileToPlayerMeld {
            seat_index,
            meld_index,
            tile_key,
        } => {
            let melds = player_zone_mut(room, *seat_index, "melds")?;
            let meld = melds
                .get_mut(*meld_index)
                .and_then(Value::as_array_mut)
                .ok_or_else(|| "invalid_action".to_string())?;
            meld.push(Value::String(tile_key.clone()));
            Ok(())
        }
        LegacyRoomMutation::RemovePlayerMeldAt {
            seat_index,
            meld_index,
        } => {
            let melds = player_zone_mut(room, *seat_index, "melds")?;
            if *meld_index >= melds.len() {
                return Err("invalid_action".to_string());
            }
            melds.remove(*meld_index);
            Ok(())
        }
        LegacyRoomMutation::PopPlayerDiscardLast { seat_index } => {
            player_zone_mut(room, *seat_index, "discards")?.pop();
            Ok(())
        }
        LegacyRoomMutation::AdvanceWallHead => {
            let wall = room
                .get_mut("round_state")
                .and_then(Value::as_object_mut)
                .and_then(|round_state| round_state.get_mut("wall"))
                .and_then(Value::as_object_mut)
                .ok_or_else(|| "invalid_action".to_string())?;
            let head_index = wall
                .get("head_index")
                .and_then(Value::as_u64)
                .ok_or_else(|| "invalid_action".to_string())?;
            wall.insert("head_index".to_string(), json!(head_index + 1));
            Ok(())
        }
        LegacyRoomMutation::RetreatWallTail => {
            let wall = room
                .get_mut("round_state")
                .and_then(Value::as_object_mut)
                .and_then(|round_state| round_state.get_mut("wall"))
                .and_then(Value::as_object_mut)
                .ok_or_else(|| "invalid_action".to_string())?;
            let tail_index = wall
                .get("tail_index")
                .and_then(Value::as_i64)
                .ok_or_else(|| "invalid_action".to_string())?;
            wall.insert("tail_index".to_string(), json!(tail_index - 1));
            Ok(())
        }
        LegacyRoomMutation::SetRoundLastDiscard { tile } => round_state_insert(
            room,
            "last_discard",
            tile.as_ref()
                .map(|tile| serde_json::to_value(tile).unwrap_or(Value::Null))
                .unwrap_or(Value::Null),
        ),
        LegacyRoomMutation::SetRoundPendingAction { pending_action } => round_state_insert(
            room,
            "pending_action",
            pending_action
                .as_ref()
                .map(PendingAction::to_legacy_value)
                .unwrap_or(Value::Null),
        ),
        LegacyRoomMutation::SetRoundRestrictedDiscardTileKey { tile_key } => round_state_insert(
            room,
            "restricted_discard_tile_key",
            tile_key.clone().map(Value::String).unwrap_or(Value::Null),
        ),
        LegacyRoomMutation::SetRoundLastActionContext { context } => round_state_insert(
            room,
            "last_action_context",
            serde_json::to_value(context).unwrap_or(Value::Null),
        ),
        LegacyRoomMutation::SetRoundCurrentActor { seat_index } => {
            round_state_insert(room, "current_actor", json!(seat_index))
        }
        LegacyRoomMutation::SetRoundScoreTrackers { score_trackers } => round_state_insert(
            room,
            "score_trackers",
            serde_json::to_value(score_trackers).unwrap_or(Value::Null),
        ),
        LegacyRoomMutation::SetRoundSkillTrackers { trackers } => {
            round_state_insert(room, "skill_trackers", trackers.to_legacy_value())
        }
        LegacyRoomMutation::SetRoomPhase { phase } => {
            let object = room
                .as_object_mut()
                .ok_or_else(|| "invalid_action".to_string())?;
            object.insert("phase".to_string(), Value::String(phase.clone()));
            Ok(())
        }
        LegacyRoomMutation::SetRoomPendingTimeout { pending_timeout } => {
            let object = room
                .as_object_mut()
                .ok_or_else(|| "invalid_action".to_string())?;
            object.insert(
                "pending_timeout".to_string(),
                pending_timeout
                    .as_ref()
                    .map(|timeout| serde_json::to_value(timeout).unwrap_or(Value::Null))
                    .unwrap_or(Value::Null),
            );
            Ok(())
        }
        LegacyRoomMutation::SetRoomMatchState { match_state } => {
            let object = room
                .as_object_mut()
                .ok_or_else(|| "invalid_action".to_string())?;
            object.insert(
                "match_state".to_string(),
                match_state
                    .as_ref()
                    .map(|match_state| serde_json::to_value(match_state).unwrap_or(Value::Null))
                    .unwrap_or(Value::Null),
            );
            Ok(())
        }
        LegacyRoomMutation::SetRoomRoundState { round_state } => {
            let object = room
                .as_object_mut()
                .ok_or_else(|| "invalid_action".to_string())?;
            object.insert(
                "round_state".to_string(),
                round_state
                    .as_ref()
                    .map(|round| round.to_legacy_value().unwrap_or(Value::Null))
                    .unwrap_or(Value::Null),
            );
            Ok(())
        }
        LegacyRoomMutation::SetContinueActionAutoAdvanceDeadline { deadline_at } => {
            let object = room
                .as_object_mut()
                .ok_or_else(|| "invalid_action".to_string())?;
            object.insert(
                "continue_action_auto_advance_deadline_at".to_string(),
                deadline_at
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            );
            Ok(())
        }
        LegacyRoomMutation::SetStartNextRoundConfirmedSeats { seats } => {
            let object = room
                .as_object_mut()
                .ok_or_else(|| "invalid_action".to_string())?;
            object.insert(
                "start_next_round_confirmed_seats".to_string(),
                Value::Array(seats.iter().map(|seat| json!(seat)).collect()),
            );
            Ok(())
        }
        LegacyRoomMutation::AddStartNextRoundConfirmedSeat { seat_index } => {
            push_unique_seat_array_value(room, "start_next_round_confirmed_seats", *seat_index)
        }
        LegacyRoomMutation::SetRestartMatchConfirmedSeats { seats } => {
            let object = room
                .as_object_mut()
                .ok_or_else(|| "invalid_action".to_string())?;
            object.insert(
                "restart_match_confirmed_seats".to_string(),
                Value::Array(seats.iter().map(|seat| json!(seat)).collect()),
            );
            Ok(())
        }
        LegacyRoomMutation::AddRestartMatchConfirmedSeat { seat_index } => {
            push_unique_seat_array_value(room, "restart_match_confirmed_seats", *seat_index)
        }
        LegacyRoomMutation::SetRoundPhase { phase } => {
            round_state_insert(room, "phase", json!(phase))
        }
        LegacyRoomMutation::SetRoundSettlement { settlement } => round_state_insert(
            room,
            "settlement",
            settlement
                .as_ref()
                .map(RoundSettlement::to_legacy_value)
                .unwrap_or(Value::Null),
        ),
        LegacyRoomMutation::SetMatchPrevailingWind { prevailing_wind } => {
            match_state_insert(room, "prevailing_wind", json!(prevailing_wind))
        }
        LegacyRoomMutation::SetMatchHandNumber { hand_number } => {
            match_state_insert(room, "hand_number", json!(hand_number))
        }
        LegacyRoomMutation::SetMatchDealerSeat { dealer_seat } => {
            match_state_insert(room, "dealer_seat", json!(dealer_seat))
        }
        LegacyRoomMutation::SetMatchFinished { match_finished } => {
            match_state_insert(room, "match_finished", json!(match_finished))
        }
        LegacyRoomMutation::SetMatchCumulativeScores { cumulative_scores } => {
            let object = cumulative_scores
                .iter()
                .map(|(seat, score)| (seat.to_string(), json!(score)))
                .collect();
            match_state_insert(room, "cumulative_scores", Value::Object(object))
        }
        LegacyRoomMutation::SetMatchLastCompletedRoundId { round_id } => match_state_insert(
            room,
            "last_completed_round_id",
            round_id.clone().map(Value::String).unwrap_or(Value::Null),
        ),
        LegacyRoomMutation::SetMatchSkillTrackers { trackers } => {
            match_state_insert(room, "skill_trackers", trackers.to_legacy_value())
        }
        LegacyRoomMutation::IncrementRoundVersion => {
            let round_state = room
                .get_mut("round_state")
                .and_then(Value::as_object_mut)
                .ok_or_else(|| "invalid_action".to_string())?;
            let version = round_state
                .get("version")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                + 1;
            round_state.insert("version".to_string(), json!(version));
            Ok(())
        }
        LegacyRoomMutation::AppendRoundKongEntry {
            kong_type,
            actor_seat,
            payer_seats,
            tile_key,
        } => {
            let round_state = room
                .get_mut("round_state")
                .and_then(Value::as_object_mut)
                .ok_or_else(|| "invalid_action".to_string())?;
            let score_trackers = round_state
                .entry("score_trackers".to_string())
                .or_insert_with(|| json!({}));
            let trackers = score_trackers
                .as_object_mut()
                .ok_or_else(|| "invalid_action".to_string())?;
            let kong_entries = trackers
                .entry("kong_entries".to_string())
                .or_insert_with(|| Value::Array(vec![]));
            let entries = kong_entries
                .as_array_mut()
                .ok_or_else(|| "invalid_action".to_string())?;
            entries.push(json!({
                "kong_type": kong_type,
                "actor_seat": actor_seat,
                "payer_seats": payer_seats,
                "tile_key": tile_key,
            }));
            Ok(())
        }
    }
}

fn apply_legacy_room_mutation_to_state(
    room: &mut RoomState,
    mutation: &LegacyRoomMutation,
) -> Result<(), String> {
    match mutation {
        LegacyRoomMutation::RemovePlayerConcealedTileById {
            seat_index,
            tile_id,
        } => {
            let player = player_mut(room, *seat_index)?;
            let tile_index = player
                .concealed_tiles
                .iter()
                .position(|tile| tile.tile_id == *tile_id)
                .ok_or_else(|| "invalid_action".to_string())?;
            player.concealed_tiles.remove(tile_index);
            Ok(())
        }
        LegacyRoomMutation::ReplacePlayerConcealedTileById {
            seat_index,
            tile_id,
            tile,
        } => {
            let player = player_mut(room, *seat_index)?;
            let tile_index = player
                .concealed_tiles
                .iter()
                .position(|current| current.tile_id == *tile_id)
                .ok_or_else(|| "invalid_action".to_string())?;
            player.concealed_tiles[tile_index] = tile.clone();
            Ok(())
        }
        LegacyRoomMutation::PushPlayerDiscard { seat_index, tile } => {
            player_mut(room, *seat_index)?.discards.push(tile.clone());
            Ok(())
        }
        LegacyRoomMutation::PushPlayerConcealedTile { seat_index, tile } => {
            player_mut(room, *seat_index)?
                .concealed_tiles
                .push(tile.clone());
            Ok(())
        }
        LegacyRoomMutation::PushPlayerMeld { seat_index, meld } => {
            player_mut(room, *seat_index)?.melds.push(meld.clone());
            Ok(())
        }
        LegacyRoomMutation::PushPlayerFlower { seat_index, tile } => {
            player_mut(room, *seat_index)?.flowers.push(tile.clone());
            Ok(())
        }
        LegacyRoomMutation::AppendTileToPlayerMeld {
            seat_index,
            meld_index,
            tile_key,
        } => {
            let player = player_mut(room, *seat_index)?;
            let meld = player
                .melds
                .get_mut(*meld_index)
                .ok_or_else(|| "invalid_action".to_string())?;
            meld.push(tile_key.clone());
            Ok(())
        }
        LegacyRoomMutation::RemovePlayerMeldAt {
            seat_index,
            meld_index,
        } => {
            let player = player_mut(room, *seat_index)?;
            if *meld_index >= player.melds.len() {
                return Err("invalid_action".to_string());
            }
            player.melds.remove(*meld_index);
            Ok(())
        }
        LegacyRoomMutation::PopPlayerDiscardLast { seat_index } => {
            player_mut(room, *seat_index)?.discards.pop();
            Ok(())
        }
        LegacyRoomMutation::AdvanceWallHead => {
            round_mut(room)?.wall.head_index += 1;
            Ok(())
        }
        LegacyRoomMutation::RetreatWallTail => {
            let round = round_mut(room)?;
            round.wall.tail_index = round.wall.tail_index.saturating_sub(1);
            Ok(())
        }
        LegacyRoomMutation::SetRoundLastDiscard { tile } => {
            round_mut(room)?.last_discard = tile.clone();
            Ok(())
        }
        LegacyRoomMutation::SetRoundPendingAction { pending_action } => {
            round_mut(room)?.pending_action = pending_action.clone();
            Ok(())
        }
        LegacyRoomMutation::SetRoundRestrictedDiscardTileKey { tile_key } => {
            round_mut(room)?.restricted_discard_tile_key = tile_key.clone();
            Ok(())
        }
        LegacyRoomMutation::SetRoundLastActionContext { context } => {
            round_mut(room)?.last_action_context = context.clone();
            Ok(())
        }
        LegacyRoomMutation::SetRoundCurrentActor { seat_index } => {
            round_mut(room)?.current_actor = *seat_index;
            Ok(())
        }
        LegacyRoomMutation::SetRoundScoreTrackers { score_trackers } => {
            round_mut(room)?.score_trackers = score_trackers.clone();
            Ok(())
        }
        LegacyRoomMutation::SetRoundSkillTrackers { trackers } => {
            round_mut(room)?.skill_trackers = trackers.clone();
            Ok(())
        }
        LegacyRoomMutation::SetRoomPhase { phase } => {
            room.phase = phase.clone();
            Ok(())
        }
        LegacyRoomMutation::SetRoomPendingTimeout { pending_timeout } => {
            room.pending_timeout = pending_timeout.clone();
            Ok(())
        }
        LegacyRoomMutation::SetRoomMatchState { match_state } => {
            room.match_state = match_state.clone();
            Ok(())
        }
        LegacyRoomMutation::SetRoomRoundState { round_state } => {
            room.round_state = round_state.clone();
            Ok(())
        }
        LegacyRoomMutation::SetContinueActionAutoAdvanceDeadline { deadline_at } => {
            let action_id = match room.phase.as_str() {
                "settlement" => "start_next_round",
                "finished" => "restart_match",
                _ => return Ok(()),
            };
            ensure_continue_action(room, action_id).auto_advance_deadline_at = deadline_at.clone();
            Ok(())
        }
        LegacyRoomMutation::SetStartNextRoundConfirmedSeats { seats } => {
            ensure_continue_action(room, "start_next_round").confirmed_seats = seats.clone();
            Ok(())
        }
        LegacyRoomMutation::AddStartNextRoundConfirmedSeat { seat_index } => {
            let action = ensure_continue_action(room, "start_next_round");
            if !action.confirmed_seats.contains(seat_index) {
                action.confirmed_seats.push(*seat_index);
            }
            Ok(())
        }
        LegacyRoomMutation::SetRestartMatchConfirmedSeats { seats } => {
            ensure_continue_action(room, "restart_match").confirmed_seats = seats.clone();
            Ok(())
        }
        LegacyRoomMutation::AddRestartMatchConfirmedSeat { seat_index } => {
            let action = ensure_continue_action(room, "restart_match");
            if !action.confirmed_seats.contains(seat_index) {
                action.confirmed_seats.push(*seat_index);
            }
            Ok(())
        }
        LegacyRoomMutation::SetRoundPhase { phase } => {
            round_mut(room)?.phase = phase.clone();
            Ok(())
        }
        LegacyRoomMutation::SetRoundSettlement { settlement } => {
            round_mut(room)?.settlement = settlement.clone();
            Ok(())
        }
        LegacyRoomMutation::SetMatchPrevailingWind { prevailing_wind } => {
            match_mut(room)?.prevailing_wind = prevailing_wind.clone();
            Ok(())
        }
        LegacyRoomMutation::SetMatchHandNumber { hand_number } => {
            match_mut(room)?.hand_number = *hand_number;
            Ok(())
        }
        LegacyRoomMutation::SetMatchDealerSeat { dealer_seat } => {
            match_mut(room)?.dealer_seat = *dealer_seat;
            Ok(())
        }
        LegacyRoomMutation::SetMatchFinished { match_finished } => {
            match_mut(room)?.match_finished = *match_finished;
            Ok(())
        }
        LegacyRoomMutation::SetMatchCumulativeScores { cumulative_scores } => {
            match_mut(room)?.cumulative_scores = cumulative_scores.clone();
            Ok(())
        }
        LegacyRoomMutation::SetMatchLastCompletedRoundId { round_id } => {
            match_mut(room)?.last_completed_round_id = round_id.clone();
            Ok(())
        }
        LegacyRoomMutation::SetMatchSkillTrackers { trackers } => {
            match_mut(room)?.skill_trackers = trackers.clone();
            Ok(())
        }
        LegacyRoomMutation::IncrementRoundVersion => {
            round_mut(room)?.version += 1;
            Ok(())
        }
        LegacyRoomMutation::AppendRoundKongEntry {
            kong_type,
            actor_seat,
            payer_seats,
            tile_key,
        } => {
            round_mut(room)?.score_trackers.kong_entries.push(
                crate::core::state::KongTrackerEntry {
                    kong_type: kong_type.clone(),
                    actor_seat: *actor_seat,
                    payer_seats: payer_seats.clone(),
                    tile_key: tile_key.clone(),
                },
            );
            Ok(())
        }
    }
}

fn round_mut(room: &mut RoomState) -> Result<&mut RoundState, String> {
    room.round_state
        .as_mut()
        .ok_or_else(|| "invalid_action".to_string())
}

fn match_mut(room: &mut RoomState) -> Result<&mut MatchState, String> {
    room.match_state
        .as_mut()
        .ok_or_else(|| "invalid_action".to_string())
}

fn player_mut(
    room: &mut RoomState,
    seat_index: usize,
) -> Result<&mut crate::core::state::PlayerRoundState, String> {
    round_mut(room)?
        .players
        .get_mut(seat_index)
        .ok_or_else(|| "invalid_action".to_string())
}

fn parse_tile(value: &Value) -> Result<Tile, String> {
    Tile::from_legacy_value(value, "mutation.tile").map_err(|error| error.to_string())
}

fn parse_meld(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|tiles| {
            tiles
                .iter()
                .filter_map(|tile| tile.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_tile_key(value: &Value) -> String {
    value
        .as_str()
        .map(ToString::to_string)
        .or_else(|| {
            value
                .get("tile_key")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .unwrap_or_default()
}

fn parse_pending_action(value: &Value) -> Option<PendingAction> {
    if value.is_null() {
        None
    } else {
        PendingAction::from_legacy_value(value)
    }
}

fn ensure_continue_action<'a>(
    room: &'a mut RoomState,
    action_id: &str,
) -> &'a mut ContinueActionState {
    let required_seats = room
        .seats
        .iter()
        .filter(|seat| !seat.is_bot)
        .map(|seat| seat.seat_index)
        .collect::<Vec<_>>();
    let online_seats = room
        .seats
        .iter()
        .filter(|seat| !seat.is_bot && seat.connected)
        .map(|seat| seat.seat_index)
        .collect::<Vec<_>>();
    room.continue_action
        .get_or_insert_with(|| ContinueActionState {
            action_id: action_id.to_string(),
            confirmed_seats: Vec::new(),
            required_seats: required_seats.clone(),
            online_seats: online_seats.clone(),
            auto_advance_deadline_at: None,
        });
    let action = room
        .continue_action
        .as_mut()
        .expect("continue action inserted");
    action.action_id = action_id.to_string();
    action.required_seats = required_seats;
    action.online_seats = online_seats;
    action
}

fn refresh_continue_action_state(room: &mut RoomState) {
    let action_id = match room.phase.as_str() {
        "settlement" => Some("start_next_round"),
        "finished" => Some("restart_match"),
        _ => None,
    };
    let Some(action_id) = action_id else {
        room.continue_action = None;
        return;
    };
    let confirmed = room
        .continue_action
        .as_ref()
        .filter(|action| action.action_id == action_id)
        .map(|action| action.confirmed_seats.clone())
        .unwrap_or_default();
    let deadline = room
        .continue_action
        .as_ref()
        .filter(|action| action.action_id == action_id)
        .and_then(|action| action.auto_advance_deadline_at.clone());
    let action = ensure_continue_action(room, action_id);
    action.confirmed_seats = confirmed;
    action.auto_advance_deadline_at = deadline;
}

fn player_zone_mut<'a>(
    room: &'a mut Value,
    seat_index: usize,
    zone: &str,
) -> Result<&'a mut Vec<Value>, String> {
    room.get_mut("round_state")
        .and_then(Value::as_object_mut)
        .and_then(|round_state| round_state.get_mut("players"))
        .and_then(Value::as_array_mut)
        .and_then(|players| players.get_mut(seat_index))
        .and_then(Value::as_object_mut)
        .and_then(|player| player.get_mut(zone))
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "invalid_action".to_string())
}

fn round_state_insert(room: &mut Value, key: &str, value: Value) -> Result<(), String> {
    let round_state = room
        .get_mut("round_state")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "invalid_action".to_string())?;
    round_state.insert(key.to_string(), value);
    Ok(())
}

fn match_state_insert(room: &mut Value, key: &str, value: Value) -> Result<(), String> {
    let match_state = room
        .get_mut("match_state")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "invalid_action".to_string())?;
    match_state.insert(key.to_string(), value);
    Ok(())
}

fn push_unique_seat_array_value(
    room: &mut Value,
    key: &str,
    seat_index: usize,
) -> Result<(), String> {
    let array = room
        .get_mut(key)
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "invalid_action".to_string())?;
    if !array.iter().any(|value| {
        value
            .as_u64()
            .map(|seat| seat as usize == seat_index)
            .unwrap_or(false)
    }) {
        array.push(json!(seat_index));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{LegacyRoomMutation, apply_legacy_room_mutations};
    use crate::core::tile::Tile;

    #[test]
    fn applies_basic_discard_mutations() {
        let mut room = json!({
            "round_state": {
                "version": 1,
                "head_index": 0,
                "current_actor": 0,
                "wall": {
                    "head_index": 0
                },
                "players": [
                    {
                        "concealed_tiles": [
                            {"tile_id": "w1#0", "tile_key": "w1"},
                            {"tile_id": "w2#0", "tile_key": "w2"}
                        ],
                        "discards": []
                    }
                ]
            }
        });

        let mutations = vec![
            LegacyRoomMutation::RemovePlayerConcealedTileById {
                seat_index: 0,
                tile_id: "w1#0".to_string(),
            },
            LegacyRoomMutation::PushPlayerDiscard {
                seat_index: 0,
                tile: Tile {
                    tile_id: "w1#0".to_string(),
                    tile_key: "w1".to_string(),
                    kind: String::new(),
                    suit: None,
                    rank: None,
                    name: None,
                },
            },
            LegacyRoomMutation::SetRoundLastDiscard {
                tile: Some(Tile {
                    tile_id: "w1#0".to_string(),
                    tile_key: "w1".to_string(),
                    kind: String::new(),
                    suit: None,
                    rank: None,
                    name: None,
                }),
            },
            LegacyRoomMutation::IncrementRoundVersion,
        ];

        apply_legacy_room_mutations(&mut room, &mutations).expect("mutations should apply");
        assert_eq!(
            room["round_state"]["players"][0]["concealed_tiles"],
            json!([{"tile_id":"w2#0","tile_key":"w2"}])
        );
        assert_eq!(
            room["round_state"]["players"][0]["discards"][0]["tile_id"],
            "w1#0"
        );
        assert_eq!(
            room["round_state"]["players"][0]["discards"][0]["tile_key"],
            "w1"
        );
        assert_eq!(room["round_state"]["last_discard"]["tile_key"], "w1");
        assert_eq!(room["round_state"]["version"], 2);
    }

    #[test]
    fn applies_runtime_room_mutations_through_typed_state_and_preserves_legacy_shape() {
        let mut room = json!({
            "table_code": "ROOM42",
            "phase": "settlement",
            "mode": "normal",
            "test_mode": false,
            "enforce_minimum_eight_fan": true,
            "seats": [{
                "seat_index": 0,
                "nickname": "Alice",
                "reconnect_token": "token-1",
                "player_session_id": 1,
                "connected": true,
                "ready": true,
                "is_bot": false,
                "seat_type": "human",
                "bot_persona": null,
                "bot_aggression": null,
                "disconnect_deadline_at": null
            }],
            "match_state": {
                "prevailing_wind": "east",
                "hand_number": 1,
                "dealer_seat": 0,
                "cumulative_scores": {"0": 0},
                "match_finished": false,
                "last_completed_round_id": null,
                "skill_trackers": null
            },
            "round_state": {
                "round_id": "east-1-dealer-0",
                "dealer_seat": 0,
                "round_wind": "east",
                "current_actor": 0,
                "phase": "settlement",
                "wall": {
                    "tiles": [],
                    "head_index": 0,
                    "tail_index": 0
                },
                "players": [{
                    "seat": 0,
                    "concealed_tiles": [],
                    "melds": [],
                    "flowers": [],
                    "discards": []
                }],
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
                    "tile_id": null,
                    "from_kong_replacement": false,
                    "was_last_live_tile": false,
                    "was_last_discard": false
                },
                "effect_state": {
                    "ongoing": [],
                    "hidden_knowledge": [],
                    "rule_overrides": []
                },
                "restricted_discard_tile_key": null,
                "skill_trackers": null,
                "enforce_minimum_eight_fan": true
            },
            "pending_timeout": null,
            "start_next_round_confirmed_seats": [],
            "restart_match_confirmed_seats": [],
            "continue_action_auto_advance_deadline_at": null
        });

        let mutations = vec![
            LegacyRoomMutation::AddStartNextRoundConfirmedSeat { seat_index: 0 },
            LegacyRoomMutation::SetContinueActionAutoAdvanceDeadline {
                deadline_at: Some("2026-04-08T00:00:00Z".to_string()),
            },
        ];

        apply_legacy_room_mutations(&mut room, &mutations).expect("mutations should apply");
        assert_eq!(room["start_next_round_confirmed_seats"], json!([0]));
        assert_eq!(
            room["continue_action_auto_advance_deadline_at"],
            json!("2026-04-08T00:00:00Z")
        );
        assert!(room.get("continue_action").is_none());
    }
}
