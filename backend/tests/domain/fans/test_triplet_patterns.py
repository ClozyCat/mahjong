from app.domain.fans.rules.triplet_patterns import get_triplet_pattern_rules


def test_triplet_pattern_rules_count_triplets_and_winds() -> None:
    rules = {rule.fan_key: rule for rule in get_triplet_pattern_rules()}
    context = {
        "features": {
            "pung_hand": True,
            "seat_wind_triplet": True,
            "round_wind_triplet": True,
            "dragon_triplet_count": 2,
            "terminal_triplet_count": 2,
            "non_seat_non_round_wind_triplet_count": 1,
        }
    }

    assert rules["all_pungs"].matcher(context) == 1
    assert rules["seat_wind"].matcher(context) == 1
    assert rules["prevalent_wind"].matcher(context) == 1
    assert rules["dragon_pung"].matcher(context) == 2
    assert rules["pung_of_terminals_or_honours"].matcher(context) == 3


def test_triplet_pattern_rules_count_concealed_and_shifted_pungs() -> None:
    rules = {rule.fan_key: rule for rule in get_triplet_pattern_rules()}
    context = {
        "winner_seat": 0,
        "concealed_tile_keys": [
            "w2",
            "w2",
            "w2",
            "w3",
            "w3",
            "w3",
            "w4",
            "w4",
            "w4",
            "w5",
            "w5",
            "w5",
            "red",
            "red",
        ],
        "kong_entries": [],
        "standard_decompositions": [
            {
                "pair": "red",
                "melds": [
                    ["w2", "w2", "w2"],
                    ["w3", "w3", "w3"],
                    ["w4", "w4", "w4"],
                    ["w5", "w5", "w5"],
                ],
            }
        ],
    }

    assert rules["two_concealed_pungs"].matcher(context) == 1
    assert rules["three_concealed_pungs"].matcher(context) == 1
    assert rules["four_concealed_pungs"].matcher(context) == 1
    assert rules["four_pure_shifted_pungs"].matcher(context) == 1
