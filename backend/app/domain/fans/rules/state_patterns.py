from __future__ import annotations

from app.domain.fans.models import FanRule


def get_state_pattern_rules() -> tuple[FanRule, ...]:
    return (
        FanRule(
            fan_key="concealed_hand",
            fan_value=2,
            category="state_patterns",
            matcher=lambda context: int(bool(context.get("features", {}).get("concealed_hand"))),
        ),
        FanRule(
            fan_key="fully_concealed_hand",
            fan_value=4,
            category="state_patterns",
            matcher=lambda context: int(
                bool(context.get("features", {}).get("concealed_hand"))
                and context.get("win_type") == "self_draw"
            ),
            excludes=("self_drawn", "concealed_hand"),
        ),
        FanRule(
            fan_key="melded_hand",
            fan_value=6,
            category="state_patterns",
            matcher=lambda context: int(_is_melded_hand(context)),
            excludes=("single_wait",),
        ),
        FanRule(
            fan_key="flower_tiles",
            fan_value=1,
            category="state_patterns",
            matcher=lambda context: int(context.get("flower_count", 0) or 0),
            value_resolver=lambda _context, match_count, fan_value: [match_count * fan_value]
            if match_count > 0
            else [],
        ),
        FanRule(
            fan_key="last_tile",
            fan_value=4,
            category="state_patterns",
            matcher=lambda context: int(_is_last_tile(context)),
        ),
        FanRule(
            fan_key="chicken_hand",
            fan_value=8,
            category="state_patterns",
            matcher=lambda _context: 0,
        ),
    )


def _is_melded_hand(context: dict) -> bool:
    return (
        context.get("win_type") == "discard"
        and len(context.get("open_meld_tile_key_groups", context.get("meld_tile_key_groups", []))) == 4
        and len(context.get("concealed_tile_keys", [])) == 2
    )


def _is_last_tile(context: dict) -> bool:
    winning_tile = context.get("winning_tile")
    if not winning_tile:
        return False
    visible_tile_keys = context.get("visible_tile_keys", [])
    return sum(1 for tile_key in visible_tile_keys if tile_key == winning_tile) >= 3
