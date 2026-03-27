from __future__ import annotations

from app.domain.fans.models import FanRule


def get_timing_pattern_rules() -> tuple[FanRule, ...]:
    return (
        FanRule(
            fan_key="self_drawn",
            fan_value=1,
            category="timing_patterns",
            matcher=lambda context: int(context.get("win_type") == "self_draw"),
        ),
        FanRule(
            fan_key="out_with_replacement_tile",
            fan_value=8,
            category="timing_patterns",
            matcher=lambda context: int(bool(context.get("timing", {}).get("gang_shang_hua"))),
            excludes=("self_drawn",),
        ),
        FanRule(
            fan_key="last_tile_draw",
            fan_value=8,
            category="timing_patterns",
            matcher=lambda context: int(
                bool(context.get("timing", {}).get("hai_di_lao_yue"))
                and not bool(context.get("timing", {}).get("gang_shang_hua"))
            ),
            excludes=("self_drawn",),
        ),
        FanRule(
            fan_key="last_tile_claim",
            fan_value=8,
            category="timing_patterns",
            matcher=lambda context: int(
                bool(context.get("timing", {}).get("he_di_lao_yu"))
                and not bool(context.get("timing", {}).get("gang_shang_hua"))
                and not bool(context.get("timing", {}).get("hai_di_lao_yue"))
            ),
        ),
        FanRule(
            fan_key="robbing_the_kong",
            fan_value=8,
            category="timing_patterns",
            matcher=lambda context: int(bool(context.get("timing", {}).get("robbing_the_kong"))),
            excludes=("last_tile",),
        ),
    )
