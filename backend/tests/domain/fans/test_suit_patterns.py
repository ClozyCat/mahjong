from app.domain.fans.rules.suit_patterns import get_suit_pattern_rules


def test_suit_pattern_rules_match_expected_flags() -> None:
    rules = {rule.fan_key: rule for rule in get_suit_pattern_rules()}
    context = {
        "features": {
            "qing_yao_jiu": False,
            "hun_yao_jiu": False,
            "duan_yao": False,
            "pure_one_suit": True,
            "mixed_one_suit": False,
        }
    }

    assert rules["full_flush"].matcher(context) == 1
    assert rules["half_flush"].matcher(context) == 0
    assert rules["all_simples"].matcher(context) == 0
