use serde_json::Value;

use crate::core::action::PlayerAction;
use crate::core::ids::{Seat, TileId};
use crate::core::state::{EffectInstance, KnowledgeEffect, RoomState, SkillInstance};
use crate::rules::scoring::{ScoreRequest, ScoreResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SkillActivation {
    #[default]
    Passive,
    ActiveTurn,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillHookKind {
    BeforeAction,
    AfterAction,
    BeforeDraw,
    BeforeHuCheck,
    BeforeScoring,
    AfterScoring,
    BuildView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreHookRequest {
    pub evaluation: ScoreRequest,
    pub required_minimum_fan_total: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillProjection {
    pub visible_effects: Vec<EffectInstance>,
    pub private_knowledge: Vec<KnowledgeEffect>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawRequest {
    pub seat: Seat,
    pub draw_count: usize,
    pub source: String,
    pub reveal_to: Vec<Seat>,
}

impl Default for DrawRequest {
    fn default() -> Self {
        Self {
            seat: 0,
            draw_count: 1,
            source: "wall".to_string(),
            reveal_to: Vec::new(),
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HuCheckRequest {
    pub seat: Seat,
    pub incoming_tile_id: Option<TileId>,
    pub minimum_fan: Option<i64>,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct RuleContext<'a> {
    pub room_state: &'a RoomState,
    pub actor: Seat,
    pub skill_instance: &'a SkillInstance,
}

#[allow(dead_code)]
pub type SkillContext<'a> = RuleContext<'a>;

impl<'a> RuleContext<'a> {
    pub fn new(room_state: &'a RoomState, actor: Seat, skill_instance: &'a SkillInstance) -> Self {
        Self {
            room_state,
            actor,
            skill_instance,
        }
    }
}

#[allow(dead_code)]
pub trait RuleHook {
    fn activation(&self) -> SkillActivation {
        SkillActivation::Passive
    }

    fn append_action_options(
        &self,
        ctx: &RuleContext<'_>,
        options: &mut Vec<String>,
    ) -> Result<(), String> {
        if self.activation() == SkillActivation::ActiveTurn && ctx.skill_instance.charges > 0 {
            options.push(format!("skill:{}", ctx.skill_instance.skill_id));
        }
        Ok(())
    }

    fn can_activate(
        &self,
        _ctx: &RuleContext<'_>,
        _target: Option<Seat>,
        _tile_ids: &[TileId],
    ) -> Result<(), String> {
        Ok(())
    }

    fn on_decline_hu(
        &self,
        _ctx: &RuleContext<'_>,
    ) -> Result<Vec<crate::core::event::GameEvent>, String> {
        Ok(Vec::new())
    }

    fn before_action(
        &self,
        _ctx: &RuleContext<'_>,
        _action: &mut PlayerAction,
    ) -> Result<(), String> {
        Ok(())
    }

    fn before_draw(
        &self,
        _ctx: &RuleContext<'_>,
        _request: &mut DrawRequest,
    ) -> Result<(), String> {
        Ok(())
    }

    fn before_hu_check(
        &self,
        _ctx: &RuleContext<'_>,
        _request: &mut HuCheckRequest,
    ) -> Result<(), String> {
        Ok(())
    }

    fn before_scoring(
        &self,
        _ctx: &RuleContext<'_>,
        _request: &mut ScoreHookRequest,
    ) -> Result<(), String> {
        Ok(())
    }

    fn after_scoring(
        &self,
        _ctx: &RuleContext<'_>,
        _request: &ScoreHookRequest,
        _result: &mut ScoreResult,
    ) -> Result<(), String> {
        Ok(())
    }

    fn build_view(
        &self,
        _ctx: &RuleContext<'_>,
        _local_seat: Seat,
        _projection: &mut SkillProjection,
    ) -> Result<(), String> {
        Ok(())
    }

    fn after_draw_settlement(
        &self,
        _ctx: &RuleContext<'_>,
        _settlement: &mut Value,
    ) -> Result<(), String> {
        Ok(())
    }
}
