mod builtin;
pub mod effects;
pub mod hooks;
pub mod instances;
pub mod registry;
mod strategems;

use std::sync::OnceLock;

use serde_json::{Value, json};

use crate::core::engine::reducer::{LegacyRoomMutation, apply_legacy_room_mutations};
use crate::core::event::GameEvent;
use crate::core::ids::{Seat, SkillId, TileId};
use crate::core::state::{
    LastActionContext, PendingAction, PendingTimeout, RoomState, RoundSettlement,
};
use crate::core::tile::Tile;
use crate::room_scoring::RoomScoringCache;
use crate::rules::standard::runtime::project_room_state as standard_project_room_state;
use crate::rules::standard::win::can_declare_hu_with_cache;

use self::builtin::{PeekOpponentTileSkill, ScoreBoostSkill};

#[allow(unused_imports)]
pub use effects::{
    EffectInstance, EffectState, KnowledgeEffect, RuleOverride, visible_effects_for_seat,
};
#[allow(unused_imports)]
pub use hooks::{
    DrawRequest, HuCheckRequest, RuleContext, RuleHook, ScoreHookRequest, SkillActivation,
    SkillContext, SkillHookKind, SkillProjection,
};
#[allow(unused_imports)]
pub use instances::{
    SkillInstance, SkillLoadout, find_skill_instance, seat_skill_loadout, seat_skill_state,
};
pub use registry::{SkillDefinition, SkillRegistry, StaticSkillRegistry};

pub fn default_registry() -> &'static StaticSkillRegistry {
    static REGISTRY: OnceLock<StaticSkillRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = StaticSkillRegistry::new();
        registry.register(std::sync::Arc::new(ScoreBoostSkill));
        registry.register(std::sync::Arc::new(PeekOpponentTileSkill));
        for definition in strategems::definitions() {
            registry.register(definition);
        }
        registry
    })
}

pub fn activate_skill(
    room_state: &RoomState,
    actor: Seat,
    skill_id: &str,
    target: Option<Seat>,
    tile_ids: &[TileId],
) -> Result<Vec<GameEvent>, String> {
    activate_skill_with_registry(
        room_state,
        actor,
        skill_id,
        target,
        tile_ids,
        default_registry(),
    )
}

pub fn activate_skill_with_registry(
    room_state: &RoomState,
    actor: Seat,
    skill_id: &str,
    target: Option<Seat>,
    tile_ids: &[TileId],
    registry: &dyn SkillRegistry,
) -> Result<Vec<GameEvent>, String> {
    if room_state.phase != "playing" {
        return Err("invalid_action".to_string());
    }
    if room_state
        .round_state
        .as_ref()
        .map(|round| round.current_actor != actor)
        .unwrap_or(true)
    {
        return Err("invalid_action".to_string());
    }
    let skill_instance = find_skill_instance(room_state, actor, &skill_id.to_string())
        .ok_or_else(|| "skill_not_equipped".to_string())?;
    if skill_instance.owner != actor {
        return Err("invalid_action".to_string());
    }
    if skill_instance.charges == 0 {
        return Err("skill_no_charges".to_string());
    }
    let definition = registry
        .get(skill_id)
        .ok_or_else(|| "skill_not_registered".to_string())?;
    let mut ctx = SkillContext::new(room_state, actor, skill_instance);
    definition.can_activate(&ctx, target, tile_ids)?;
    definition.activate(&mut ctx, target, tile_ids)
}

#[allow(dead_code)]
pub fn has_registered_skill(registry: &dyn SkillRegistry, skill_id: &SkillId) -> bool {
    registry.get(skill_id).is_some()
}

pub fn skill_action_options(room_state: &RoomState, seat: Seat) -> Vec<String> {
    skill_action_options_with_registry(room_state, seat, default_registry())
}

pub fn skill_action_options_with_registry(
    room_state: &RoomState,
    seat: Seat,
    registry: &dyn SkillRegistry,
) -> Vec<String> {
    let mut options = Vec::new();
    let _ = for_each_equipped_skill(room_state, registry, |ctx, definition| {
        if ctx.actor == seat {
            definition.append_action_options(ctx, &mut options)?;
        }
        Ok(())
    });
    options.sort();
    options.dedup();
    options
}

pub fn build_skill_projection(room_state: &RoomState, local_seat: Seat) -> SkillProjection {
    build_skill_projection_with_registry(room_state, local_seat, default_registry())
}

