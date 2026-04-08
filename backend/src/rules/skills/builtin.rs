use serde_json::json;

use crate::core::event::GameEvent;
use crate::core::ids::{Seat, TileId};
use crate::rules::scoring::{FanBreakdownEntry, FanResult};

use super::{
    EffectInstance, KnowledgeEffect, RuleContext, RuleHook, RuleOverride, ScoreHookRequest,
    SkillActivation, SkillContext, SkillDefinition, SkillProjection,
};

pub struct ScoreBoostSkill;

impl RuleHook for ScoreBoostSkill {
    fn activation(&self) -> SkillActivation {
        SkillActivation::ActiveTurn
    }

    fn build_view(
        &self,
        ctx: &RuleContext<'_>,
        local_seat: Seat,
        projection: &mut SkillProjection,
    ) -> Result<(), String> {
        let Some(round) = ctx.room_state.round_state.as_ref() else {
            return Ok(());
        };
        projection.visible_effects.extend(
            round
                .effect_state
                .ongoing
                .iter()
                .filter(|effect| {
                    effect.source_skill.as_deref() == Some(ctx.skill_instance.skill_id.as_str())
                        && (effect.owner == local_seat || effect.target_seats.contains(&local_seat))
                })
                .cloned(),
        );
        Ok(())
    }

    fn after_scoring(
        &self,
        ctx: &RuleContext<'_>,
        request: &ScoreHookRequest,
        result: &mut FanResult,
    ) -> Result<(), String> {
        let Some(round) = ctx.room_state.round_state.as_ref() else {
            return Ok(());
        };
        let applies_to_winner = request.evaluation.winner_seat == Some(ctx.actor);
        if !applies_to_winner {
            return Ok(());
        }
        for override_rule in &round.effect_state.rule_overrides {
            let from_this_skill =
                override_rule.source_skill.as_deref() == Some(ctx.skill_instance.skill_id.as_str());
            let owned_by_actor = override_rule.owner == ctx.actor;
            let targets_winner = override_rule.target_seat.is_none()
                || override_rule.target_seat == request.evaluation.winner_seat;
            if !from_this_skill || !owned_by_actor || !targets_winner {
                continue;
            }
            match override_rule.rule_key.as_str() {
                "bonus_fan" => {
                    let amount = override_rule
                        .payload
                        .get("amount")
                        .and_then(serde_json::Value::as_i64)
                        .unwrap_or(0);
                    if amount <= 0 {
                        continue;
                    }
                    result.fan_total += amount;
                    let fan_key = format!("skill_bonus:{}", ctx.skill_instance.skill_id);
                    result.fan_breakdown.push(FanBreakdownEntry {
                        fan_key: fan_key.clone(),
                        fan_value: amount,
                    });
                    result.fan_keys.push(fan_key);
                }
                "minimum_fan" => {
                    let amount = override_rule
                        .payload
                        .get("amount")
                        .and_then(serde_json::Value::as_i64)
                        .unwrap_or(0);
                    result.minimum_qualifying_fan_total =
                        result.minimum_qualifying_fan_total.max(amount);
                }
                _ => {}
            }
        }

        crate::rules::scoring::evaluator::recompute_score_delta(
            result,
            &request.evaluation.win_type,
            request.evaluation.winner_seat,
            request.evaluation.discarder_seat,
            request.evaluation.seat_count,
        );
        Ok(())
    }
}

impl SkillDefinition for ScoreBoostSkill {
    fn id(&self) -> &str {
        "score_boost"
    }

    fn name(&self) -> &'static str {
        "Score Boost"
    }

    fn activate(
        &self,
        ctx: &mut SkillContext<'_>,
        _target: Option<Seat>,
        _tile_ids: &[TileId],
    ) -> Result<Vec<GameEvent>, String> {
        let amount = ctx
            .skill_instance
            .config
            .get("amount")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(1);
        let effect_id = format!(
            "score_boost:{}:{}:{}",
            ctx.actor,
            ctx.skill_instance.skill_id,
            ctx.room_state
                .round_state
                .as_ref()
                .map(|round| round.version)
                .unwrap_or(0)
        );
        Ok(vec![
            GameEvent::SkillActivated {
                seat: ctx.actor,
                skill_id: ctx.skill_instance.skill_id.clone(),
            },
            GameEvent::EffectApplied {
                effect: EffectInstance {
                    effect_id,
                    effect_type: "score_boost".to_string(),
                    owner: ctx.actor,
                    target_seats: vec![ctx.actor],
                    source_skill: Some(ctx.skill_instance.skill_id.clone()),
                    remaining_turns: Some(1),
                    stacks: 1,
                    consumed: false,
                    payload: json!({ "amount": amount }),
                },
            },
            GameEvent::RuleOverrideApplied {
                override_rule: RuleOverride {
                    owner: ctx.actor,
                    target_seat: Some(ctx.actor),
                    rule_key: "bonus_fan".to_string(),
                    source_skill: Some(ctx.skill_instance.skill_id.clone()),
                    payload: json!({ "amount": amount }),
                },
            },
        ])
    }
}

