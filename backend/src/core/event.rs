use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::ids::{Seat, SkillId};
use crate::core::state::effect::{EffectInstance, KnowledgeEffect, RuleOverride};
use crate::core::tile::Tile;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameEvent {
    MatchStarted {
        dealer: Seat,
    },
    RoundStarted {
        round_id: String,
    },
    TileDrawn {
        seat: Seat,
        tile: Tile,
        source: String,
    },
    TileDiscarded {
        seat: Seat,
        tile: Tile,
    },
    MeldClaimed {
        seat: Seat,
        meld: Vec<String>,
        from: Seat,
    },
    HuDeclared {
        winner: Seat,
        source: String,
    },
    SettlementPrepared {
        settlement: SettlementResult,
    },
    SkillActivated {
        seat: Seat,
        skill_id: SkillId,
    },
    EffectApplied {
        effect: EffectInstance,
    },
    EffectExpired {
        effect_id: String,
    },
    ViewKnowledgeGranted {
        seat: Seat,
        knowledge: KnowledgeEffect,
    },
    RuleOverrideApplied {
        override_rule: RuleOverride,
    },
    LegacyRoundEvent {
        event_type: String,
        event: Value,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettlementResult {
    pub winner: Option<Seat>,
    pub fan_total: Option<i64>,
    pub total_delta_by_seat: BTreeMap<Seat, i64>,
}