pub fn build_skill_projection_with_registry(
    room_state: &RoomState,
    local_seat: Seat,
    registry: &dyn SkillRegistry,
) -> SkillProjection {
    let mut projection = SkillProjection::default();
    let _ = for_each_equipped_skill(room_state, registry, |ctx, definition| {
        definition.build_view(ctx, local_seat, &mut projection)
    });
    dedup_projection(&mut projection);
    projection
}

pub fn apply_before_scoring_hooks(
    room_state: &RoomState,
    request: &mut ScoreHookRequest,
) -> Result<(), String> {
    apply_before_scoring_hooks_with_registry(room_state, request, default_registry())
}

pub fn apply_before_scoring_hooks_with_registry(
    room_state: &RoomState,
    request: &mut ScoreHookRequest,
    registry: &dyn SkillRegistry,
) -> Result<(), String> {
    for_each_equipped_skill(room_state, registry, |ctx, definition| {
        definition.before_scoring(ctx, request)
    })
}

pub fn apply_after_scoring_hooks(
    room_state: &RoomState,
    request: &ScoreHookRequest,
    result: &mut crate::rules::scoring::FanResult,
) -> Result<(), String> {
    apply_after_scoring_hooks_with_registry(room_state, request, result, default_registry())
}

pub fn apply_after_scoring_hooks_with_registry(
    room_state: &RoomState,
    request: &ScoreHookRequest,
    result: &mut crate::rules::scoring::FanResult,
    registry: &dyn SkillRegistry,
) -> Result<(), String> {
    for_each_equipped_skill(room_state, registry, |ctx, definition| {
        definition.after_scoring(ctx, request, result)
    })
}

pub fn apply_draw_settlement_hooks(
    room_state: &RoomState,
    settlement: &mut RoundSettlement,
) -> Result<(), String> {
    apply_draw_settlement_hooks_with_registry(room_state, settlement, default_registry())
}

pub fn apply_draw_settlement_hooks_with_registry(
    room_state: &RoomState,
    settlement: &mut RoundSettlement,
    registry: &dyn SkillRegistry,
) -> Result<(), String> {
    for_each_equipped_skill(room_state, registry, |ctx, definition| {
        definition.after_draw_settlement(ctx, settlement)
    })
}

pub fn apply_skill_events_to_legacy_room(
    room: &mut Value,
    actor: Seat,
    skill_id: &str,
    events: &[GameEvent],
) -> Result<Vec<Value>, String> {
    decrement_skill_charge(room, actor, skill_id)?;
    let result = apply_events_to_legacy_room(room, events);
    sync_round_skill_trackers(room);
    result
}

pub fn apply_passive_skill_events_to_legacy_room(
    room: &mut Value,
    events: &[GameEvent],
) -> Result<Vec<Value>, String> {
    let result = apply_events_to_legacy_room(room, events);
    sync_round_skill_trackers(room);
    result
}

pub fn sync_match_skill_trackers_after_settlement(room: &mut Value) {
    let Some(state) = standard_project_room_state(room).ok() else {
        return;
    };
    let winner_seat = state
        .round_state
        .as_ref()
        .and_then(|round| round.settlement.as_ref())
        .and_then(|settlement| settlement.winner_seat);

    let seat_count = room
        .get("seats")
        .and_then(Value::as_array)
        .map(|seats| seats.len())
        .unwrap_or(4);

    let mut trackers = state
        .match_state
        .as_ref()
        .map(|match_state| match_state.skill_trackers.clone())
        .unwrap_or_default();
    if let Some(winner) = winner_seat {
        trackers
            .zou_wei_shang_ji
            .pending_win_penalty
            .remove(&winner);
    }
    match winner_seat {
        Some(winner) => {
            for seat in 0..seat_count {
                let next = if seat == winner {
                    trackers
                        .lian_huan_ji
                        .streaks
                        .get(&seat)
                        .copied()
                        .unwrap_or(0)
                        + 1
                } else {
                    0
                };
                trackers.lian_huan_ji.streaks.insert(seat, next);
            }
        }
        None => {
            for seat in 0..seat_count {
                trackers.lian_huan_ji.streaks.insert(seat, 0);
            }
        }
    }

    let _ = apply_legacy_room_mutations(
        room,
        &[LegacyRoomMutation::SetMatchSkillTrackers { trackers }],
    );
}

