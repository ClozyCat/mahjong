use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::core::ids::SkillId;

#[derive(Debug, Clone, Deserialize)]
pub struct SkillCatalog {
    #[serde(default)]
    pub selection: SkillSelectionConfig,
    pub skills: Vec<SkillCatalogEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SkillCatalogEntry {
    pub skill_id: SkillId,
    #[serde(default)]
    pub serial: Option<String>,
    pub name: String,
    pub summary: String,
    #[serde(rename = "type")]
    pub skill_type: SkillKind,
    pub interaction_kind: Option<SkillInteractionKind>,
    pub interaction_hint: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub tiers: SkillTierSet,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SkillTierSet {
    pub common: SkillTierData,
    pub rare: SkillTierData,
    pub epic: SkillTierData,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SkillTierData {
    pub detail: String,
    pub gain: Option<i64>,
    pub loss: Option<i64>,
    pub preview_count: Option<usize>,
    pub minimum_fan_penalty: Option<i64>,
    pub minimum_fan_override: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SkillSelectionConfig {
    pub duration_seconds: i64,
    pub duration_rounds: u8,
    pub active_uses_per_round: u8,
    pub common_weight: u8,
    pub rare_weight: u8,
    pub epic_weight: u8,
    pub offer_count: usize,
}

impl Default for SkillSelectionConfig {
    fn default() -> Self {
        Self {
            duration_seconds: 30,
            duration_rounds: 2,
            active_uses_per_round: 1,
            common_weight: 65,
            rare_weight: 30,
            epic_weight: 5,
            offer_count: 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillKind {
    Active,
    Passive,
}

impl SkillKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Active => "主动技能",
            Self::Passive => "被动技能",
        }
    }

    pub fn activation_limit_per_round(self) -> u8 {
        match self {
            Self::Active => 1,
            Self::Passive => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillInteractionKind {
    Confirm,
    PreviewWall,
    SelectTarget,
    SelectHandTile,
    SelectMeld,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkillRarity {
    Common,
    Rare,
    Epic,
}

impl SkillRarity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Common => "普通",
            Self::Rare => "稀有",
            Self::Epic => "史诗",
        }
    }

    pub fn tone(self) -> &'static str {
        match self {
            Self::Common => "jade",
            Self::Rare => "azure",
            Self::Epic => "violet",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Common => "common",
            Self::Rare => "rare",
            Self::Epic => "epic",
        }
    }
}

impl SkillCatalogEntry {
    pub fn tier(&self, rarity: SkillRarity) -> &SkillTierData {
        match rarity {
            SkillRarity::Common => &self.tiers.common,
            SkillRarity::Rare => &self.tiers.rare,
            SkillRarity::Epic => &self.tiers.epic,
        }
    }

    pub fn tier_for_level(&self, level: u8) -> &SkillTierData {
        self.tier(rarity_for_level(level))
    }
}

pub fn catalog() -> &'static SkillCatalog {
    static CATALOG: OnceLock<SkillCatalog> = OnceLock::new();
    CATALOG.get_or_init(load_catalog)
}

pub fn entry(skill_id: &str) -> Option<&'static SkillCatalogEntry> {
    catalog().skills.iter().find(|entry| entry.skill_id == skill_id)
}

pub fn stratagem_skill_ids() -> &'static [SkillId] {
    static IDS: OnceLock<Vec<SkillId>> = OnceLock::new();
    IDS.get_or_init(|| catalog().skills.iter().map(|entry| entry.skill_id.clone()).collect())
        .as_slice()
}

pub fn rarity_for_level(level: u8) -> SkillRarity {
    match level {
        0 | 1 => SkillRarity::Common,
        2 => SkillRarity::Rare,
        _ => SkillRarity::Epic,
    }
}

pub fn kind_for_skill(skill_id: &str) -> Option<SkillKind> {
    entry(skill_id).map(|entry| entry.skill_type)
}

pub fn interaction_kind_for_skill(skill_id: &str) -> Option<SkillInteractionKind> {
    entry(skill_id).and_then(|entry| entry.interaction_kind)
}

pub fn interaction_hint_for_skill(skill_id: &str) -> Option<String> {
    entry(skill_id).and_then(|entry| entry.interaction_hint.clone())
}

pub fn detail_for_skill(skill_id: &str, level: u8) -> Option<String> {
    entry(skill_id).map(|entry| entry.tier_for_level(level).detail.clone())
}

pub fn value_i64_for_skill(skill_id: &str, level: u8, key: &str) -> i64 {
    let Some(tier) = entry(skill_id).map(|entry| entry.tier_for_level(level)) else {
        return 0;
    };
    match key {
        "gain" => tier.gain.unwrap_or(0),
        "loss" => tier.loss.unwrap_or(0),
        "minimum_fan_penalty" => tier.minimum_fan_penalty.unwrap_or(0),
        "minimum_fan_override" => tier.minimum_fan_override.unwrap_or(0),
        _ => 0,
    }
}

pub fn value_usize_for_skill(skill_id: &str, level: u8, key: &str) -> usize {
    let Some(tier) = entry(skill_id).map(|entry| entry.tier_for_level(level)) else {
        return 0;
    };
    match key {
        "preview_count" => tier.preview_count.unwrap_or(0),
        _ => 0,
    }
}

pub fn rarity_weights() -> &'static BTreeMap<SkillRarity, u8> {
    static WEIGHTS: OnceLock<BTreeMap<SkillRarity, u8>> = OnceLock::new();
    WEIGHTS.get_or_init(|| {
        let selection = &catalog().selection;
        BTreeMap::from([
            (SkillRarity::Common, selection.common_weight),
            (SkillRarity::Rare, selection.rare_weight),
            (SkillRarity::Epic, selection.epic_weight),
        ])
    })
}

fn load_catalog() -> SkillCatalog {
    let embedded = include_str!("../../../data/skills.json");
    serde_json::from_str::<SkillCatalog>(embedded)
        .unwrap_or_else(|error| panic!("failed to parse backend/data/skills.json: {error}"))
}
