from app.domain.fans.rules.state_patterns import get_state_pattern_rules


def test_state_pattern_rules_match_concealed_and_flower_counts() -> None:
    rules = {rule.fan_key: rule for rule in get_state_pattern_rules()}
    context = {
        "features": {"concealed_hand": True},
        "flower_count": 2,
    }

    assert rules["concealed_hand"].matcher(context) == 1
    assert rules["flower_tiles"].matcher(context) == 2
