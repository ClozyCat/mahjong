pub mod bot_view;
pub mod hand_insight;
pub mod match_result;
pub mod prompt;
pub mod room_snapshot;
pub mod support;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatProjectionSupport {
    pub has_concealed_flower: bool,
    pub has_self_kong: bool,
    pub can_hu: bool,
    pub can_ready_hand: bool,
    pub restricted_discard_tile_ids: Vec<String>,
}