pub struct PeekOpponentTileSkill;

impl RuleHook for PeekOpponentTileSkill {
    fn activation(&self) -> SkillActivation {
        SkillActivation::ActiveTurn
    }

    fn build_view(
        &self,
        ctx: &RuleContext<'_>,
        local_seat: Seat,
        projection: &mut SkillProjection,
    ) -> Result<(), String> {
        let Some(round) = ctx.room_state.round_state.as_ref() else {
            return Ok(());
        };
        projection.visible_effects.extend(
            round
                .effect_state
                .ongoing
                .iter()
                .filter(|effect| {
                    effect.source_skill.as_deref() == Some(ctx.skill_instance.skill_id.as_str())
                        && (effect.owner == local_seat || effect.target_seats.contains(&local_seat))
                })
                .cloned(),
        );
        projection.private_knowledge.extend(
            round
                .effect_state
                .hidden_knowledge
                .iter()
                .filter(|knowledge| {
                    knowledge.viewer == local_seat
                        && knowledge.source_skill.as_deref()
                            == Some(ctx.skill_instance.skill_id.as_str())
                })
                .cloned(),
        );
        Ok(())
    }
}

impl SkillDefinition for PeekOpponentTileSkill {
    fn id(&self) -> &str {
        "peek_opponent_tile"
    }

    fn name(&self) -> &'static str {
        "Peek Opponent Tile"
    }

    fn activate(
        &self,
        ctx: &mut SkillContext<'_>,
        target: Option<Seat>,
        _tile_ids: &[TileId],
    ) -> Result<Vec<GameEvent>, String> {
        let target = target.ok_or_else(|| "skill_requires_target".to_string())?;
        if target == ctx.actor {
            return Err("invalid_skill_target".to_string());
        }
        let target_player = ctx
            .room_state
            .round_state
            .as_ref()
            .and_then(|round| round.players.iter().find(|player| player.seat == target))
            .ok_or_else(|| "invalid_skill_target".to_string())?;
        let revealed_tile = target_player
            .concealed_tiles
            .first()
            .ok_or_else(|| "invalid_skill_target".to_string())?;
        let effect_id = format!(
            "peek_opponent_tile:{}:{}:{}",
            ctx.actor,
            target,
            ctx.room_state
                .round_state
                .as_ref()
                .map(|round| round.version)
                .unwrap_or(0)
        );
        Ok(vec![
            GameEvent::SkillActivated {
                seat: ctx.actor,
                skill_id: ctx.skill_instance.skill_id.clone(),
            },
            GameEvent::EffectApplied {
                effect: EffectInstance {
                    effect_id,
                    effect_type: "peek_opponent_tile".to_string(),
                    owner: ctx.actor,
                    target_seats: vec![ctx.actor],
                    source_skill: Some(ctx.skill_instance.skill_id.clone()),
                    remaining_turns: Some(1),
                    stacks: 1,
                    consumed: false,
                    payload: json!({
                        "target_seat": target,
                        "tile_id": revealed_tile.tile_id,
                        "tile_key": revealed_tile.tile_key,
                    }),
                },
            },
            GameEvent::ViewKnowledgeGranted {
                seat: ctx.actor,
                knowledge: KnowledgeEffect {
                    viewer: ctx.actor,
                    target_seat: Some(target),
                    tile_ids: vec![revealed_tile.tile_id.clone()],
                    tile_keys: vec![revealed_tile.tile_key.clone()],
                    source_skill: Some(ctx.skill_instance.skill_id.clone()),
                    description: Some(format!("Peeked seat {target}")),
                },
            },
        ])
    }
}
