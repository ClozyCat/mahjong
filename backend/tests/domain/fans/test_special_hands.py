from app.domain.fans.rules.special_hands import get_special_hand_rules


def test_special_hand_rules_match_thirteen_orphans_and_seven_pairs() -> None:
    rules = {rule.fan_key: rule for rule in get_special_hand_rules()}

    assert rules["thirteen_orphans"].matcher({"features": {"thirteen_orphans": True}}) == 1
    assert rules["seven_pairs"].matcher({"features": {"seven_pairs": True}}) == 1
