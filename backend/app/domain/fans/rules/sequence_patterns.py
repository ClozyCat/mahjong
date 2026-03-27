from __future__ import annotations

from app.domain.fans.models import FanRule


def get_sequence_pattern_rules() -> tuple[FanRule, ...]:
    return (
        FanRule(
            fan_key="pure_double_chow",
            fan_value=1,
            category="sequence_patterns",
            matcher=lambda context: int(bool(context.get("features", {}).get("yi_ban_gao"))),
        ),
    )
