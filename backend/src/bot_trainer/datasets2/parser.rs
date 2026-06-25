use std::path::Path;

use super::super::botzone::{
    BotZoneAction, BotZoneEvent, BotZoneMatch, BotZoneResult, map_botzone_tile,
};

pub(crate) fn parse_match_text(raw: &str, source_name: &str) -> Result<BotZoneMatch, String> {
    let lines = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.len() < 6 {
        return Err("expected source, header, four deal lines".to_string());
    }

    let match_id = match_id_from_source(lines[0], source_name);
    let header = split_tab_line(lines[1]);
    let round_wind = header
        .first()
        .map_or("east".to_string(), |value| parse_wind(value));
    let dealer_seat = header
        .get(1)
        .and_then(|value| parse_seat(value))
        .unwrap_or(0);
    let result = parse_result(&header);

    let mut deals: [Vec<String>; 4] = std::array::from_fn(|_| Vec::new());
    for line in &lines[2..6] {
        let parts = split_tab_line(line);
        let seat = parts
            .first()
            .and_then(|value| parse_seat(value))
            .ok_or_else(|| format!("invalid deal line: {line}"))?;
        deals[seat] = parse_tile_list(parts.get(1).copied().unwrap_or_default());
    }

    let mut events = Vec::new();
    for line in &lines[6..] {
        if let Some(event) = parse_event(line)? {
            events.push(event);
        }
    }

    Ok(BotZoneMatch {
        match_id,
        round_wind,
        dealer_seat,
        cumulative_scores: [0; 4],
        deals,
        events,
        result,
    })
}

fn parse_event(line: &str) -> Result<Option<BotZoneEvent>, String> {
    let parts = split_tab_line(line);
    if parts.len() < 3 {
        return Ok(None);
    }
    let actor = parts
        .first()
        .and_then(|value| parse_seat(value))
        .ok_or_else(|| format!("invalid actor in event: {line}"))?;
    let action_name = parts[1];
    let tiles = parse_tile_list(parts[2]);
    let tile = tiles.first().cloned();
    let action = match action_name {
        "摸牌" => {
            let Some(tile_key) = tile else {
                return Ok(None);
            };
            BotZoneAction::Draw { tile_key }
        }
        "补花后摸牌" => BotZoneAction::Draw {
            tile_key: tile.ok_or_else(|| format!("missing draw tile: {line}"))?,
        },
        "打牌" => BotZoneAction::Play {
            tile_key: tile.ok_or_else(|| format!("missing play tile: {line}"))?,
        },
        "碰" => BotZoneAction::Peng {
            tile_key: parse_optional_tile(parts.get(3).copied())
                .or(tile)
                .ok_or_else(|| format!("missing peng tile: {line}"))?,
        },
        "吃" => BotZoneAction::Chi {
            middle_tile_key: middle_tile_key(&tiles)
                .ok_or_else(|| format!("missing chi middle tile: {line}"))?,
        },
        "杠" | "明杠" => BotZoneAction::Gang {
            tile_key: parse_optional_tile(parts.get(3).copied())
                .or(tile)
                .ok_or_else(|| format!("missing gang tile: {line}"))?,
        },
        "暗杠" => BotZoneAction::AnGang {
            tile_key: parse_optional_tile(parts.get(3).copied())
                .or(tile)
                .ok_or_else(|| format!("missing concealed kong tile: {line}"))?,
        },
        "补杠" | "补杠牌" => BotZoneAction::BuGang {
            tile_key: parse_optional_tile(parts.get(3).copied())
                .or(tile)
                .ok_or_else(|| format!("missing add kong tile: {line}"))?,
        },
        "胡" | "和" | "自摸" => BotZoneAction::Hu {
            tile_key: tile.unwrap_or_default(),
        },
        "补花" => return Ok(None),
        _ => return Ok(None),
    };
    Ok(Some(BotZoneEvent {
        actor,
        action,
        ignored_claims: Vec::new(),
    }))
}

fn parse_result(header: &[&str]) -> BotZoneResult {
    let is_drawn = header.iter().any(|value| value.contains("荒庄"));
    if is_drawn {
        BotZoneResult::Huang {
            score_delta: parse_score_delta(header),
        }
    } else {
        BotZoneResult::Hu {
            fan: parse_fan(header),
            description: header.last().copied().unwrap_or_default().to_string(),
            score_delta: parse_score_delta(header),
        }
    }
}

fn parse_score_delta(header: &[&str]) -> [i64; 4] {
    header
        .iter()
        .find_map(|value| {
            let trimmed = value.trim();
            if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
                return None;
            }
            let numbers = trimmed
                .trim_matches(['[', ']'])
                .split(',')
                .filter_map(|part| part.trim().trim_matches('\'').parse::<i64>().ok())
                .collect::<Vec<_>>();
            if numbers.len() == 4 {
                Some(std::array::from_fn(|index| numbers[index]))
            } else {
                None
            }
        })
        .unwrap_or([0; 4])
}

fn parse_fan(header: &[&str]) -> i64 {
    header
        .iter()
        .filter_map(|value| {
            value
                .trim()
                .trim_matches(['[', ']', '\''])
                .split('-')
                .next_back()
                .and_then(|part| part.parse::<i64>().ok())
        })
        .max()
        .unwrap_or(0)
}

fn parse_tile_list(raw: &str) -> Vec<String> {
    raw.trim()
        .trim_matches(['[', ']'])
        .split(',')
        .filter_map(|part| {
            let tile = part.trim().trim_matches('\'').trim_matches('"');
            if tile.starts_with('H') {
                return None;
            }
            map_botzone_tile(tile)
        })
        .collect()
}

fn parse_optional_tile(raw: Option<&str>) -> Option<String> {
    raw.and_then(|value| parse_tile_list(value).into_iter().next())
}

fn middle_tile_key(tiles: &[String]) -> Option<String> {
    let mut indexes = tiles
        .iter()
        .filter_map(|tile| crate::bot::action_space::tile_index(tile))
        .collect::<Vec<_>>();
    indexes.sort_unstable();
    crate::bot::action_space::TILE_KEYS
        .get(*indexes.get(indexes.len() / 2)?)
        .map(|value| (*value).to_string())
}

fn match_id_from_source(first_line: &str, source_name: &str) -> String {
    Path::new(first_line)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            Path::new(source_name)
                .file_stem()
                .and_then(|value| value.to_str())
        })
        .unwrap_or("unknown")
        .to_string()
}

fn split_tab_line(line: &str) -> Vec<&str> {
    line.split('\t').map(str::trim).collect()
}

fn parse_seat(raw: &str) -> Option<usize> {
    raw.trim().parse::<usize>().ok().filter(|seat| *seat < 4)
}

fn parse_wind(raw: &str) -> String {
    match raw.trim() {
        "东" | "F1" | "east" => "east",
        "南" | "F2" | "south" => "south",
        "西" | "F3" | "west" => "west",
        "北" | "F4" | "north" => "north",
        _ => "east",
    }
    .to_string()
}
