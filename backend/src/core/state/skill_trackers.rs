use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::ids::Seat;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RoundSkillTrackers {
    pub claimed_discard_counts_by_seat: BTreeMap<Seat, i64>,
    pub pending_honor_rebuy_tile_by_seat: BTreeMap<Seat, String>,
    pub honor_redraw_success_by_seat: BTreeMap<Seat, bool>,
    pub discard_counts: BTreeMap<String, i64>,
    pub discarded_five_by_seat: BTreeMap<Seat, bool>,
    pub discard_suits_by_seat: BTreeMap<Seat, Vec<String>>,
    pub players_with_kong: Vec<Seat>,
    pub live_tiles_remaining: i64,
    pub tiles_drawn_since_opening: i64,
    pub multi_hu_candidates: Vec<Seat>,
    pub tenpai_seats: Vec<Seat>,
    pub tenpai_waits_by_seat: BTreeMap<Seat, Vec<String>>,
}

impl RoundSkillTrackers {
    pub(crate) fn from_value(value: Option<&Value>) -> Self {
        match value {
            Some(value) if !value.is_null() => {
                serde_json::from_value(value.clone()).unwrap_or_default()
            }
            _ => Self::default(),
        }
    }

    pub(crate) fn to_value(&self) -> Value {
        if self.is_empty() {
            Value::Null
        } else {
            serde_json::to_value(self).unwrap_or(Value::Null)
        }
    }
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MatchSkillTrackers {
    pub lian_huan_ji: LianHuanJiTracker,
    pub zou_wei_shang_ji: ZouWeiShangJiTracker,
}

impl MatchSkillTrackers {
    pub(crate) fn from_value(value: Option<&Value>) -> Self {
        match value {
            Some(value) if !value.is_null() => {
                serde_json::from_value(value.clone()).unwrap_or_default()
            }
            _ => Self::default(),
        }
    }

    pub(crate) fn to_value(&self) -> Value {
        if self.is_empty() {
            Value::Null
        } else {
            serde_json::to_value(self).unwrap_or(Value::Null)
        }
    }
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LianHuanJiTracker {
    pub streaks: BTreeMap<Seat, i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ZouWeiShangJiTracker {
    pub pending_win_penalty: BTreeMap<Seat, i64>,
}
