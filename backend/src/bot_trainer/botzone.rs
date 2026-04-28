#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BotZoneMatch {
    pub(crate) match_id: String,
    pub(crate) round_wind: String,
    pub(crate) deals: [Vec<String>; 4],
    pub(crate) events: Vec<BotZoneEvent>,
    pub(crate) result: BotZoneResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BotZoneEvent {
    pub(crate) actor: usize,
    pub(crate) action: BotZoneAction,
    pub(crate) ignored_claims: Vec<BotZoneIgnoredClaim>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BotZoneIgnoredClaim {
    pub(crate) actor: usize,
    pub(crate) action: BotZoneAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BotZoneAction {
    Draw { tile_key: String },
    Play { tile_key: String },
    Chi { middle_tile_key: String },
    Peng { tile_key: String },
    Gang { tile_key: String },
    AnGang { tile_key: String },
    BuGang { tile_key: String },
    Hu { tile_key: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BotZoneResult {
    Hu {
        fan: i64,
        description: String,
        score_delta: [i64; 4],
    },
    Huang {
        score_delta: [i64; 4],
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BotZoneParseError {
    message: String,
}

impl BotZoneParseError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for BotZoneParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for BotZoneParseError {}

pub(crate) fn map_botzone_tile(raw: &str) -> Option<String> {
    let raw = raw.trim();
    match raw {
        "F1" => Some("east".to_string()),
        "F2" => Some("north".to_string()),
        "F3" => Some("west".to_string()),
        "F4" => Some("south".to_string()),
        "J1" => Some("red".to_string()),
        "J2" => Some("green".to_string()),
        "J3" => Some("white".to_string()),
        _ => {
            let mut chars = raw.chars();
            let suit = match chars.next()? {
                'W' => 'w',
                'T' => 't',
                'B' => 'b',
                _ => return None,
            };
            let rank = chars.next()?;
            if chars.next().is_some() || !('1'..='9').contains(&rank) {
                return None;
            }
            Some(format!("{suit}{rank}"))
        }
    }
}

pub(crate) fn parse_matches(raw: &str) -> Result<Vec<BotZoneMatch>, BotZoneParseError> {
    let mut blocks = Vec::new();
    let mut current = Vec::new();
    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if line.starts_with("Match ") && !current.is_empty() {
            blocks.push(current);
            current = Vec::new();
        }
        current.push(line.to_string());
    }
    if !current.is_empty() {
        blocks.push(current);
    }

    blocks
        .into_iter()
        .map(|block| parse_match_lines(&block))
        .collect()
}

#[cfg(test)]
pub(crate) fn parse_match(raw: &str) -> Result<BotZoneMatch, BotZoneParseError> {
    let lines = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    parse_match_lines(&lines)
}

fn parse_match_lines(lines: &[String]) -> Result<BotZoneMatch, BotZoneParseError> {
    let mut match_id = None;
    let mut round_wind = "east".to_string();
    let mut deals: [Vec<String>; 4] = std::array::from_fn(|_| Vec::new());
    let mut events = Vec::new();
    let mut score_delta = [0_i64; 4];
    let mut fan = 0_i64;
    let mut description = String::new();
    let mut is_drawn = false;

    for line in lines {
        if let Some(rest) = line.strip_prefix("Match ") {
            match_id = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("Wind ") {
            round_wind = parse_wind(rest);
            continue;
        }
        if let Some((seat, rest)) = parse_player_prefix(line) {
            if let Some(deal) = rest.strip_prefix("Deal ") {
                deals[seat] = deal
                    .split_whitespace()
                    .filter_map(map_botzone_tile)
                    .collect();
                continue;
            }
            events.push(parse_event_line(line)?);
            continue;
        }
        if let Some(rest) = line.strip_prefix("Fan ") {
            let mut parts = rest.splitn(2, char::is_whitespace);
            fan = parts
                .next()
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(0);
            description = parts.next().unwrap_or_default().trim().to_string();
            continue;
        }
        if let Some(rest) = line.strip_prefix("Score ") {
            score_delta = parse_score_delta(rest);
            continue;
        }
        if line.starts_with("Huang") {
            is_drawn = true;
        }
    }

    let result = if is_drawn {
        BotZoneResult::Huang { score_delta }
    } else {
        BotZoneResult::Hu {
            fan,
            description,
            score_delta,
        }
    };

    Ok(BotZoneMatch {
        match_id: match_id.unwrap_or_else(|| "unknown".to_string()),
        round_wind,
        deals,
        events,
        result,
    })
}

pub(crate) fn parse_event_line(line: &str) -> Result<BotZoneEvent, BotZoneParseError> {
    let tokens = line.split_whitespace().collect::<Vec<_>>();
    let (actor, mut index) = parse_player_tokens(&tokens, 0)?;
    let (action, next_index) = parse_action_tokens(&tokens, index)?;
    index = next_index;
    let mut ignored_claims = Vec::new();
    while index < tokens.len() {
        if tokens.get(index) != Some(&"Ignore") {
            return Err(BotZoneParseError::new(format!(
                "unexpected token '{}' in event line",
                tokens[index]
            )));
        }
        let (ignored_actor, after_player) = parse_player_tokens(&tokens, index + 1)?;
        let (ignored_action, after_action) = parse_action_tokens(&tokens, after_player)?;
        ignored_claims.push(BotZoneIgnoredClaim {
            actor: ignored_actor,
            action: ignored_action,
        });
        index = after_action;
    }

    Ok(BotZoneEvent {
        actor,
        action,
        ignored_claims,
    })
}

fn parse_player_prefix(line: &str) -> Option<(usize, &str)> {
    let rest = line.strip_prefix("Player ")?;
    let (seat, rest) = rest.split_once(' ')?;
    Some((seat.parse().ok()?, rest))
}

fn parse_player_tokens(tokens: &[&str], index: usize) -> Result<(usize, usize), BotZoneParseError> {
    if tokens.get(index) != Some(&"Player") {
        return Err(BotZoneParseError::new("expected Player token"));
    }
    let seat = tokens
        .get(index + 1)
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or_else(|| BotZoneParseError::new("expected player seat"))?;
    if seat >= 4 {
        return Err(BotZoneParseError::new("player seat out of range"));
    }
    Ok((seat, index + 2))
}

fn parse_action_tokens(
    tokens: &[&str],
    index: usize,
) -> Result<(BotZoneAction, usize), BotZoneParseError> {
    let action = tokens
        .get(index)
        .ok_or_else(|| BotZoneParseError::new("missing action"))?;
    let tile = tokens
        .get(index + 1)
        .and_then(|value| map_botzone_tile(value))
        .ok_or_else(|| BotZoneParseError::new("missing or invalid tile"))?;
    let normalized = match *action {
        "Draw" | "DRAW" | "Mo" => BotZoneAction::Draw { tile_key: tile },
        "Play" | "PLAY" | "打出" => BotZoneAction::Play { tile_key: tile },
        "Chi" | "CHI" => BotZoneAction::Chi {
            middle_tile_key: tile,
        },
        "Peng" | "PENG" => BotZoneAction::Peng { tile_key: tile },
        "Gang" | "GANG" => BotZoneAction::Gang { tile_key: tile },
        "AnGang" | "ANGANG" => BotZoneAction::AnGang { tile_key: tile },
        "BuGang" | "BUGANG" => BotZoneAction::BuGang { tile_key: tile },
        "Hu" | "HU" => BotZoneAction::Hu { tile_key: tile },
        other => return Err(BotZoneParseError::new(format!("unknown action {other}"))),
    };
    Ok((normalized, index + 2))
}

fn parse_score_delta(rest: &str) -> [i64; 4] {
    let values = rest
        .split_whitespace()
        .filter_map(|value| value.parse::<i64>().ok())
        .collect::<Vec<_>>();
    std::array::from_fn(|index| values.get(index).copied().unwrap_or(0))
}

fn parse_wind(raw: &str) -> String {
    match raw.trim() {
        "F1" | "east" | "East" => "east",
        "F2" | "north" | "North" => "north",
        "F3" | "west" | "West" => "west",
        "F4" | "south" | "South" => "south",
        _ => "east",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_botzone_tile_codes_to_backend_tile_keys() {
        assert_eq!(map_botzone_tile("W1"), Some("w1".to_string()));
        assert_eq!(map_botzone_tile("T9"), Some("t9".to_string()));
        assert_eq!(map_botzone_tile("B5"), Some("b5".to_string()));
        assert_eq!(map_botzone_tile("F1"), Some("east".to_string()));
        assert_eq!(map_botzone_tile("F2"), Some("north".to_string()));
        assert_eq!(map_botzone_tile("F3"), Some("west".to_string()));
        assert_eq!(map_botzone_tile("F4"), Some("south".to_string()));
        assert_eq!(map_botzone_tile("J1"), Some("red".to_string()));
        assert_eq!(map_botzone_tile("J2"), Some("green".to_string()));
        assert_eq!(map_botzone_tile("J3"), Some("white".to_string()));
    }

    #[test]
    fn parses_ignore_claims_on_action_line() {
        let event =
            parse_event_line("Player 1 Hu B3 Ignore Player 0 PENG B3 Ignore Player 3 CHI B4")
                .expect("event");

        assert_eq!(event.actor, 1);
        assert_eq!(
            event.action,
            BotZoneAction::Hu {
                tile_key: "b3".to_string()
            }
        );
        assert_eq!(event.ignored_claims.len(), 2);
        assert_eq!(event.ignored_claims[0].actor, 0);
        assert_eq!(
            event.ignored_claims[0].action,
            BotZoneAction::Peng {
                tile_key: "b3".to_string()
            }
        );
        assert_eq!(event.ignored_claims[1].actor, 3);
    }

    #[test]
    fn parses_complete_single_match() {
        let record = parse_match(
            r#"
Match fixture
Wind F2
Player 0 Deal W1 W2 W3
Player 1 Deal B1 B1 B2
Player 0 Draw T1
Player 0 Play W1
Score 8 -8 0 0
"#,
        )
        .expect("match");

        assert_eq!(record.match_id, "fixture");
        assert_eq!(record.round_wind, "north");
        assert_eq!(record.deals[0][0], "w1");
        assert_eq!(record.events.len(), 2);
    }
}
