from __future__ import annotations

from app.domain.fans.models import FanRule


def get_suit_pattern_rules() -> tuple[FanRule, ...]:
    return (
        FanRule(
            fan_key="all_simples",
            fan_value=2,
            category="suit_patterns",
            matcher=lambda context: int(
                bool(context.get("features", {}).get("duan_yao"))
                and not bool(context.get("features", {}).get("hun_yao_jiu"))
                and not bool(context.get("features", {}).get("qing_yao_jiu"))
            ),
            excludes=("no_honours",),
        ),
        FanRule(
            fan_key="full_flush",
            fan_value=24,
            category="suit_patterns",
            matcher=lambda context: int(bool(context.get("features", {}).get("pure_one_suit"))),
            excludes=("one_voided_suit", "no_honours"),
        ),
        FanRule(
            fan_key="half_flush",
            fan_value=6,
            category="suit_patterns",
            matcher=lambda context: int(
                bool(context.get("features", {}).get("mixed_one_suit"))
                and not bool(context.get("features", {}).get("pure_one_suit"))
            ),
            excludes=("one_voided_suit",),
        ),
    )
