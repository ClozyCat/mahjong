use std::sync::Arc;

use rand::seq::SliceRandom;
use serde_json::{Value, json};

use crate::core::event::GameEvent;
use crate::core::ids::{Seat, TileId};
use crate::core::state::{EffectInstance, KnowledgeEffect, RoundSettlement};
use crate::core::tile::Tile;
use crate::rules::scoring::FanResult;

use super::catalog::{value_i64_for_skill, value_usize_for_skill};
use super::{
    RuleContext, RuleHook, ScoreHookRequest, SkillActivation, SkillContext, SkillDefinition,
    SkillProjection,
};

const STRATAGEMS: [(&str, &str); 36] = [
    ("man_tian_guo_hai", "Mantian Guohai"),
    ("wei_wei_jiu_zhao", "Wei Wei Jiu Zhao"),
    ("jie_dao_sha_ren", "Jie Dao Sha Ren"),
    ("yi_yi_dai_lao", "Yi Yi Dai Lao"),
    ("chen_huo_da_jie", "Chen Huo Da Jie"),
    ("sheng_dong_ji_xi", "Sheng Dong Ji Xi"),
    ("wu_zhong_sheng_you", "Wu Zhong Sheng You"),
    ("an_du_chen_cang", "An Du Chen Cang"),
    ("ge_an_guan_huo", "Ge An Guan Huo"),
    ("xiao_li_cang_dao", "Xiao Li Cang Dao"),
    ("li_dai_tao_jiang", "Li Dai Tao Jiang"),
    ("shun_shou_qian_yang", "Shun Shou Qian Yang"),
    ("da_cao_jing_she", "Da Cao Jing She"),
    ("jie_shi_huan_hun", "Jie Shi Huan Hun"),
    ("diao_hu_li_shan", "Diao Hu Li Shan"),
    ("yu_qin_gu_zong", "Yu Qin Gu Zong"),
    ("pao_zhuan_yin_yu", "Pao Zhuan Yin Yu"),
    ("qin_zei_qin_wang", "Qin Zei Qin Wang"),
    ("fu_di_chou_xin", "Fu Di Chou Xin"),
    ("hun_shui_mo_yu", "Hun Shui Mo Yu"),
    ("jin_chan_tuo_qiao", "Jin Chan Tuo Qiao"),
    ("guan_men_zhuo_zei", "Guan Men Zhuo Zei"),
    ("yuan_jiao_jin_gong", "Yuan Jiao Jin Gong"),
    ("jia_dao_fa_guo", "Jia Dao Fa Guo"),
    ("tou_liang_huan_zhu", "Tou Liang Huan Zhu"),
    ("zhi_sang_ma_huai", "Zhi Sang Ma Huai"),
    ("jia_chi_bu_dian", "Jia Chi Bu Dian"),
    ("shang_wu_chou_ti", "Shang Wu Chou Ti"),
    ("shu_shang_kai_hua", "Shu Shang Kai Hua"),
    ("fan_ke_wei_zhu", "Fan Ke Wei Zhu"),
    ("mei_ren_ji", "Mei Ren Ji"),
    ("kong_cheng_ji", "Kong Cheng Ji"),
    ("fan_jian_ji", "Fan Jian Ji"),
    ("ku_rou_ji", "Ku Rou Ji"),
    ("lian_huan_ji", "Lian Huan Ji"),
    ("zou_wei_shang_ji", "Zou Wei Shang Ji"),
];

pub fn definitions() -> Vec<Arc<dyn SkillDefinition>> {
    STRATAGEMS
        .iter()
        .map(|(id, name)| Arc::new(StratagemSkill { id, name }) as Arc<dyn SkillDefinition>)
        .collect()
}

struct StratagemSkill {
    id: &'static str,
    name: &'static str,
}

impl RuleHook for StratagemSkill {
    fn activation(&self) -> SkillActivation {
        match self.id {
            "sheng_dong_ji_xi" | "wu_zhong_sheng_you" | "an_du_chen_cang" | "jin_chan_tuo_qiao"
            | "tou_liang_huan_zhu" | "zou_wei_shang_ji" => SkillActivation::ActiveTurn,
            _ => SkillActivation::Passive,
        }
    }