pub fn sync_round_skill_trackers(room: &mut Value) {
    let Some(state) = standard_project_room_state(room).ok() else {
        return;
    };
    let Some(round) = state.round_state.as_ref() else {
        return;
    };
    let seat_count = round.players.len();
    let mut trackers = round.skill_trackers.clone();

    let mut discard_counts = std::collections::BTreeMap::new();
    let mut discarded_five_by_seat = std::collections::BTreeMap::new();
    let mut discard_suits_by_seat = std::collections::BTreeMap::new();
    let mut players_with_kong = Vec::new();

    for (seat, player) in round.players.iter().enumerate() {
        let mut suit_set = std::collections::BTreeSet::new();
        let mut discarded_five = false;
        for discard in &player.discards {
            let tile_key = discard.tile_key.as_str();
            *discard_counts.entry(tile_key.to_string()).or_default() += 1;
            if is_suit_five(tile_key) {
                discarded_five = true;
            }
            if let Some(prefix) = suit_prefix(tile_key) {
                suit_set.insert(prefix.to_string());
            }
        }
        discarded_five_by_seat.insert(seat, discarded_five);
        discard_suits_by_seat.insert(seat, suit_set.into_iter().collect());
        let has_kong = player.melds.iter().any(|meld| meld.len() == 4);
        if has_kong {
            players_with_kong.push(seat);
        }
    }
    trackers.discard_counts = discard_counts;
    trackers.discarded_five_by_seat = discarded_five_by_seat;
    trackers.discard_suits_by_seat = discard_suits_by_seat;
    trackers.players_with_kong = players_with_kong;
    trackers.live_tiles_remaining = round.wall.live_tiles_remaining() as i64;
    trackers.tiles_drawn_since_opening = round.wall.head_index.saturating_sub(53) as i64;
    trackers.multi_hu_candidates = pending_multi_hu_candidates(round);

    let (tenpai_seats, tenpai_waits_by_seat) = compute_tenpai_trackers(room, seat_count);
    trackers.tenpai_seats = tenpai_seats;
    trackers.tenpai_waits_by_seat = tenpai_waits_by_seat;

    let _ = apply_legacy_room_mutations(
        room,
        &[LegacyRoomMutation::SetRoundSkillTrackers { trackers }],
    );
}

pub fn note_tracker_discard(room: &mut Value, seat: Seat, tile_key: &str) {
    let Some(state) = standard_project_room_state(room).ok() else {
        return;
    };
    let mut trackers = state
        .round_state
        .as_ref()
        .map(|round| round.skill_trackers.clone())
        .unwrap_or_default();
    if is_honor_tile_key(tile_key) {
        trackers
            .pending_honor_rebuy_tile_by_seat
            .insert(seat, tile_key.to_string());
    } else {
        trackers.pending_honor_rebuy_tile_by_seat.remove(&seat);
    }
    let _ = apply_legacy_room_mutations(
        room,
        &[LegacyRoomMutation::SetRoundSkillTrackers { trackers }],
    );
}

pub fn note_tracker_draw(room: &mut Value, seat: Seat, tile_key: &str) {
    let Some(state) = standard_project_room_state(room).ok() else {
        return;
    };
    let pending_tile = state
        .round_state
        .as_ref()
        .and_then(|round| {
            round
                .skill_trackers
                .pending_honor_rebuy_tile_by_seat
                .get(&seat)
        })
        .map(ToString::to_string);
    let mut trackers = state
        .round_state
        .as_ref()
        .map(|round| round.skill_trackers.clone())
        .unwrap_or_default();
    trackers.pending_honor_rebuy_tile_by_seat.remove(&seat);
    if pending_tile.as_deref() == Some(tile_key) {
        trackers.honor_redraw_success_by_seat.insert(seat, true);
    }
    let _ = apply_legacy_room_mutations(
        room,
        &[LegacyRoomMutation::SetRoundSkillTrackers { trackers }],
    );
}

pub fn note_tracker_claimed_discard(room: &mut Value, discarder_seat: Seat) {
    let Some(state) = standard_project_room_state(room).ok() else {
        return;
    };
    let mut trackers = state
        .round_state
        .as_ref()
        .map(|round| round.skill_trackers.clone())
        .unwrap_or_default();
    *trackers
        .claimed_discard_counts_by_seat
        .entry(discarder_seat)
        .or_default() += 1;
    let _ = apply_legacy_room_mutations(
        room,
        &[LegacyRoomMutation::SetRoundSkillTrackers { trackers }],
    );
}

