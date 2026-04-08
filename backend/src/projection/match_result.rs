use serde::Serialize;
use serde_json::Value;

use crate::core::state::{RoomState, RoundSettlement};

#[derive(Debug, Clone, Serialize)]
struct MatchResultMessage {
    #[serde(rename = "type")]
    kind: &'static str,
    payload: MatchResultPayload,
}

#[derive(Debug, Clone, Serialize)]
struct MatchResultPayload {
    table_code: String,
    round_id: String,
    phase: &'static str,
    #[serde(flatten)]
    settlement: RoundSettlement,
}

pub fn match_result_message(state: &RoomState) -> Option<Value> {
    let round = state.round_state.as_ref()?;
    if round.phase != "settlement" {
        return None;
    }
    let settlement = round.settlement.clone()?;
    serde_json::to_value(MatchResultMessage {
        kind: "match_result",
        payload: MatchResultPayload {
            table_code: state.table_code.clone(),
            round_id: round.round_id.clone(),
            phase: "settlement",
            settlement,
        },
    })
    .ok()
}
