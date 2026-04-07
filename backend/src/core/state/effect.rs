use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::ids::{EffectId, Seat, SkillId, TileId, TileKey};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SkillLoadout {
    pub equipped: Vec<SkillInstance>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillInstance {
    pub skill_id: SkillId,
    pub owner: Seat,
    pub level: u8,
    pub cooldown: u8,
    pub charges: u8,
    pub config: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EffectState {
    pub ongoing: Vec<EffectInstance>,
    pub hidden_knowledge: Vec<KnowledgeEffect>,
    pub rule_overrides: Vec<RuleOverride>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectInstance {
    pub effect_id: EffectId,
    pub effect_type: String,
    pub owner: Seat,
    pub target_seats: Vec<Seat>,
    pub source_skill: Option<SkillId>,
    pub remaining_turns: Option<u8>,
    pub stacks: u8,
    pub consumed: bool,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeEffect {
    pub viewer: Seat,
    pub target_seat: Option<Seat>,
    pub tile_ids: Vec<TileId>,
    pub tile_keys: Vec<TileKey>,
    pub source_skill: Option<SkillId>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleOverride {
    pub owner: Seat,
    pub target_seat: Option<Seat>,
    pub rule_key: String,
    pub source_skill: Option<SkillId>,
    pub payload: Value,
}
