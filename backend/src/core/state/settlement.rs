use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::ids::Seat;

use super::{KongTrackerEntry, i64_opt, seat_vec, string_opt};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RoundSettlement {
    pub provisional: bool,
    pub win_type: String,
    pub winner_seat: Option<Seat>,
    pub discarder_seat: Option<Seat>,
    pub display_win_label: Option<String>,
    pub fan_total: i64,
    pub fan_keys: Vec<String>,
    pub fan_breakdown: Vec<SettlementFanBreakdownEntry>,
    pub score_delta: SettlementScoreDelta,
    pub flower_count: usize,
    pub draw_type: Option<String>,
    pub kong_score_detail: Vec<SettlementKongScoreDetailEntry>,
}

impl RoundSettlement {
    pub(crate) fn from_value(value: &Value) -> Self {
        Self {
            provisional: value
                .get("provisional")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            win_type: value
                .get("win_type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            winner_seat: value
                .get("winner_seat")
                .and_then(Value::as_u64)
                .map(|seat| seat as Seat),
            discarder_seat: value
                .get("discarder_seat")
                .and_then(Value::as_u64)
                .map(|seat| seat as Seat),
            display_win_label: string_opt(value, "display_win_label"),
            fan_total: value
                .get("fan_total")
                .and_then(Value::as_i64)
                .unwrap_or_default(),
            fan_keys: value
                .get("fan_keys")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(ToString::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            fan_breakdown: value
                .get("fan_breakdown")
                .and_then(Value::as_array)
                .map(|entries| {
                    entries
                        .iter()
                        .map(SettlementFanBreakdownEntry::from_value)
                        .collect()
                })
                .unwrap_or_default(),
            score_delta: SettlementScoreDelta::from_value(value.get("score_delta")),
            flower_count: value
                .get("flower_count")
                .and_then(Value::as_u64)
                .map(|count| count as usize)
                .unwrap_or_default(),
            draw_type: string_opt(value, "draw_type"),
            kong_score_detail: value
                .get("kong_score_detail")
                .and_then(Value::as_array)
                .map(|entries| {
                    entries
                        .iter()
                        .map(SettlementKongScoreDetailEntry::from_value)
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    pub(crate) fn to_value(&self) -> Value {
        serde_json::json!({
            "provisional": self.provisional,
            "win_type": self.win_type,
            "winner_seat": self.winner_seat,
            "discarder_seat": self.discarder_seat,
            "display_win_label": self.display_win_label,
            "fan_total": self.fan_total,
            "fan_keys": self.fan_keys,
            "fan_breakdown": self
                .fan_breakdown
                .iter()
                .map(SettlementFanBreakdownEntry::to_value)
                .collect::<Vec<_>>(),
            "score_delta": self.score_delta.to_value(),
            "flower_count": self.flower_count,
            "draw_type": self.draw_type,
            "kong_score_detail": self
                .kong_score_detail
                .iter()
                .map(SettlementKongScoreDetailEntry::to_value)
                .collect::<Vec<_>>(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SettlementFanBreakdownEntry {
    pub fan_key: String,
    pub fan_value: i64,
}

impl SettlementFanBreakdownEntry {
    fn from_value(value: &Value) -> Self {
        Self {
            fan_key: value
                .get("fan_key")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            fan_value: value
                .get("fan_value")
                .and_then(Value::as_i64)
                .unwrap_or_default(),
        }
    }

    fn to_value(&self) -> Value {
        serde_json::json!({
            "fan_key": self.fan_key,
            "fan_value": self.fan_value,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SettlementScoreDelta {
    pub provisional: bool,
    pub basic_points: i64,
    pub base_points: i64,
    pub fan_total: i64,
    pub minimum_qualifying_fan_total: i64,
    pub fan_delta_by_seat: BTreeMap<Seat, i64>,
    pub kong_delta_by_seat: BTreeMap<Seat, i64>,
    pub total_delta_by_seat: BTreeMap<Seat, i64>,
}

impl SettlementScoreDelta {
    fn from_value(value: Option<&Value>) -> Self {
        let Some(value) = value else {
            return Self::default();
        };
        Self {
            provisional: value
                .get("provisional")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            basic_points: i64_opt(value, "basic_points").unwrap_or_default(),
            base_points: i64_opt(value, "base_points").unwrap_or_default(),
            fan_total: i64_opt(value, "fan_total").unwrap_or_default(),
            minimum_qualifying_fan_total: i64_opt(value, "minimum_qualifying_fan_total")
                .unwrap_or_default(),
            fan_delta_by_seat: score_map_from_value(value.get("fan_delta_by_seat")),
            kong_delta_by_seat: score_map_from_value(value.get("kong_delta_by_seat")),
            total_delta_by_seat: score_map_from_value(value.get("total_delta_by_seat")),
        }
    }

    pub fn to_value(&self) -> Value {
        serde_json::json!({
            "provisional": self.provisional,
            "basic_points": self.basic_points,
            "base_points": self.base_points,
            "fan_total": self.fan_total,
            "minimum_qualifying_fan_total": self.minimum_qualifying_fan_total,
            "fan_delta_by_seat": score_map_to_value(&self.fan_delta_by_seat),
            "kong_delta_by_seat": score_map_to_value(&self.kong_delta_by_seat),
            "total_delta_by_seat": score_map_to_value(&self.total_delta_by_seat),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SettlementKongScoreDetailEntry {
    pub kong_type: String,
    pub actor_seat: Seat,
    pub payer_seats: Vec<Seat>,
    pub delta_by_seat: BTreeMap<Seat, i64>,
}

impl SettlementKongScoreDetailEntry {
    pub(crate) fn from_value(value: &Value) -> Self {
        Self {
            kong_type: value
                .get("kong_type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            actor_seat: value
                .get("actor_seat")
                .and_then(Value::as_u64)
                .map(|seat| seat as Seat)
                .unwrap_or_default(),
            payer_seats: seat_vec(value.get("payer_seats")),
            delta_by_seat: score_map_from_value(value.get("delta_by_seat")),
        }
    }

    pub(crate) fn from_tracker_entry(entry: &KongTrackerEntry, seat_count: usize) -> Self {
        let unit_score = match entry.kong_type.as_str() {
            "exposed_kong" | "concealed_kong" | "add_kong" => 1_i64,
            _ => 0,
        };
        let mut delta_by_seat = zero_score_map(seat_count);
        for payer_seat in &entry.payer_seats {
            if let Some(delta) = delta_by_seat.get_mut(payer_seat) {
                *delta -= unit_score;
            }
            if let Some(delta) = delta_by_seat.get_mut(&entry.actor_seat) {
                *delta += unit_score;
            }
        }
        Self {
            kong_type: entry.kong_type.clone(),
            actor_seat: entry.actor_seat,
            payer_seats: entry.payer_seats.clone(),
            delta_by_seat,
        }
    }

    fn to_value(&self) -> Value {
        serde_json::json!({
            "kong_type": self.kong_type,
            "actor_seat": self.actor_seat,
            "payer_seats": self.payer_seats,
            "delta_by_seat": score_map_to_value(&self.delta_by_seat),
        })
    }
}

pub(crate) fn zero_score_map(seat_count: usize) -> BTreeMap<Seat, i64> {
    (0..seat_count).map(|seat| (seat, 0)).collect()
}

fn score_map_from_value(value: Option<&Value>) -> BTreeMap<Seat, i64> {
    value
        .and_then(Value::as_object)
        .map(|object| {
            object
                .iter()
                .filter_map(|(seat, delta)| seat.parse::<usize>().ok().zip(delta.as_i64()))
                .collect()
        })
        .unwrap_or_default()
}

fn score_map_to_value(scores: &BTreeMap<Seat, i64>) -> Value {
    Value::Object(
        scores
            .iter()
            .map(|(seat, delta)| (seat.to_string(), Value::from(*delta)))
            .collect(),
    )
}
