use serde::{Deserialize, Serialize};

use crate::core::ids::{Seat, SkillId};
use crate::core::state::RoundSettlement;
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
    FlowerExposed {
        seat: Seat,
        tile_id: String,
    },
    SelfKongDeclared {
        seat: Seat,
        kong_type: String,
        tile_key: String,
        tile_ids: Vec<String>,
    },
    ClaimAutoPassed {
        discarder_seat: Seat,
        seats: Vec<Seat>,
    },
    HuDeclared {
        winner: Seat,
        source: String,
    },
    SettlementPrepared {
        settlement: RoundSettlement,
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
    SkillTileReplaced {
        seat: Seat,
        removed_tile_id: String,
        replacement_tile: Tile,
    },
    SkillReclaimMeld {
        seat: Seat,
        meld_index: usize,
        tile_keys: Vec<String>,
    },
    SkillForceDraw {
        seat: Seat,
        penalty: i64,
        next_round_penalty: i64,
    },
    SkillScoreAdjusted {
        seat: Seat,
        delta: i64,
        reason: Option<String>,
    },
}