fn apply_events_to_legacy_room(
    room: &mut Value,
    events: &[GameEvent],
) -> Result<Vec<Value>, String> {
    let mut emitted_messages = Vec::new();
    for event in events {
        match event {
            GameEvent::SkillActivated { seat, skill_id } => {
                emitted_messages.push(round_event_message(
                    "skill_activated",
                    json!({
                        "type": "skill_activated",
                        "seat": seat,
                        "skill_id": skill_id,
                    }),
                ));
            }
            GameEvent::EffectApplied { effect } => {
                update_round_effect_state(room, |effect_state| {
                    effect_state.ongoing.push(effect.clone());
                    Ok(())
                })?;
            }
            GameEvent::EffectExpired { effect_id } => {
                update_round_effect_state(room, |effect_state| {
                    effect_state
                        .ongoing
                        .retain(|effect| effect.effect_id != effect_id.as_str());
                    Ok(())
                })?;
            }
            GameEvent::ViewKnowledgeGranted { knowledge, .. } => {
                update_round_effect_state(room, |effect_state| {
                    effect_state.hidden_knowledge.push(knowledge.clone());
                    Ok(())
                })?;
            }
            GameEvent::RuleOverrideApplied { override_rule } => {
                update_round_effect_state(room, |effect_state| {
                    effect_state.rule_overrides.push(override_rule.clone());
                    Ok(())
                })?;
            }
            GameEvent::LegacyRoundEvent { event_type, event } => {
                handle_legacy_skill_event(room, event_type, event, &mut emitted_messages)?;
            }
            _ => {}
        }
    }
    increment_round_version(room)?;
    Ok(emitted_messages)
}

pub fn decline_hu_events(room_state: &RoomState, actor: Seat) -> Result<Vec<GameEvent>, String> {
    decline_hu_events_with_registry(room_state, actor, default_registry())
}

pub fn decline_hu_events_with_registry(
    room_state: &RoomState,
    actor: Seat,
    registry: &dyn SkillRegistry,
) -> Result<Vec<GameEvent>, String> {
    let mut events = Vec::new();
    let Some(loadout) = seat_skill_loadout(room_state, actor) else {
        return Ok(events);
    };
    for skill_instance in &loadout.equipped {
        let Some(definition) = registry.get(&skill_instance.skill_id) else {
            continue;
        };
        let ctx = RuleContext::new(room_state, actor, skill_instance);
        events.extend(definition.on_decline_hu(&ctx)?);
    }
    Ok(events)
}

fn round_event_message(event_type: &str, event: Value) -> Value {
    json!({
        "type": "round_event",
        "payload": {
            "event_type": event_type,
            "event": event,
        }
    })
}

fn pending_multi_hu_candidates(round: &crate::core::state::RoundState) -> Vec<Seat> {
    match round.pending_action.as_ref() {
        Some(PendingAction::ClaimWindow(claim)) => claim
            .claim_window
            .iter()
            .enumerate()
            .filter(|(_, claims)| claims.iter().any(|claim| claim == "hu"))
            .map(|(seat, _)| seat)
            .collect(),
        Some(PendingAction::RobKongWindow(rob)) => rob.offered_hu_seats.clone(),
        _ => Vec::new(),
    }
}

fn compute_tenpai_trackers(
    room: &Value,
    seat_count: usize,
) -> (Vec<Seat>, std::collections::BTreeMap<Seat, Vec<String>>) {
    let cache = RoomScoringCache::from_room(room);
    let mut tenpai_seats = Vec::new();
    let mut waits_by_seat = std::collections::BTreeMap::new();
    for seat in 0..seat_count {
        let waits = standard_wait_tile_keys(room, &cache, seat);
        if !waits.is_empty() {
            tenpai_seats.push(seat);
        }
        waits_by_seat.insert(seat, waits);
    }
    (tenpai_seats, waits_by_seat)
}

fn standard_wait_tile_keys(room: &Value, cache: &RoomScoringCache, seat: Seat) -> Vec<String> {
    const TILE_KEYS: [&str; 34] = [
        "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "t1", "t2", "t3", "t4", "t5", "t6",
        "t7", "t8", "t9", "b1", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9", "east", "south",
        "west", "north", "red", "green", "white",
    ];
    TILE_KEYS
        .iter()
        .filter(|tile_key| can_declare_hu_with_cache(room, cache, seat, Some(tile_key), None))
        .map(|tile_key| (*tile_key).to_string())
        .collect()
}

fn suit_prefix(tile_key: &str) -> Option<&'static str> {
    match tile_key.as_bytes().first().copied() {
        Some(b'w') => Some("w"),
        Some(b't') => Some("t"),
        Some(b'b') => Some("b"),
        _ => None,
    }
}

