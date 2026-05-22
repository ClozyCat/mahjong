use serde::Serialize;
use serde_json::Value;

use crate::core::ids::Seat;
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
    settlement_seats: Vec<SettlementSeatView>,
    #[serde(flatten)]
    settlement: RoundSettlement,
}

#[derive(Debug, Clone, Serialize)]
struct SettlementSeatView {
    seat_index: Seat,
    user_id: Option<i64>,
    nickname: Option<String>,
    points: Option<i64>,
    title: Option<String>,
    connected: bool,
    is_bot: bool,
    seat_type: String,
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
            settlement_seats: settlement_seats(state),
            settlement,
        },
    })
    .ok()
}

fn settlement_seats(state: &RoomState) -> Vec<SettlementSeatView> {
    state
        .seats
        .iter()
        .map(|seat| SettlementSeatView {
            seat_index: seat.seat_index,
            user_id: seat.user_id,
            nickname: seat.nickname.clone(),
            points: seat.points,
            title: seat.title.clone(),
            connected: seat.connected,
            is_bot: seat.is_bot,
            seat_type: seat.seat_type.clone(),
        })
        .collect()
}
