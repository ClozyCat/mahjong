mod builtin;
pub mod effects;
pub mod hooks;
pub mod instances;
pub mod registry;
mod strategems;

use std::sync::OnceLock;

use serde_json::{Value, json};

use crate::core::event::GameEvent;
use crate::core::ids::{Seat, SkillId, TileId};
use crate::core::state::RoomState;

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
    settlement: &mut Value,
) -> Result<(), String> {
    apply_draw_settlement_hooks_with_registry(room_state, settlement, default_registry())
}

pub fn apply_draw_settlement_hooks_with_registry(
    room_state: &RoomState,
    settlement: &mut Value,
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
    apply_events_to_legacy_room(room, events)
}

pub fn apply_passive_skill_events_to_legacy_room(
    room: &mut Value,
    events: &[GameEvent],
) -> Result<Vec<Value>, String> {
    apply_events_to_legacy_room(room, events)
}

pub fn sync_match_skill_trackers_after_settlement(room: &mut Value) {
    let winner_seat = room
        .get("round_state")
        .and_then(|round| round.get("settlement"))
        .and_then(|settlement| settlement.get("winner_seat"))
        .and_then(Value::as_u64)
        .map(|value| value as usize);

    let seat_count = room
        .get("seats")
        .and_then(Value::as_array)
        .map(|seats| seats.len())
        .unwrap_or(4);

    let Some(trackers) = ensure_match_skill_trackers(room) else {
        return;
    };

    let streaks = trackers
        .entry("lian_huan_ji".to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .and_then(|object| {
            Some(
                object
                    .entry("streaks".to_string())
                    .or_insert_with(|| json!({})),
            )
        })
        .and_then(Value::as_object_mut);
    if let Some(streaks) = streaks {
        match winner_seat {
            Some(winner) => {
                for seat in 0..seat_count {
                    let key = seat.to_string();
                    let next = if seat == winner {
                        streaks.get(&key).and_then(Value::as_i64).unwrap_or(0) + 1
                    } else {
                        0
                    };
                    streaks.insert(key, json!(next));
                }
            }
            None => {
                for seat in 0..seat_count {
                    streaks.insert(seat.to_string(), json!(0));
                }
            }
        }
    }

    if let Some(winner) = winner_seat {
        let pending = trackers
            .get_mut("zou_wei_shang_ji")
            .and_then(Value::as_object_mut)
            .and_then(|object| object.get_mut("pending_win_penalty"))
            .and_then(Value::as_object_mut);
        if let Some(pending) = pending {
            pending.remove(&winner.to_string());
        }
    }
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
                ensure_effect_state(room)?
                    .get_mut("ongoing")
                    .and_then(Value::as_array_mut)
                    .ok_or_else(|| "invalid_action".to_string())?
                    .push(serde_json::to_value(effect).map_err(|_| "invalid_action".to_string())?);
            }
            GameEvent::EffectExpired { effect_id } => {
                if let Some(ongoing) = ensure_effect_state(room)?
                    .get_mut("ongoing")
                    .and_then(Value::as_array_mut)
                {
                    ongoing.retain(|effect| {
                        effect.get("effect_id").and_then(Value::as_str) != Some(effect_id.as_str())
                    });
                }
            }
            GameEvent::ViewKnowledgeGranted { knowledge, .. } => {
                ensure_effect_state(room)?
                    .get_mut("hidden_knowledge")
                    .and_then(Value::as_array_mut)
                    .ok_or_else(|| "invalid_action".to_string())?
                    .push(
                        serde_json::to_value(knowledge)
                            .map_err(|_| "invalid_action".to_string())?,
                    );
            }
            GameEvent::RuleOverrideApplied { override_rule } => {
                ensure_effect_state(room)?
                    .get_mut("rule_overrides")
                    .and_then(Value::as_array_mut)
                    .ok_or_else(|| "invalid_action".to_string())?
                    .push(
                        serde_json::to_value(override_rule)
                            .map_err(|_| "invalid_action".to_string())?,
                    );
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

fn ensure_effect_state(room: &mut Value) -> Result<&mut serde_json::Map<String, Value>, String> {
    let round_state = room
        .get_mut("round_state")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "invalid_action".to_string())?;
    let effect_state = round_state.entry("effect_state").or_insert_with(|| {
        json!({
            "ongoing": [],
            "hidden_knowledge": [],
            "rule_overrides": [],
        })
    });
    let effect_state = effect_state
        .as_object_mut()
        .ok_or_else(|| "invalid_action".to_string())?;
    effect_state
        .entry("ongoing".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    effect_state
        .entry("hidden_knowledge".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    effect_state
        .entry("rule_overrides".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    Ok(effect_state)
}

fn decrement_skill_charge(room: &mut Value, actor: Seat, skill_id: &str) -> Result<(), String> {
    let equipped = room
        .get_mut("round_state")
        .and_then(Value::as_object_mut)
        .and_then(|round| round.get_mut("players"))
        .and_then(Value::as_array_mut)
        .and_then(|players| players.get_mut(actor))
        .and_then(Value::as_object_mut)
        .map(|player| {
            player
                .entry("skill_loadout".to_string())
                .or_insert_with(|| json!({ "equipped": [] }))
        })
        .and_then(Value::as_object_mut)
        .map(|loadout| {
            loadout
                .entry("equipped".to_string())
                .or_insert_with(|| Value::Array(Vec::new()))
        })
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "invalid_action".to_string())?;

    let skill = equipped
        .iter_mut()
        .find(|skill| skill.get("skill_id").and_then(Value::as_str) == Some(skill_id))
        .ok_or_else(|| "skill_not_equipped".to_string())?;
    let charges = skill.get("charges").and_then(Value::as_u64).unwrap_or(0);
    if charges == 0 {
        return Err("skill_no_charges".to_string());
    }
    let object = skill
        .as_object_mut()
        .ok_or_else(|| "invalid_action".to_string())?;
    object.insert("charges".to_string(), json!(charges - 1));
    Ok(())
}

fn increment_round_version(room: &mut Value) -> Result<(), String> {
    let round_state = room
        .get_mut("round_state")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "invalid_action".to_string())?;
    let version = round_state
        .get("version")
        .and_then(Value::as_u64)
        .unwrap_or_default()
        + 1;
    round_state.insert("version".to_string(), json!(version));
    Ok(())
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
    let replacement_tile = event
        .get("replacement_tile")
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;
    let replacement_tile_id = replacement_tile
        .get("tile_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let round_state = room
        .get_mut("round_state")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "invalid_action".to_string())?;
    let players = round_state
        .get_mut("players")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "invalid_action".to_string())?;
    let player = players
        .get_mut(seat)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "invalid_action".to_string())?;
    let concealed_tiles = player
        .get_mut("concealed_tiles")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "invalid_action".to_string())?;
    let Some(tile_index) = concealed_tiles
        .iter()
        .position(|tile| tile.get("tile_id").and_then(Value::as_str) == Some(removed_tile_id))
    else {
        return Err("invalid_action".to_string());
    };
    concealed_tiles[tile_index] = replacement_tile.clone();

    let wall = round_state
        .get_mut("wall")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "invalid_action".to_string())?;
    let head_index = wall
        .get("head_index")
        .and_then(Value::as_u64)
        .unwrap_or_default() as usize;
    let tail_index = wall
        .get("tail_index")
        .and_then(Value::as_u64)
        .unwrap_or_default() as usize;
    wall.insert(
        "head_index".to_string(),
        json!(head_index.saturating_add(1)),
    );
    round_state.insert(
        "last_action_context".to_string(),
        json!({
            "kind": "draw",
            "seat": seat,
            "tile_id": replacement_tile_id,
            "from_kong_replacement": false,
            "was_last_live_tile": head_index >= tail_index,
            "was_last_discard": false,
        }),
    );
    if room
        .get("pending_timeout")
        .and_then(|timeout| timeout.get("kind"))
        .and_then(Value::as_str)
        == Some("active_turn")
        && room
            .get("pending_timeout")
            .and_then(|timeout| timeout.get("seat_index"))
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            == Some(seat)
    {
        if let Some(timeout) = room
            .get_mut("pending_timeout")
            .and_then(Value::as_object_mut)
        {
            timeout.insert("drawn_tile_id".to_string(), json!(replacement_tile_id));
        }
    }

    emitted_messages.push(round_event_message(
        "skill_tile_replaced",
        json!({
            "type": "skill_tile_replaced",
            "seat": seat,
            "removed_tile_id": removed_tile_id,
            "replacement_tile_id": replacement_tile.get("tile_id").cloned().unwrap_or(Value::Null),
            "replacement_tile_key": replacement_tile.get("tile_key").cloned().unwrap_or(Value::Null),
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

    let round_state = room
        .get_mut("round_state")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "invalid_action".to_string())?;
    let version = round_state
        .get("version")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let players = round_state
        .get_mut("players")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "invalid_action".to_string())?;
    let player = players
        .get_mut(seat)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "invalid_action".to_string())?;
    let melds = player
        .get_mut("melds")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "invalid_action".to_string())?;
    if meld_index >= melds.len() {
        return Err("invalid_action".to_string());
    }
    melds.remove(meld_index);
    let concealed_tiles = player
        .get_mut("concealed_tiles")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "invalid_action".to_string())?;
    for (offset, tile_key) in tile_keys.iter().enumerate() {
        let Some(tile_key) = tile_key.as_str() else {
            continue;
        };
        concealed_tiles.push(json!({
            "tile_id": format!("{tile_key}#reclaim:{seat}:{version}:{offset}"),
            "tile_key": tile_key,
            "kind": "unknown",
            "suit": Value::Null,
            "rank": Value::Null,
            "name": Value::Null,
        }));
    }
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
    if let Some(settlement) = room
        .get_mut("round_state")
        .and_then(|round| round.get_mut("settlement"))
        .and_then(Value::as_object_mut)
    {
        settlement.insert("draw_type".to_string(), json!("skill_forced"));
        if let Some(score_delta) = settlement
            .get_mut("score_delta")
            .and_then(Value::as_object_mut)
        {
            adjust_score_map_value(score_delta.get_mut("total_delta_by_seat"), seat, -penalty);
            adjust_score_map_value(score_delta.get_mut("fan_delta_by_seat"), seat, -penalty);
        }
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

fn adjust_score_map_value(target: Option<&mut Value>, seat: Seat, delta: i64) {
    let Some(Value::Object(map)) = target else {
        return;
    };
    let key = seat.to_string();
    let current = map.get(&key).and_then(Value::as_i64).unwrap_or(0);
    map.insert(key, json!(current + delta));
}

fn adjust_match_cumulative_score(room: &mut Value, seat: Seat, delta: i64) {
    let Some(scores) = room
        .get_mut("match_state")
        .and_then(|state| state.get_mut("cumulative_scores"))
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let key = seat.to_string();
    let current = scores.get(&key).and_then(Value::as_i64).unwrap_or(0);
    scores.insert(key, json!(current + delta));
}

fn ensure_match_skill_trackers(room: &mut Value) -> Option<&mut serde_json::Map<String, Value>> {
    room.get_mut("match_state")
        .and_then(Value::as_object_mut)
        .map(|state| {
            state
                .entry("skill_trackers".to_string())
                .or_insert_with(|| json!({}))
        })
        .and_then(Value::as_object_mut)
}

fn set_pending_next_round_win_penalty(room: &mut Value, seat: Seat, penalty: i64) {
    let Some(trackers) = ensure_match_skill_trackers(room) else {
        return;
    };
    let pending = trackers
        .entry("zou_wei_shang_ji".to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .and_then(|object| {
            Some(
                object
                    .entry("pending_win_penalty".to_string())
                    .or_insert_with(|| json!({})),
            )
        })
        .and_then(Value::as_object_mut);
    let Some(pending) = pending else {
        return;
    };
    pending.insert(seat.to_string(), json!(penalty));
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