fn is_suit_five(tile_key: &str) -> bool {
    matches!(tile_key.as_bytes(), [b'w' | b't' | b'b', b'5'])
}

fn is_honor_tile_key(tile_key: &str) -> bool {
    suit_prefix(tile_key).is_none()
}

fn decrement_skill_charge(room: &mut Value, actor: Seat, skill_id: &str) -> Result<(), String> {
    let state = standard_project_room_state(room)?;
    let mut round = state
        .round_state
        .clone()
        .ok_or_else(|| "invalid_action".to_string())?;
    let skill = round
        .players
        .get_mut(actor)
        .and_then(|player| {
            player
                .skill_loadout
                .equipped
                .iter_mut()
                .find(|skill| skill.skill_id == skill_id)
        })
        .ok_or_else(|| "skill_not_equipped".to_string())?;
    if skill.charges == 0 {
        return Err("skill_no_charges".to_string());
    }
    skill.charges -= 1;
    apply_legacy_room_mutations(
        room,
        &[LegacyRoomMutation::SetRoomRoundState {
            round_state: Some(round),
        }],
    )
}

fn increment_round_version(room: &mut Value) -> Result<(), String> {
    apply_legacy_room_mutations(room, &[LegacyRoomMutation::IncrementRoundVersion])
}

fn handle_legacy_skill_event(
    room: &mut Value,
    event_type: &str,
    event: &Value,
    emitted_messages: &mut Vec<Value>,
) -> Result<(), String> {
    match event_type {
        "skill_replace_tile" => apply_replace_tile_event(room, event, emitted_messages),
        "skill_reclaim_meld" => apply_reclaim_meld_event(room, event, emitted_messages),
        "skill_force_draw" => apply_force_draw_event(room, event, emitted_messages),
        "skill_score_adjust" => apply_score_adjust_event(room, event, emitted_messages),
        _ => {
            emitted_messages.push(round_event_message(event_type, event.clone()));
            Ok(())
        }
    }
}

