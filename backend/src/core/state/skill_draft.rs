use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::core::ids::{Seat, SkillId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SkillDraftState {
    pub cycle_key: String,
    pub cycle_label: String,
    pub round_id: String,
    pub deadline_at: String,
    pub offers_by_seat: BTreeMap<Seat, SkillDraftOffer>,
}

impl SkillDraftState {
    pub fn is_active(&self) -> bool {
        self.offers_by_seat
            .values()
            .any(|offer| offer.status == SkillDraftStatus::Pending)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SkillDraftOffer {
    pub seat: Seat,
    pub status: SkillDraftStatus,
    pub options: Vec<SkillDraftChoice>,
    pub selected_skill_id: Option<SkillId>,
    pub selected_rarity: Option<SkillRarity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SkillDraftChoice {
    pub skill_id: SkillId,
    pub rarity: SkillRarity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SkillDraftStatus {
    #[default]
    Pending,
    Selected,
    Declined,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SkillRarity {
    #[default]
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

    pub fn level(self) -> u8 {
        match self {
            Self::Common => 1,
            Self::Rare => 2,
            Self::Epic => 3,
        }
    }
}