    fn can_activate(
        &self,
        ctx: &RuleContext<'_>,
        target: Option<Seat>,
        tile_ids: &[TileId],
    ) -> Result<(), String> {
        match self.id {
            "sheng_dong_ji_xi" | "zou_wei_shang_ji" => Ok(()),
            "jin_chan_tuo_qiao" => {
                if active_effects(ctx, "jin_chan_tuo_qiao_guard").is_empty() {
                    Ok(())
                } else {
                    Err("invalid_action".to_string())
                }
            }
            "wu_zhong_sheng_you" => {
                if tile_ids.len() != 1 {
                    return Err("invalid_action".to_string());
                }
                let Some(player) = round_player(ctx, ctx.actor) else {
                    return Err("invalid_action".to_string());
                };
                if player
                    .concealed_tiles
                    .iter()
                    .all(|tile| tile.tile_id != tile_ids[0])
                {
                    return Err("invalid_action".to_string());
                }
                Ok(())
            }
            "an_du_chen_cang" => {
                let target = target.ok_or_else(|| "skill_requires_target".to_string())?;
                if target == ctx.actor || round_player(ctx, target).is_none() {
                    return Err("invalid_skill_target".to_string());
                }
                Ok(())
            }
            "tou_liang_huan_zhu" => {
                let Some(player) = round_player(ctx, ctx.actor) else {
                    return Err("invalid_action".to_string());
                };
                if player.melds.is_empty() {
                    return Err("invalid_action".to_string());
                }
                if let Some(meld_index) = parse_meld_index(tile_ids) {
                    if meld_index >= player.melds.len() {
                        return Err("invalid_action".to_string());
                    }
                } else if player.melds.len() > 1 {
                    return Err("skill_requires_target".to_string());
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn on_decline_hu(&self, ctx: &RuleContext<'_>) -> Result<Vec<GameEvent>, String> {
        match self.id {
            "yu_qin_gu_zong" => Ok(vec![decline_hu_window_effect(
                ctx,
                "yu_qin_gu_zong_window",
                6,
            )]),
            _ => Ok(Vec::new()),
        }
    }

    fn before_scoring(
        &self,
        ctx: &RuleContext<'_>,
        request: &mut ScoreHookRequest,
    ) -> Result<(), String> {
        if request.evaluation.winner_seat != Some(ctx.actor) {
            return Ok(());
        }
        match self.id {
            "sheng_dong_ji_xi" => {
                if !active_effects(ctx, "sheng_dong_ji_xi_preview").is_empty() {
                    request.required_minimum_fan_total +=
                        preview_minimum_penalty(ctx.skill_instance.level);
                }
            }
            "jia_chi_bu_dian" => {
                request.required_minimum_fan_total = request
                    .required_minimum_fan_total
                    .min(minimum_fan_override(ctx.skill_instance.level));
            }
            _ => {}
        }
        Ok(())
    }

    fn build_view(
        &self,
        ctx: &RuleContext<'_>,
        local_seat: Seat,
        projection: &mut SkillProjection,
    ) -> Result<(), String> {
        match self.id {
            "sheng_dong_ji_xi" => extend_projection_from_effects(
                ctx,
                local_seat,
                "sheng_dong_ji_xi_preview",
                projection,
                true,
            ),
            "an_du_chen_cang" => extend_projection_from_effects(
                ctx,
                local_seat,
                "an_du_chen_cang_view",
                projection,
                true,
            ),
            "yu_qin_gu_zong" => extend_projection_from_effects(
                ctx,
                local_seat,
                "yu_qin_gu_zong_window",
                projection,
                false,
            ),
            "jin_chan_tuo_qiao" => extend_projection_from_effects(
                ctx,
                local_seat,
                "jin_chan_tuo_qiao_guard",
                projection,
                false,
            ),
            _ => {}
        }
        Ok(())
    }

    fn after_scoring(
        &self,
        ctx: &RuleContext<'_>,
        request: &ScoreHookRequest,
        result: &mut FanResult,
    ) -> Result<(), String> {
        apply_after_scoring(self.id, ctx, request, result);
        Ok(())
    }

    fn after_draw_settlement(
        &self,
        ctx: &RuleContext<'_>,
        settlement: &mut RoundSettlement,
    ) -> Result<(), String> {
        apply_after_draw(self.id, ctx, settlement);
        Ok(())
    }
}

impl SkillDefinition for StratagemSkill {
    fn id(&self) -> &str {
        self.id
    }

    fn name(&self) -> &'static str {
        self.name
    }

    fn activate(
        &self,
        ctx: &mut SkillContext<'_>,
        target: Option<Seat>,
        tile_ids: &[TileId],
    ) -> Result<Vec<GameEvent>, String> {
        match self.id {
            "sheng_dong_ji_xi" => activate_sheng_dong_ji_xi(ctx),
            "wu_zhong_sheng_you" => activate_wu_zhong_sheng_you(ctx, tile_ids),
            "an_du_chen_cang" => activate_an_du_chen_cang(ctx, target),
            "jin_chan_tuo_qiao" => activate_jin_chan_tuo_qiao(ctx),
            "tou_liang_huan_zhu" => activate_tou_liang_huan_zhu(ctx, tile_ids),
            "zou_wei_shang_ji" => activate_zou_wei_shang_ji(ctx),
            _ => Ok(vec![GameEvent::SkillActivated {
                seat: ctx.actor,
                skill_id: ctx.skill_instance.skill_id.clone(),
            }]),
        }
    }
}

fn gain_value(skill_id: &str, level: u8) -> i64 {
    value_i64_for_skill(skill_id, level, "gain")
}

fn loss_value(skill_id: &str, level: u8) -> i64 {
    value_i64_for_skill(skill_id, level, "loss")
}

fn score_penalty_value(skill_id: &str, level: u8) -> i64 {
    value_i64_for_skill(skill_id, level, "score_penalty")
}

fn minimum_fan_override(level: u8) -> i64 {
    value_i64_for_skill("jia_chi_bu_dian", level, "minimum_fan_override")
}

fn preview_count(level: u8) -> usize {
    value_usize_for_skill("sheng_dong_ji_xi", level, "preview_count")
}

fn an_du_preview_count(level: u8) -> usize {
    value_usize_for_skill("an_du_chen_cang", level, "preview_count")
}

fn preview_minimum_penalty(level: u8) -> i64 {
    value_i64_for_skill("sheng_dong_ji_xi", level, "minimum_fan_penalty")
}

fn active_effects<'a>(ctx: &'a RuleContext<'_>, effect_type: &'a str) -> Vec<&'a EffectInstance> {
    ctx.room_state
        .round_state
        .as_ref()
        .map(|round| {
            round
                .effect_state
                .ongoing
                .iter()
                .filter(|effect| {
                    effect.effect_type == effect_type
                        && effect.owner == ctx.actor
                        && effect.source_skill.as_deref()
                            == Some(ctx.skill_instance.skill_id.as_str())
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn extend_projection_from_effects(
    ctx: &RuleContext<'_>,
    local_seat: Seat,
    effect_type: &str,
    projection: &mut SkillProjection,
    include_knowledge: bool,
) {
    let Some(round) = ctx.room_state.round_state.as_ref() else {
        return;
    };
    projection.visible_effects.extend(
        round
            .effect_state
            .ongoing
            .iter()
            .filter(|effect| {
                effect.effect_type == effect_type
                    && effect.source_skill.as_deref() == Some(ctx.skill_instance.skill_id.as_str())
                    && (effect.owner == local_seat || effect.target_seats.contains(&local_seat))
            })
            .cloned(),
    );
    if include_knowledge {
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
    }
}

fn seat_delta(result: &FanResult, seat: Seat) -> i64 {
    result
        .score_delta
        .total_delta_by_seat
        .get(seat)
        .copied()
        .unwrap_or(0)
}

fn adjust_score_delta(result: &mut FanResult, seat: Seat, delta: i64) {
    if delta == 0 {
        return;
    }
    if let Some(value) = result.score_delta.total_delta_by_seat.get_mut(seat) {
        *value += delta;
    }
    if let Some(value) = result.score_delta.fan_delta_by_seat.get_mut(seat) {
        *value += delta;
    }
}

fn live_tiles_remaining(ctx: &RuleContext<'_>) -> usize {
    ctx.room_state
        .round_state
        .as_ref()
        .map(|round| round.skill_trackers.live_tiles_remaining.max(0) as usize)
        .filter(|value| *value > 0)
        .or_else(|| {
            ctx.room_state
                .round_state
                .as_ref()
                .map(|round| round.wall.live_tiles_remaining())
        })
        .unwrap_or(0)
}

fn concealed_tile_count(request: &ScoreHookRequest) -> usize {
    request.evaluation.concealed_tile_keys.len()
}

fn open_meld_count(request: &ScoreHookRequest) -> usize {
    request.evaluation.open_meld_tile_key_groups.len()
}

fn has_any_fan(result: &FanResult, fan_key: &str) -> bool {
    result.fan_keys.iter().any(|entry| entry == fan_key)
}

fn is_terminal_or_honour(tile_key: &str) -> bool {
    match tile_key.as_bytes() {
        [b'w' | b't' | b'b', b'1' | b'9'] => true,
        [b'w' | b't' | b'b', ..] => false,
        _ => true,
    }
}

fn is_honour_tile(tile_key: &str) -> bool {
    !matches!(tile_key.as_bytes(), [b'w' | b't' | b'b', ..])
}

fn tile_is_five(tile_key: &str) -> bool {
    matches!(tile_key.as_bytes(), [b'w' | b't' | b'b', b'5'])
}

fn same_seat_distance(from: Seat, to: Seat) -> usize {
    (to + 4 - from) % 4
}

fn current_round_version(ctx: &RuleContext<'_>) -> u64 {
    ctx.room_state
        .round_state
        .as_ref()
        .map(|round| round.version)
        .unwrap_or(0)
}

fn tracker_bool_for_seat(ctx: &RuleContext<'_>, key: &str, seat: Seat) -> bool {
    let Some(round) = ctx.room_state.round_state.as_ref() else {
        return false;
    };
    match key {
        "discarded_five_by_seat" => round
            .skill_trackers
            .discarded_five_by_seat
            .get(&seat)
            .copied()
            .unwrap_or(false),
        "honor_redraw_success_by_seat" => round
            .skill_trackers
            .honor_redraw_success_by_seat
            .get(&seat)
            .copied()
            .unwrap_or(false),
        _ => false,
    }
}

fn tracker_i64_for_seat(ctx: &RuleContext<'_>, key: &str, seat: Seat) -> i64 {
    let Some(round) = ctx.room_state.round_state.as_ref() else {
        return 0;
    };
    match key {
        "claimed_discard_counts_by_seat" => round
            .skill_trackers
            .claimed_discard_counts_by_seat
            .get(&seat)
            .copied()
            .unwrap_or(0),
        _ => 0,
    }
}

fn tracker_discard_count(ctx: &RuleContext<'_>, tile_key: &str) -> i64 {
    ctx.room_state
        .round_state
        .as_ref()
        .and_then(|round| round.skill_trackers.discard_counts.get(tile_key))
        .copied()
        .unwrap_or(0)
}

fn round_player<'a>(
    ctx: &'a RuleContext<'_>,
    seat: Seat,
) -> Option<&'a crate::core::state::PlayerRoundState> {
    ctx.room_state
        .round_state
        .as_ref()
        .and_then(|round| round.players.iter().find(|player| player.seat == seat))
}

fn tail_preview_tiles(ctx: &RuleContext<'_>, count: usize) -> Vec<Tile> {
    let Some(round) = ctx.room_state.round_state.as_ref() else {
        return Vec::new();
    };
    let start = round.wall.head_index.min(round.wall.tail_index);
    let mut tiles = Vec::new();
    let mut index = round.wall.tail_index;
    while index >= start && tiles.len() < count {
        if let Some(tile) = round.wall.tiles.get(index).cloned() {
            tiles.push(tile);
        }
        if index == 0 {
            break;
        }
        index -= 1;
    }
    tiles
}

fn parse_meld_index(tile_ids: &[TileId]) -> Option<usize> {
    tile_ids
        .first()
        .and_then(|value| value.strip_prefix("meld:"))
        .and_then(|value| value.parse::<usize>().ok())
}

fn decline_hu_window_effect(
    ctx: &RuleContext<'_>,
    effect_type: &str,
    version_window: u64,
) -> GameEvent {
    GameEvent::EffectApplied {
        effect: EffectInstance {
            effect_id: format!("{effect_type}:{}:{}", ctx.actor, current_round_version(ctx)),
            effect_type: effect_type.to_string(),
            owner: ctx.actor,
            target_seats: vec![ctx.actor],
            source_skill: Some(ctx.skill_instance.skill_id.clone()),
            remaining_turns: Some(3),
            stacks: 1,
            consumed: false,
            payload: json!({
                "trigger_version": current_round_version(ctx),
                "expires_version": current_round_version(ctx) + version_window,
            }),
        },
    }
}

fn activate_sheng_dong_ji_xi(ctx: &mut SkillContext<'_>) -> Result<Vec<GameEvent>, String> {
    let preview_tiles = tail_preview_tiles(ctx, preview_count(ctx.skill_instance.level));
    if preview_tiles.is_empty() {
        return Err("round_not_ready".to_string());
    }
    let effect_id = format!(
        "sheng_dong_ji_xi:{}:{}",
        ctx.actor,
        current_round_version(ctx)
    );
    Ok(vec![
        GameEvent::SkillActivated {
            seat: ctx.actor,
            skill_id: ctx.skill_instance.skill_id.clone(),
        },
        GameEvent::EffectApplied {
            effect: EffectInstance {
                effect_id,
                effect_type: "sheng_dong_ji_xi_preview".to_string(),
                owner: ctx.actor,
                target_seats: vec![ctx.actor],
                source_skill: Some(ctx.skill_instance.skill_id.clone()),
                remaining_turns: Some(1),
                stacks: 1,
                consumed: false,
                payload: json!({
                    "preview_count": preview_tiles.len(),
                    "minimum_fan_penalty": preview_minimum_penalty(ctx.skill_instance.level),
                }),
            },
        },
        GameEvent::ViewKnowledgeGranted {
            seat: ctx.actor,
            knowledge: KnowledgeEffect {
                viewer: ctx.actor,
                target_seat: None,
                tile_ids: preview_tiles
                    .iter()
                    .map(|tile| tile.tile_id.clone())
                    .collect(),
                tile_keys: preview_tiles
                    .iter()
                    .map(|tile| tile.tile_key.clone())
                    .collect(),
                source_skill: Some(ctx.skill_instance.skill_id.clone()),
                description: Some("tail_preview".to_string()),
            },
        },
    ])
}

fn activate_wu_zhong_sheng_you(
    ctx: &mut SkillContext<'_>,
    tile_ids: &[TileId],
) -> Result<Vec<GameEvent>, String> {
    let removed_tile_id = tile_ids
        .first()
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;
    let replacement_tile = ctx
        .room_state
        .round_state
        .as_ref()
        .and_then(|round| round.wall.tiles.get(round.wall.head_index))
        .cloned()
        .ok_or_else(|| "round_not_ready".to_string())?;
    Ok(vec![
        GameEvent::SkillActivated {
            seat: ctx.actor,
            skill_id: ctx.skill_instance.skill_id.clone(),
        },
        GameEvent::SkillTileReplaced {
            seat: ctx.actor,
            removed_tile_id: removed_tile_id,
            replacement_tile: replacement_tile.clone(),
        },
        GameEvent::SkillScoreAdjusted {
            seat: ctx.actor,
            delta: -score_penalty_value("wu_zhong_sheng_you", ctx.skill_instance.level),
            reason: Some("wu_zhong_sheng_you".to_string()),
        },
    ])
}

fn activate_an_du_chen_cang(
    ctx: &mut SkillContext<'_>,
    target: Option<Seat>,
) -> Result<Vec<GameEvent>, String> {
    let target = target.ok_or_else(|| "skill_requires_target".to_string())?;
    let target_player =
        round_player(ctx, target).ok_or_else(|| "invalid_skill_target".to_string())?;
    let mut preview_tiles = target_player.concealed_tiles.clone();
    let mut rng = rand::rng();
    preview_tiles.shuffle(&mut rng);
    preview_tiles.truncate(an_du_preview_count(ctx.skill_instance.level));
    let effect_id = format!(
        "an_du_chen_cang:{}:{}:{}",
        ctx.actor,
        target,
        current_round_version(ctx)
    );
    Ok(vec![
        GameEvent::SkillActivated {
            seat: ctx.actor,
            skill_id: ctx.skill_instance.skill_id.clone(),
        },
        GameEvent::EffectApplied {
            effect: EffectInstance {
                effect_id,
                effect_type: "an_du_chen_cang_view".to_string(),
                owner: ctx.actor,
                target_seats: vec![ctx.actor],
                source_skill: Some(ctx.skill_instance.skill_id.clone()),
                remaining_turns: Some(1),
                stacks: 1,
                consumed: false,
                payload: json!({
                    "target_seat": target,
                    "preview_count": preview_tiles.len(),
                    "score_penalty": score_penalty_value("an_du_chen_cang", ctx.skill_instance.level),
                }),
            },
        },
        GameEvent::SkillScoreAdjusted {
            seat: ctx.actor,
            delta: -score_penalty_value("an_du_chen_cang", ctx.skill_instance.level),
            reason: Some("an_du_chen_cang".to_string()),
        },
        GameEvent::ViewKnowledgeGranted {
            seat: ctx.actor,
            knowledge: KnowledgeEffect {
                viewer: ctx.actor,
                target_seat: Some(target),
                tile_ids: preview_tiles
                    .iter()
                    .map(|tile| tile.tile_id.clone())
                    .collect(),
                tile_keys: preview_tiles
                    .iter()
                    .map(|tile| tile.tile_key.clone())
                    .collect(),
                source_skill: Some(ctx.skill_instance.skill_id.clone()),
                description: Some("partial_hand_preview".to_string()),
            },
        },
    ])
}

fn activate_jin_chan_tuo_qiao(ctx: &mut SkillContext<'_>) -> Result<Vec<GameEvent>, String> {
    let effect_id = format!(
        "jin_chan_tuo_qiao:{}:{}",
        ctx.actor,
        current_round_version(ctx)
    );
    Ok(vec![
        GameEvent::SkillActivated {
            seat: ctx.actor,
            skill_id: ctx.skill_instance.skill_id.clone(),
        },
        GameEvent::EffectApplied {
            effect: EffectInstance {
                effect_id,
                effect_type: "jin_chan_tuo_qiao_guard".to_string(),
                owner: ctx.actor,
                target_seats: vec![ctx.actor],
                source_skill: Some(ctx.skill_instance.skill_id.clone()),
                remaining_turns: Some(1),
                stacks: 1,
                consumed: false,
                payload: json!({
                    "blocks_next_discard_response": true,
                    "score_penalty": score_penalty_value("jin_chan_tuo_qiao", ctx.skill_instance.level),
                }),
            },
        },
        GameEvent::SkillScoreAdjusted {
            seat: ctx.actor,
            delta: -score_penalty_value("jin_chan_tuo_qiao", ctx.skill_instance.level),
            reason: Some("jin_chan_tuo_qiao".to_string()),
        },
    ])
}

fn activate_tou_liang_huan_zhu(
    ctx: &mut SkillContext<'_>,
    tile_ids: &[TileId],
) -> Result<Vec<GameEvent>, String> {
    let player = round_player(ctx, ctx.actor).ok_or_else(|| "invalid_action".to_string())?;
    let meld_index = parse_meld_index(tile_ids).unwrap_or(0);
    let tile_keys = player
        .melds
        .get(meld_index)
        .cloned()
        .ok_or_else(|| "invalid_action".to_string())?;
    Ok(vec![
        GameEvent::SkillActivated {
            seat: ctx.actor,
            skill_id: ctx.skill_instance.skill_id.clone(),
        },
        GameEvent::SkillReclaimMeld {
            seat: ctx.actor,
            meld_index,
            tile_keys: tile_keys.clone(),
        },
        GameEvent::SkillScoreAdjusted {
            seat: ctx.actor,
            delta: -score_penalty_value("tou_liang_huan_zhu", ctx.skill_instance.level),
            reason: Some("tou_liang_huan_zhu".to_string()),
        },
    ])
}

fn activate_zou_wei_shang_ji(ctx: &mut SkillContext<'_>) -> Result<Vec<GameEvent>, String> {
    let penalty = score_penalty_value("zou_wei_shang_ji", ctx.skill_instance.level);
    Ok(vec![
        GameEvent::SkillActivated {
            seat: ctx.actor,
            skill_id: ctx.skill_instance.skill_id.clone(),
        },
        GameEvent::SkillForceDraw {
            seat: ctx.actor,
            penalty,
            next_round_penalty: 0,
        },
    ])
}

fn apply_after_scoring(
    skill_id: &str,
    ctx: &RuleContext<'_>,
    request: &ScoreHookRequest,
    result: &mut FanResult,
) {
    let actor = ctx.actor;
    let is_winner = request.evaluation.winner_seat == Some(actor);
    let gain = gain_value(skill_id, ctx.skill_instance.level);
    let loss = loss_value(skill_id, ctx.skill_instance.level);

    match skill_id {
        "man_tian_guo_hai" if is_winner => adjust_score_delta(
            result,
            actor,
            if open_meld_count(request) == 0 {
                gain
            } else {
                -loss
            },
        ),
        "wei_wei_jiu_zhao" => {
            if is_winner {
                adjust_score_delta(result, actor, -loss);
            } else if seat_delta(result, actor) < 0 {
                adjust_score_delta(result, actor, gain.min(-seat_delta(result, actor)));
            }
        }
        "jie_dao_sha_ren" if is_winner => {
            let melds = open_meld_count(request) as i64;
            adjust_score_delta(result, actor, if melds > 0 { gain * melds } else { -loss });
        }
        "yi_yi_dai_lao" if is_winner => adjust_score_delta(
            result,
            actor,
            if live_tiles_remaining(ctx) < 40 {
                gain
            } else {
                -loss
            },
        ),
        "chen_huo_da_jie" if is_winner => adjust_score_delta(
            result,
            actor,
            if opponents_look_ready(ctx) {
                gain
            } else {
                -loss
            },
        ),
        "ge_an_guan_huo" if request.evaluation.winner_seat.is_some() => {
            adjust_score_delta(result, actor, -loss)
        }
        "xiao_li_cang_dao" if is_winner => adjust_score_delta(
            result,
            actor,
            if has_any_fan(result, "outside_hand")
                || has_any_fan(result, "pung_of_terminals_or_honours")
            {
                gain
            } else if request.evaluation.features.duan_yao {
                -loss
            } else {
                0
            },
        ),
        "li_dai_tao_jiang" => {
            if is_winner {
                adjust_score_delta(result, actor, -loss);
            } else if seat_delta(result, actor) < 0 {
                adjust_score_delta(result, actor, gain.min(-seat_delta(result, actor)));
            }
        }
        "shun_shou_qian_yang" if is_winner => adjust_score_delta(
            result,
            actor,
            if request.evaluation.timing.robbing_the_kong || multi_hu_window(ctx, actor) {
                gain
            } else {
                -loss
            },
        ),
        "da_cao_jing_she" if is_winner => {
            adjust_score_delta(result, actor, if has_any_kong(ctx) { gain } else { -loss })
        }
        "jie_shi_huan_hun" if is_winner => adjust_score_delta(
            result,
            actor,
            match request.evaluation.incoming_tile.as_deref() {
                Some(tile) if tracker_discard_count(ctx, tile) > 0 => gain,
                Some(_) => -loss,
                None => 0,
            },
        ),
        "diao_hu_li_shan" if is_winner => adjust_score_delta(
            result,
            actor,
            if request.evaluation.features.duan_yao {
                gain
            } else if request
                .evaluation
                .tile_keys
                .iter()
                .any(|tile| is_terminal_or_honour(tile))
            {
                -loss
            } else {
                0
            },
        ),
        "yu_qin_gu_zong" => {
            let active = active_effects(ctx, "yu_qin_gu_zong_window");
            if let Some(effect) = active.first() {
                let still_valid = effect
                    .payload
                    .get("expires_version")
                    .and_then(Value::as_u64)
                    .is_some_and(|expires| current_round_version(ctx) <= expires);
                adjust_score_delta(
                    result,
                    actor,
                    if is_winner && still_valid {
                        gain
                    } else if !is_winner {
                        -loss
                    } else {
                        0
                    },
                );
            }
        }
        "pao_zhuan_yin_yu" if is_winner => {
            let discarded_five = tracker_bool_for_seat(ctx, "discarded_five_by_seat", actor);
            let hand_has_five = request
                .evaluation
                .tile_keys
                .iter()
                .any(|tile| tile_is_five(tile));
            adjust_score_delta(
                result,
                actor,
                if hand_has_five {
                    -loss
                } else if discarded_five {
                    gain
                } else {
                    0
                },
            );
        }
        "qin_zei_qin_wang" if is_winner => adjust_score_delta(
            result,
            actor,
            if result.fan_total >= 12 {
                gain
            } else if result.fan_total < 8 {
                -loss
            } else {
                0
            },
        ),
        "fu_di_chou_xin" if is_winner => adjust_score_delta(
            result,
            actor,
            if live_tiles_remaining(ctx) <= 10 {
                gain
            } else if tiles_drawn_since_opening(ctx) <= 30 {
                -loss
            } else {
                0
            },
        ),
        "hun_shui_mo_yu" if is_winner => adjust_score_delta(
            result,
            actor,
            if winner_has_full_tile_diversity(request) {
                gain
            } else {
                -loss
            },
        ),
        "guan_men_zhuo_zei" if is_winner => adjust_score_delta(
            result,
            actor,
            if has_any_fan(result, "edge_wait")
                || has_any_fan(result, "closed_wait")
                || has_any_fan(result, "single_wait")
            {
                gain
            } else {
                -loss
            },
        ),
        "yuan_jiao_jin_gong" if is_winner => {
            if let Some(discarder) = request.evaluation.discarder_seat {
                adjust_score_delta(
                    result,
                    actor,
                    if same_seat_distance(actor, discarder) == 2 {
                        gain
                    } else {
                        -loss
                    },
                );
            }
        }
        "jia_dao_fa_guo" if is_winner => adjust_score_delta(
            result,
            actor,
            if request.evaluation.timing.gang_shang_hua {
                gain
            } else if !winner_has_kong(ctx, actor) {
                -loss
            } else {
                0
            },
        ),
        "zhi_sang_ma_huai" if is_winner => adjust_score_delta(
            result,
            actor,
            if tracker_bool_for_seat(ctx, "honor_redraw_success_by_seat", actor) {
                gain
            } else {
                -loss
            },
        ),
        "jia_chi_bu_dian" if is_winner => adjust_score_delta(
            result,
            actor,
            if result.fan_total >= 16 { -loss } else { 0 },
        ),
        "shang_wu_chou_ti" if is_winner => adjust_score_delta(
            result,
            actor,
            if live_tiles_remaining(ctx) > 80 {
                -loss
            } else {
                scaled_late_game_bonus(gain, live_tiles_remaining(ctx))
            },
        ),
        "shu_shang_kai_hua" if is_winner => adjust_score_delta(
            result,
            actor,
            if request.evaluation.flower_count > 0 {
                gain * request.evaluation.flower_count as i64
            } else {
                -loss
            },
        ),
        "fan_ke_wei_zhu" => {
            let dealer = ctx
                .room_state
                .round_state
                .as_ref()
                .map(|round| round.dealer_seat)
                .unwrap_or(0);
            if is_winner {
                adjust_score_delta(
                    result,
                    actor,
                    if actor != dealer && request.evaluation.discarder_seat == Some(dealer) {
                        gain
                    } else if actor == dealer && request.evaluation.discarder_seat.is_none() {
                        gain
                    } else {
                        0
                    },
                );
            } else if request.evaluation.winner_seat.is_some() {
                adjust_score_delta(result, actor, -loss);
            }
        }
        "mei_ren_ji" if is_winner => {
            let groups = honour_group_count(request);
            adjust_score_delta(
                result,
                actor,
                if groups > 0 {
                    gain * groups as i64
                } else {
                    -loss
                },
            );
        }
        "kong_cheng_ji" if is_winner => adjust_score_delta(
            result,
            actor,
            match concealed_tile_count(request) {
                1 | 4 => gain,
                10 | 13 => -loss,
                _ => 0,
            },
        ),
        "fan_jian_ji" if is_winner => adjust_score_delta(
            result,
            actor,
            if tracker_i64_for_seat(ctx, "claimed_discard_counts_by_seat", actor) > 0 {
                gain
            } else {
                -loss
            },
        ),
        "ku_rou_ji" if is_winner => adjust_score_delta(
            result,
            actor,
            if cumulative_score(ctx, actor) < 0 {
                gain
            } else {
                -loss
            },
        ),
        "lian_huan_ji" => {
            let streak = lian_huan_streak(ctx, actor);
            if is_winner && streak > 0 {
                adjust_score_delta(result, actor, gain * streak);
            } else if !is_winner && streak > 0 {
                adjust_score_delta(result, actor, -loss);
            }
        }
        _ => {}
    }
}

fn apply_after_draw(skill_id: &str, ctx: &RuleContext<'_>, settlement: &mut RoundSettlement) {
    let actor = ctx.actor;
    let gain = gain_value(skill_id, ctx.skill_instance.level);
    let loss = loss_value(skill_id, ctx.skill_instance.level);
    match skill_id {
        "ge_an_guan_huo" => adjust_draw_delta(settlement, actor, gain),
        "yu_qin_gu_zong" if !active_effects(ctx, "yu_qin_gu_zong_window").is_empty() => {
            adjust_draw_delta(settlement, actor, -loss)
        }
        "lian_huan_ji" if lian_huan_streak(ctx, actor) > 0 => {
            adjust_draw_delta(settlement, actor, -loss)
        }
        _ => {}
    }
}

fn adjust_draw_delta(settlement: &mut RoundSettlement, seat: Seat, delta: i64) {
    if delta == 0 {
        return;
    }
    *settlement
        .score_delta
        .fan_delta_by_seat
        .entry(seat)
        .or_default() += delta;
    *settlement
        .score_delta
        .total_delta_by_seat
        .entry(seat)
        .or_default() += delta;
}

fn opponents_look_ready(ctx: &RuleContext<'_>) -> bool {
    ctx.room_state.round_state.as_ref().is_some_and(|round| {
        round
            .skill_trackers
            .tenpai_seats
            .iter()
            .any(|seat| *seat != ctx.actor)
    })
}

fn multi_hu_window(ctx: &RuleContext<'_>, actor: Seat) -> bool {
    ctx.room_state.round_state.as_ref().is_some_and(|round| {
        round
            .skill_trackers
            .multi_hu_candidates
            .iter()
            .any(|seat| *seat != actor)
    })
}

fn has_any_kong(ctx: &RuleContext<'_>) -> bool {
    ctx.room_state
        .round_state
        .as_ref()
        .is_some_and(|round| !round.skill_trackers.players_with_kong.is_empty())
}

fn winner_has_kong(ctx: &RuleContext<'_>, actor: Seat) -> bool {
    round_player(ctx, actor)
        .map(|player| player.melds.iter().any(|meld| meld.len() == 4))
        .unwrap_or(false)
        || ctx
            .room_state
            .round_state
            .as_ref()
            .map(|round| {
                round
                    .score_trackers
                    .kong_entries
                    .iter()
                    .any(|entry| entry.actor_seat == actor)
            })
            .unwrap_or(false)
}

fn tiles_drawn_since_opening(ctx: &RuleContext<'_>) -> usize {
    ctx.room_state
        .round_state
        .as_ref()
        .map(|round| round.skill_trackers.tiles_drawn_since_opening.max(0) as usize)
        .filter(|value| *value > 0)
        .or_else(|| {
            ctx.room_state
                .round_state
                .as_ref()
                .map(|round| round.wall.head_index.saturating_sub(53))
        })
        .unwrap_or(0)
}

fn scaled_late_game_bonus(max_bonus: i64, live_tiles: usize) -> i64 {
    let scarcity = (80usize.saturating_sub(live_tiles)) as i64;
    ((max_bonus * scarcity) / 80).max(1)
}

fn honour_group_count(request: &ScoreHookRequest) -> usize {
    let Some(decomposition) = request.evaluation.decompositions.first() else {
        return 0;
    };
    let meld_groups = decomposition
        .melds
        .iter()
        .filter(|meld| meld.len() == 3 && meld.iter().all(|tile| is_honour_tile(tile)))
        .count();
    meld_groups + usize::from(decomposition.pair.as_deref().is_some_and(is_honour_tile))
}

fn winner_has_full_tile_diversity(request: &ScoreHookRequest) -> bool {
    request
        .evaluation
        .tile_keys
        .iter()
        .filter_map(|tile| tile_family(tile))
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        == 4
}

fn tile_family(tile_key: &str) -> Option<&'static str> {
    match tile_key.as_bytes() {
        [b'w', ..] => Some("wan"),
        [b't', ..] => Some("tong"),
        [b'b', ..] => Some("tiao"),
        [_, ..] => Some("honor"),
        [] => None,
    }
}

fn cumulative_score(ctx: &RuleContext<'_>, actor: Seat) -> i64 {
    ctx.room_state
        .match_state
        .as_ref()
        .and_then(|state| state.cumulative_scores.get(&actor).copied())
        .unwrap_or(0)
}

fn lian_huan_streak(ctx: &RuleContext<'_>, actor: Seat) -> i64 {
    ctx.room_state
        .match_state
        .as_ref()
        .and_then(|state| state.skill_trackers.lian_huan_ji.streaks.get(&actor))
        .copied()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::{apply_after_scoring, gain_value, loss_value};
    use crate::core::state::{
        MatchState, PlayerRoundState, RoomState, RoundState, RuleRuntimeState, SeatState,
        SkillInstance, SkillLoadout,
    };
    use crate::rules::scoring::{
        EvaluationInput, FanResult, HandFeatures, ScoreDelta, TimingFeatures,
    };
    use crate::rules::skills::{RuleContext, ScoreHookRequest};

    #[test]
    fn jia_chi_bu_dian_only_penalizes_big_hands() {
        let room = room_with_skill("jia_chi_bu_dian");
        let skill = room.round_state.as_ref().unwrap().players[0]
            .skill_loadout
            .equipped[0]
            .clone();
        let ctx = RuleContext::new(&room, 0, &skill);

        let request = score_hook_request(vec!["w1", "w2", "w3"]);
        let mut regular_hand = fan_result_with_total(12);
        apply_after_scoring("jia_chi_bu_dian", &ctx, &request, &mut regular_hand);
        assert_eq!(regular_hand.score_delta.total_delta_by_seat[0], 24);

        let mut big_hand = fan_result_with_total(16);
        apply_after_scoring("jia_chi_bu_dian", &ctx, &request, &mut big_hand);
        assert_eq!(
            big_hand.score_delta.total_delta_by_seat[0],
            24 - loss_value("jia_chi_bu_dian", skill.level)
        );
    }

    #[test]
    fn pao_zhuan_yin_yu_penalizes_hands_that_still_keep_a_five() {
        let mut room = room_with_skill("pao_zhuan_yin_yu");
        room.round_state
            .as_mut()
            .unwrap()
            .skill_trackers
            .discarded_five_by_seat
            .insert(0, true);
        let skill = room.round_state.as_ref().unwrap().players[0]
            .skill_loadout
            .equipped[0]
            .clone();
        let ctx = RuleContext::new(&room, 0, &skill);

        let mut keeps_a_five = fan_result_with_total(8);
        apply_after_scoring(
            "pao_zhuan_yin_yu",
            &ctx,
            &score_hook_request(vec!["w5", "w1", "w2"]),
            &mut keeps_a_five,
        );
        assert_eq!(
            keeps_a_five.score_delta.total_delta_by_seat[0],
            24 - loss_value("pao_zhuan_yin_yu", skill.level)
        );

        let mut cashed_out = fan_result_with_total(8);
        apply_after_scoring(
            "pao_zhuan_yin_yu",
            &ctx,
            &score_hook_request(vec!["w1", "w2", "w3"]),
            &mut cashed_out,
        );
        assert_eq!(
            cashed_out.score_delta.total_delta_by_seat[0],
            24 + gain_value("pao_zhuan_yin_yu", skill.level)
        );
    }

    fn room_with_skill(skill_id: &str) -> RoomState {
        RoomState {
            table_code: "ROOM42".to_string(),
            phase: "playing".to_string(),
            mode: "skill".to_string(),
            test_mode: false,
            enforce_minimum_eight_fan: true,
            seats: (0..4)
                .map(|seat_index| SeatState {
                    seat_index,
                    connected: true,
                    ready: true,
                    seat_type: "human".to_string(),
                    ..Default::default()
                })
                .collect(),
            match_state: Some(MatchState {
                prevailing_wind: "east".to_string(),
                hand_number: 1,
                dealer_seat: 0,
                cumulative_scores: BTreeMap::from([(0, 0), (1, 0), (2, 0), (3, 0)]),
                match_finished: false,
                last_completed_round_id: None,
                statistics: Default::default(),
                skill_trackers: Default::default(),
            }),
            round_state: Some(RoundState {
                round_id: "round-1".to_string(),
                dealer_seat: 0,
                round_wind: "east".to_string(),
                current_actor: 0,
                phase: "playing".to_string(),
                players: vec![
                    PlayerRoundState {
                        seat: 0,
                        skill_loadout: SkillLoadout {
                            equipped: vec![SkillInstance {
                                skill_id: skill_id.to_string(),
                                owner: 0,
                                level: 1,
                                rarity: "common".to_string(),
                                remaining_rounds: 2,
                                cooldown: 0,
                                charges: 1,
                                charges_per_round: 1,
                                config: json!({}),
                            }],
                        },
                        ..Default::default()
                    },
                    PlayerRoundState {
                        seat: 1,
                        ..Default::default()
                    },
                    PlayerRoundState {
                        seat: 2,
                        ..Default::default()
                    },
                    PlayerRoundState {
                        seat: 3,
                        ..Default::default()
                    },
                ],
                rule_state: RuleRuntimeState {
                    enforce_minimum_eight_fan: true,
                },
                ..Default::default()
            }),
            pending_timeout: None,
            continue_action: None,
        }
    }

    fn score_hook_request(tile_keys: Vec<&str>) -> ScoreHookRequest {
        ScoreHookRequest {
            evaluation: EvaluationInput {
                win_type: "discard".to_string(),
                winner_seat: Some(0),
                discarder_seat: Some(1),
                flower_count: 0,
                seat_count: 4,
                features: HandFeatures::default(),
                timing: TimingFeatures::default(),
                kong_entries: Vec::new(),
                tile_keys: tile_keys.into_iter().map(ToString::to_string).collect(),
                visible_tile_keys: Vec::new(),
                concealed_tile_keys: Vec::new(),
                meld_tile_key_groups: Vec::new(),
                open_meld_tile_key_groups: Vec::new(),
                incoming_tile: Some("w3".to_string()),
                decompositions: Vec::new(),
            },
            required_minimum_fan_total: 8,
        }
    }

    fn fan_result_with_total(fan_total: i64) -> FanResult {
        FanResult {
            fan_total,
            minimum_qualifying_fan_total: 8,
            fan_keys: Vec::new(),
            fan_breakdown: Vec::new(),
            score_delta: ScoreDelta {
                provisional: true,
                basic_points: fan_total,
                base_points: 8,
                fan_total,
                minimum_qualifying_fan_total: 8,
                fan_delta_by_seat: vec![24, -8, -8, -8],
                kong_delta_by_seat: vec![0, 0, 0, 0],
                total_delta_by_seat: vec![24, -8, -8, -8],
            },
            kong_score_detail: Vec::new(),
            provisional: true,
        }
    }
}