fn apply_replace_tile_event(
    room: &mut Value,
    event: &Value,
    emitted_messages: &mut Vec<Value>,
) -> Result<(), String> {
    let seat = event
        .get("seat")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .ok_or_else(|| "invalid_action".to_string())?;
    let removed_tile_id = event
        .get("removed_tile_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "invalid_action".to_string())?;
    let replacement_tile = Tile::from_legacy_value(
        event
            .get("replacement_tile")
            .ok_or_else(|| "invalid_action".to_string())?,
        "skill_replace_tile.replacement_tile",
    )
    .map_err(|_| "invalid_action".to_string())?;
    let state = standard_project_room_state(room)?;
    let round = state
        .round_state
        .as_ref()
        .ok_or_else(|| "invalid_action".to_string())?;
    let _ = round
        .players
        .get(seat)
        .and_then(|player| {
            player
                .concealed_tiles
                .iter()
                .find(|tile| tile.tile_id == removed_tile_id)
        })
        .ok_or_else(|| "invalid_action".to_string())?;

    let mut mutations = vec![
        LegacyRoomMutation::ReplacePlayerConcealedTileById {
            seat_index: seat,
            tile_id: removed_tile_id.to_string(),
            tile: replacement_tile.clone(),
        },
        LegacyRoomMutation::AdvanceWallHead,
        LegacyRoomMutation::SetRoundLastActionContext {
            context: LastActionContext {
                kind: "draw".to_string(),
                seat,
                tile_id: Some(replacement_tile.tile_id.clone()),
                from_kong_replacement: false,
                was_last_live_tile: round.wall.head_index >= round.wall.tail_index,
                was_last_discard: false,
            },
        },
    ];
    if let Some(timeout) = state
        .pending_timeout
        .as_ref()
        .filter(|timeout| timeout.kind == "active_turn" && timeout.seat_index == seat)
    {
        mutations.push(LegacyRoomMutation::SetRoomPendingTimeout {
            pending_timeout: Some(PendingTimeout {
                drawn_tile_id: Some(replacement_tile.tile_id.clone()),
                ..timeout.clone()
            }),
        });
    }
    apply_legacy_room_mutations(room, &mutations)?;
    note_tracker_draw(room, seat, &replacement_tile.tile_key);

    emitted_messages.push(round_event_message(
        "skill_tile_replaced",
        json!({
            "type": "skill_tile_replaced",
            "seat": seat,
            "removed_tile_id": removed_tile_id,
            "replacement_tile_id": replacement_tile.tile_id,
            "replacement_tile_key": replacement_tile.tile_key,
        }),
    ));
    Ok(())
}

fn apply_reclaim_meld_event(
    room: &mut Value,
    event: &Value,
    emitted_messages: &mut Vec<Value>,
) -> Result<(), String> {
    let seat = event
        .get("seat")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .ok_or_else(|| "invalid_action".to_string())?;
    let meld_index = event
        .get("meld_index")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .ok_or_else(|| "invalid_action".to_string())?;
    let tile_keys = event
        .get("tile_keys")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;
    let state = standard_project_room_state(room)?;
    let round = state
        .round_state
        .as_ref()
        .ok_or_else(|| "invalid_action".to_string())?;
    let player = round
        .players
        .get(seat)
        .ok_or_else(|| "invalid_action".to_string())?;
    if meld_index >= player.melds.len() {
        return Err("invalid_action".to_string());
    }
    let mut mutations = vec![LegacyRoomMutation::RemovePlayerMeldAt {
        seat_index: seat,
        meld_index,
    }];
    for (offset, tile_key) in tile_keys.iter().enumerate() {
        let Some(tile_key) = tile_key.as_str() else {
            continue;
        };
        mutations.push(LegacyRoomMutation::PushPlayerConcealedTile {
            seat_index: seat,
            tile: Tile {
                tile_id: format!("{tile_key}#reclaim:{seat}:{}:{offset}", round.version),
                tile_key: tile_key.to_string(),
                kind: "unknown".to_string(),
                suit: None,
                rank: None,
                name: None,
            },
        });
    }
    apply_legacy_room_mutations(room, &mutations)?;
    emitted_messages.push(round_event_message(
        "skill_reclaim_meld",
        json!({
            "type": "skill_reclaim_meld",
            "seat": seat,
            "meld_index": meld_index,
            "tile_keys": tile_keys,
        }),
    ));
    Ok(())
}

fn apply_force_draw_event(
    room: &mut Value,
    event: &Value,
    emitted_messages: &mut Vec<Value>,
) -> Result<(), String> {
    let seat = event
        .get("seat")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .ok_or_else(|| "invalid_action".to_string())?;
    let penalty = event
        .get("penalty")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let next_round_penalty = event
        .get("next_round_penalty")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let mut messages = crate::rules::standard::settlement::settle_exhaustive_draw(room);
    if let Some(mut settlement) = standard_project_room_state(room)
        .ok()
        .and_then(|state| state.round_state)
        .and_then(|round| round.settlement)
    {
        settlement.draw_type = Some("skill_forced".to_string());
        *settlement
            .score_delta
            .total_delta_by_seat
            .entry(seat)
            .or_default() -= penalty;
        *settlement
            .score_delta
            .fan_delta_by_seat
            .entry(seat)
            .or_default() -= penalty;
        apply_legacy_room_mutations(
            room,
            &[LegacyRoomMutation::SetRoundSettlement {
                settlement: Some(settlement),
            }],
        )?;
    }
    adjust_match_cumulative_score(room, seat, -penalty);
    if next_round_penalty > 0 {
        set_pending_next_round_win_penalty(room, seat, next_round_penalty);
    }
    messages.push(round_event_message(
        "skill_force_draw",
        json!({
            "type": "skill_force_draw",
            "seat": seat,
            "penalty": penalty,
            "next_round_penalty": next_round_penalty,
        }),
    ));
    emitted_messages.extend(messages);
    Ok(())
}

fn apply_score_adjust_event(
    room: &mut Value,
    event: &Value,
    emitted_messages: &mut Vec<Value>,
) -> Result<(), String> {
    let seat = event
        .get("seat")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .ok_or_else(|| "invalid_action".to_string())?;
    let delta = event
        .get("delta")
        .and_then(Value::as_i64)
        .ok_or_else(|| "invalid_action".to_string())?;
    adjust_match_cumulative_score(room, seat, delta);
    emitted_messages.push(round_event_message(
        "skill_score_adjusted",
        json!({
            "type": "skill_score_adjusted",
            "seat": seat,
            "delta": delta,
            "reason": event.get("reason").cloned().unwrap_or(Value::Null),
        }),
    ));
    Ok(())
}

fn adjust_match_cumulative_score(room: &mut Value, seat: Seat, delta: i64) {
    let Some(state) = standard_project_room_state(room).ok() else {
        return;
    };
    let Some(match_state) = state.match_state.as_ref() else {
        return;
    };
    let mut scores = match_state.cumulative_scores.clone();
    *scores.entry(seat).or_default() += delta;
    let _ = apply_legacy_room_mutations(
        room,
        &[LegacyRoomMutation::SetMatchCumulativeScores {
            cumulative_scores: scores,
        }],
    );
}

fn set_pending_next_round_win_penalty(room: &mut Value, seat: Seat, penalty: i64) {
    let Some(state) = standard_project_room_state(room).ok() else {
        return;
    };
    let mut trackers = state
        .match_state
        .as_ref()
        .map(|match_state| match_state.skill_trackers.clone())
        .unwrap_or_default();
    trackers
        .zou_wei_shang_ji
        .pending_win_penalty
        .insert(seat, penalty);
    let _ = apply_legacy_room_mutations(
        room,
        &[LegacyRoomMutation::SetMatchSkillTrackers { trackers }],
    );
}

fn update_round_effect_state<F>(room: &mut Value, mut mutate: F) -> Result<(), String>
where
    F: FnMut(&mut crate::core::state::EffectState) -> Result<(), String>,
{
    let state = standard_project_room_state(room)?;
    let mut round = state
        .round_state
        .clone()
        .ok_or_else(|| "invalid_action".to_string())?;
    mutate(&mut round.effect_state)?;
    apply_legacy_room_mutations(
        room,
        &[LegacyRoomMutation::SetRoomRoundState {
            round_state: Some(round),
        }],
    )
}

fn for_each_equipped_skill<F>(
    room_state: &RoomState,
    registry: &dyn SkillRegistry,
    mut callback: F,
) -> Result<(), String>
where
    F: FnMut(&RuleContext<'_>, &dyn SkillDefinition) -> Result<(), String>,
{
    let Some(round) = room_state.round_state.as_ref() else {
        return Ok(());
    };
    for player in &round.players {
        for skill_instance in &player.skill_loadout.equipped {
            let Some(definition) = registry.get(&skill_instance.skill_id) else {
                continue;
            };
            let ctx = RuleContext::new(room_state, player.seat, skill_instance);
            callback(&ctx, definition)?;
        }
    }
    Ok(())
}

fn dedup_projection(projection: &mut SkillProjection) {
    projection.visible_effects.sort_by(|left, right| {
        left.effect_id
            .cmp(&right.effect_id)
            .then(left.effect_type.cmp(&right.effect_type))
    });
    projection
        .visible_effects
        .dedup_by(|left, right| left.effect_id == right.effect_id);

    projection.private_knowledge.sort_by(|left, right| {
        left.viewer
            .cmp(&right.viewer)
            .then(left.target_seat.cmp(&right.target_seat))
            .then(left.source_skill.cmp(&right.source_skill))
            .then(left.tile_ids.cmp(&right.tile_ids))
    });
    projection.private_knowledge.dedup();
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use crate::core::event::GameEvent;
    use crate::core::state::{
        EffectState, PlayerRoundState, RoomState, RoundScoreTrackers, RoundState, RuleRuntimeState,
        SkillInstance, SkillLoadout, WallState,
    };

    use super::*;

    fn room_with_skills(skill_ids: &[&str]) -> RoomState {
        RoomState {
            table_code: "ROOM42".to_string(),
            phase: "playing".to_string(),
            mode: "normal".to_string(),
            test_mode: false,
            enforce_minimum_eight_fan: true,
            seats: Vec::new(),
            match_state: None,
            round_state: Some(RoundState {
                round_id: "round-1".to_string(),
                dealer_seat: 0,
                round_wind: "east".to_string(),
                current_actor: 0,
                phase: "playing".to_string(),
                wall: WallState {
                    tiles: Vec::new(),
                    head_index: 0,
                    tail_index: 0,
                },
                players: vec![PlayerRoundState {
                    seat: 0,
                    concealed_tiles: Vec::new(),
                    melds: Vec::new(),
                    flowers: Vec::new(),
                    discards: Vec::new(),
                    skill_loadout: SkillLoadout {
                        equipped: skill_ids
                            .iter()
                            .map(|skill_id| SkillInstance {
                                skill_id: (*skill_id).to_string(),
                                owner: 0,
                                level: 1,
                                cooldown: 0,
                                charges: 1,
                                config: json!({}),
                            })
                            .collect(),
                    },
                }],
                last_discard: None,
                pending_action: None,
                settlement: None,
                version: 1,
                score_trackers: RoundScoreTrackers::default(),
                last_action_context: Default::default(),
                rule_state: RuleRuntimeState {
                    enforce_minimum_eight_fan: true,
                },
                effect_state: EffectState::default(),
                restricted_discard_tile_key: None,
                skill_trackers: Default::default(),
            }),
            pending_timeout: None,
            continue_action: None,
        }
    }

    fn room_with_skill(skill_id: &str) -> RoomState {
        room_with_skills(&[skill_id])
    }

    struct TestSkill;

    impl RuleHook for TestSkill {}

    impl SkillDefinition for TestSkill {
        fn id(&self) -> &str {
            "test-skill"
        }

        fn name(&self) -> &'static str {
            "Test Skill"
        }

        fn activate(
            &self,
            ctx: &mut SkillContext<'_>,
            _target: Option<Seat>,
            _tile_ids: &[TileId],
        ) -> Result<Vec<GameEvent>, String> {
            Ok(vec![GameEvent::SkillActivated {
                seat: ctx.actor,
                skill_id: ctx.skill_instance.skill_id.clone(),
            }])
        }
    }

    struct ActiveOptionSkill;

    impl RuleHook for ActiveOptionSkill {
        fn activation(&self) -> SkillActivation {
            SkillActivation::ActiveTurn
        }
    }

    impl SkillDefinition for ActiveOptionSkill {
        fn id(&self) -> &str {
            "active-option"
        }

        fn name(&self) -> &'static str {
            "Active Option"
        }
    }

    struct ViewHookSkill;

    impl RuleHook for ViewHookSkill {
        fn build_view(
            &self,
            ctx: &RuleContext<'_>,
            local_seat: Seat,
            projection: &mut SkillProjection,
        ) -> Result<(), String> {
            if ctx.actor != local_seat {
                return Ok(());
            }
            projection.visible_effects.push(EffectInstance {
                effect_id: "effect-1".to_string(),
                effect_type: "test-effect".to_string(),
                owner: local_seat,
                target_seats: vec![local_seat],
                source_skill: Some(ctx.skill_instance.skill_id.clone()),
                remaining_turns: Some(1),
                stacks: 1,
                consumed: false,
                payload: json!({ "flag": true }),
            });
            projection.private_knowledge.push(KnowledgeEffect {
                viewer: local_seat,
                target_seat: Some(1),
                tile_ids: vec!["w1#0".to_string()],
                tile_keys: vec!["w1".to_string()],
                source_skill: Some(ctx.skill_instance.skill_id.clone()),
                description: Some("peek".to_string()),
            });
            Ok(())
        }
    }

    impl SkillDefinition for ViewHookSkill {
        fn id(&self) -> &str {
            "view-hook"
        }

        fn name(&self) -> &'static str {
            "View Hook"
        }
    }

    #[test]
    fn activate_skill_requires_equipped_skill() {
        let room = room_with_skill("other-skill");
        let error = activate_skill(&room, 0, "missing-skill", None, &[])
            .expect_err("missing equipped skill should be rejected");
        assert_eq!(error, "skill_not_equipped");
    }

    #[test]
    fn activate_skill_uses_registered_definition() {
        let room = room_with_skill("test-skill");
        let registry = StaticSkillRegistry::new().with_definition(Arc::new(TestSkill));

        let events =
            activate_skill_with_registry(&room, 0, "test-skill", None, &[], &registry).unwrap();

        assert!(matches!(
            events.as_slice(),
            [GameEvent::SkillActivated {
                seat: 0,
                skill_id
            }] if skill_id == "test-skill"
        ));
    }

    #[test]
    fn action_options_only_include_registered_active_skills() {
        let room = room_with_skills(&["active-option", "passive-skill"]);
        let registry = StaticSkillRegistry::new()
            .with_definition(Arc::new(ActiveOptionSkill))
            .with_definition(Arc::new(TestSkill));

        let options = skill_action_options_with_registry(&room, 0, &registry);

        assert_eq!(options, vec!["skill:active-option".to_string()]);
    }

    #[test]
    fn build_skill_projection_uses_registered_view_hooks() {
        let room = room_with_skill("view-hook");
        let registry = StaticSkillRegistry::new().with_definition(Arc::new(ViewHookSkill));

        let projection = build_skill_projection_with_registry(&room, 0, &registry);

        assert_eq!(projection.visible_effects.len(), 1);
        assert_eq!(projection.visible_effects[0].effect_type, "test-effect");
        assert_eq!(projection.private_knowledge.len(), 1);
        assert_eq!(
            projection.private_knowledge[0].tile_keys,
            vec!["w1".to_string()]
        );
    }
}
