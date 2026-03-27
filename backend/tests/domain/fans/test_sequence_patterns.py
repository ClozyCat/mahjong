from app.domain.fans.rules.sequence_patterns import get_sequence_pattern_rules


def test_sequence_pattern_rules_match_pure_double_chow() -> None:
    rules = {rule.fan_key: rule for rule in get_sequence_pattern_rules()}
    context = {"features": {"ping_hu": True, "yi_ban_gao": True}}

    assert rules["pure_double_chow"].matcher(context) == 1
