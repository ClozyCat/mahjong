from app.domain.fans.rules.timing_patterns import get_timing_pattern_rules


def test_timing_pattern_rules_match_self_draw_and_timing_flags() -> None:
    rules = {rule.fan_key: rule for rule in get_timing_pattern_rules()}
    context = {
        "win_type": "self_draw",
        "timing": {
            "gang_shang_hua": True,
            "hai_di_lao_yue": False,
            "he_di_lao_yu": False,
        },
    }

    assert rules["self_drawn"].matcher(context) == 1
    assert rules["out_with_replacement_tile"].matcher(context) == 1
    assert rules["last_tile_draw"].matcher(context) == 0
