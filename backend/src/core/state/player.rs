use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::error::EngineError;
use crate::core::ids::{Seat, TileKey};
use crate::core::tile::Tile;

use super::{SkillLoadout, array, bool_or, i64_opt, string_opt, usize_or};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SeatState {
    pub seat_index: Seat,
    pub nickname: Option<String>,
    pub reconnect_token: Option<String>,
    pub player_session_id: Option<i64>,
    pub connected: bool,
    pub ready: bool,
    pub is_bot: bool,
    pub seat_type: String,
    pub bot_persona: Option<String>,
    pub bot_aggression: Option<i64>,
    pub disconnect_deadline_at: Option<String>,
    #[serde(default)]
    pub skill_loadout: SkillLoadout,
}

impl SeatState {
    pub(crate) fn from_value(value: &Value) -> Self {
        Self {
            seat_index: usize_or(value, "seat_index", 0),
            nickname: string_opt(value, "nickname"),
            reconnect_token: string_opt(value, "reconnect_token"),
            player_session_id: i64_opt(value, "player_session_id"),
            connected: bool_or(value, "connected", false),
            ready: bool_or(value, "ready", false),
            is_bot: bool_or(value, "is_bot", false),
            seat_type: value
                .get("seat_type")
                .and_then(Value::as_str)
                .unwrap_or("human")
                .to_string(),
            bot_persona: string_opt(value, "bot_persona"),
            bot_aggression: i64_opt(value, "bot_aggression"),
            disconnect_deadline_at: string_opt(value, "disconnect_deadline_at"),
            skill_loadout: SkillLoadout::from_value(value.get("skill_loadout")).unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlayerRoundState {
    pub seat: Seat,
    pub concealed_tiles: Vec<Tile>,
    pub melds: Vec<Vec<TileKey>>,
    pub flowers: Vec<Tile>,
    pub discards: Vec<Tile>,
    pub skill_loadout: SkillLoadout,
}

impl PlayerRoundState {
    pub(crate) fn from_value(value: &Value) -> Result<Self, EngineError> {
        let concealed_tiles = parse_tiles(value.get("concealed_tiles"), "concealed_tiles")?;
        let flowers = parse_tiles(value.get("flowers"), "flowers")?;
        let discards = parse_tiles(value.get("discards"), "discards")?;
        let melds = value
            .get("melds")
            .map(|melds| {
                array(melds, "round_state.players[].melds").map(|melds| {
                    melds
                        .iter()
                        .map(|meld| {
                            meld.as_array()
                                .map(|tiles| {
                                    tiles
                                        .iter()
                                        .filter_map(|tile| tile.as_str().map(ToString::to_string))
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default()
                        })
                        .collect::<Vec<_>>()
                })
            })
            .transpose()?
            .unwrap_or_default();
        Ok(Self {
            seat: usize_or(value, "seat", 0),
            concealed_tiles,
            melds,
            flowers,
            discards,
            skill_loadout: SkillLoadout::from_value(value.get("skill_loadout"))?,
        })
    }
}

fn parse_tiles(value: Option<&Value>, context: &str) -> Result<Vec<Tile>, EngineError> {
    value
        .map(|value| {
            array(value, context).and_then(|tiles| {
                tiles
                    .iter()
                    .enumerate()
                    .map(|(index, tile)| {
                        Tile::from_value(tile, &format!("{context}[{index}]"))
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
        })
        .transpose()
        .map(Option::unwrap_or_default)
}
