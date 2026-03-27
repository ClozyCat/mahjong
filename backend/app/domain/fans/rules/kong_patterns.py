from __future__ import annotations

from app.domain.fans.models import FanRule


def get_kong_pattern_rules() -> tuple[FanRule, ...]:
    return (
        FanRule(
            fan_key="concealed_kong",
            fan_value=2,
            category="kong_patterns",
            matcher=lambda context: int(_concealed_kong_count(context) >= 1),
        ),
        FanRule(
            fan_key="two_concealed_kongs",
            fan_value=8,
            category="kong_patterns",
            matcher=lambda context: int(_concealed_kong_count(context) >= 2),
            excludes=("concealed_kong",),
        ),
        FanRule(
            fan_key="two_melded_kongs",
            fan_value=4,
            category="kong_patterns",
            matcher=lambda context: int(_melded_kong_count(context) >= 2),
        ),
        FanRule(
            fan_key="melded_kong",
            fan_value=1,
            category="kong_patterns",
            matcher=lambda context: int(_melded_kong_count(context) >= 1),
        ),
        FanRule(
            fan_key="three_kongs",
            fan_value=32,
            category="kong_patterns",
            matcher=lambda context: int(_total_kong_count(context) >= 3),
        ),
        FanRule(
            fan_key="four_kongs",
            fan_value=88,
            category="kong_patterns",
            matcher=lambda context: int(_total_kong_count(context) >= 4),
            excludes=("all_pungs", "single_wait"),
        ),
    )


def _concealed_kong_count(context: dict) -> int:
    winner_seat = context.get("winner_seat")
    return sum(
        1
        for entry in context.get("kong_entries", [])
        if winner_seat is None or entry.get("actor_seat") == winner_seat
        if entry.get("kong_type") == "concealed_kong"
    )


def _melded_kong_count(context: dict) -> int:
    winner_seat = context.get("winner_seat")
    return sum(
        1
        for entry in context.get("kong_entries", [])
        if winner_seat is None or entry.get("actor_seat") == winner_seat
        if entry.get("kong_type") in {"exposed_kong", "add_kong"}
    )


def _total_kong_count(context: dict) -> int:
    winner_seat = context.get("winner_seat")
    return sum(
        1
        for entry in context.get("kong_entries", [])
        if winner_seat is None or entry.get("actor_seat") == winner_seat
    )
