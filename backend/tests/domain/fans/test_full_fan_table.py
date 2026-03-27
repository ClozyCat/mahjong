from app.domain.fans.engine import evaluate_fan_context


def test_evaluate_fan_context_scores_big_three_dragons() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "dragon_triplet_count": 3,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
            },
            "tile_keys": [
                "red",
                "red",
                "red",
                "green",
                "green",
                "green",
                "white",
                "white",
                "white",
                "w1",
                "w1",
                "w1",
                "w9",
                "w9",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "w9",
                    "melds": [
                        ["red", "red", "red"],
                        ["green", "green", "green"],
                        ["white", "white", "white"],
                        ["w1", "w1", "w1"],
                    ],
                }
            ],
        }
    )

    assert "big_three_dragons" in result["fan_keys"]


def test_evaluate_fan_context_scores_little_three_dragons() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "dragon_triplet_count": 2,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
            },
            "tile_keys": [
                "red",
                "red",
                "red",
                "green",
                "green",
                "green",
                "white",
                "white",
                "w1",
                "w1",
                "w1",
                "w9",
                "w9",
                "w9",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "white",
                    "melds": [
                        ["red", "red", "red"],
                        ["green", "green", "green"],
                        ["w1", "w1", "w1"],
                        ["w9", "w9", "w9"],
                    ],
                }
            ],
        }
    )

    assert "little_three_dragons" in result["fan_keys"]


def test_evaluate_fan_context_scores_big_four_winds_and_all_honors() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "dragon_triplet_count": 0,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
            },
            "tile_keys": [
                "east",
                "east",
                "east",
                "south",
                "south",
                "south",
                "west",
                "west",
                "west",
                "north",
                "north",
                "north",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["east", "east", "east"],
                        ["south", "south", "south"],
                        ["west", "west", "west"],
                        ["north", "north", "north"],
                    ],
                }
            ],
        }
    )

    assert "big_four_winds" in result["fan_keys"]
    assert "all_honours" in result["fan_keys"]


def test_evaluate_fan_context_scores_little_four_winds() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "dragon_triplet_count": 0,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
            },
            "tile_keys": [
                "east",
                "east",
                "east",
                "south",
                "south",
                "south",
                "west",
                "west",
                "west",
                "north",
                "north",
                "red",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "north",
                    "melds": [
                        ["east", "east", "east"],
                        ["south", "south", "south"],
                        ["west", "west", "west"],
                        ["red", "red", "red"],
                    ],
                }
            ],
        }
    )

    assert "little_four_winds" in result["fan_keys"]


def test_evaluate_fan_context_scores_all_green() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "self_draw",
            "winner_seat": 0,
            "discarder_seat": None,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "dragon_triplet_count": 1,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
            },
            "tile_keys": [
                "t2",
                "t2",
                "t2",
                "t3",
                "t3",
                "t3",
                "t4",
                "t4",
                "t4",
                "t6",
                "t6",
                "t8",
                "t8",
                "green",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "t8",
                    "melds": [
                        ["t2", "t2", "t2"],
                        ["t3", "t3", "t3"],
                        ["t4", "t4", "t4"],
                        ["green", "green", "green"],
                    ],
                }
            ],
        }
    )

    assert "all_green" in result["fan_keys"]


def test_evaluate_fan_context_scores_big_three_winds() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "dragon_triplet_count": 0,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
            },
            "tile_keys": [
                "east",
                "east",
                "east",
                "south",
                "south",
                "south",
                "west",
                "west",
                "west",
                "red",
                "red",
                "red",
                "w9",
                "w9",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "w9",
                    "melds": [
                        ["east", "east", "east"],
                        ["south", "south", "south"],
                        ["west", "west", "west"],
                        ["red", "red", "red"],
                    ],
                }
            ],
        }
    )

    assert "big_three_winds" in result["fan_keys"]


def test_evaluate_fan_context_scores_two_dragon_pungs() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "dragon_triplet_count": 2,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
            },
            "tile_keys": [
                "red",
                "red",
                "red",
                "green",
                "green",
                "green",
                "w1",
                "w1",
                "w1",
                "w9",
                "w9",
                "w9",
                "east",
                "east",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "east",
                    "melds": [
                        ["red", "red", "red"],
                        ["green", "green", "green"],
                        ["w1", "w1", "w1"],
                        ["w9", "w9", "w9"],
                    ],
                }
            ],
        }
    )

    assert "two_dragon_pungs" in result["fan_keys"]


def test_evaluate_fan_context_scores_all_terminals_and_honors() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "dragon_triplet_count": 1,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 3,
            },
            "tile_keys": [
                "w1",
                "w1",
                "w1",
                "w9",
                "w9",
                "w9",
                "t1",
                "t1",
                "t1",
                "east",
                "east",
                "east",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w1", "w1", "w1"],
                        ["w9", "w9", "w9"],
                        ["t1", "t1", "t1"],
                        ["east", "east", "east"],
                    ],
                }
            ],
        }
    )

    assert "all_terminals_and_honours" in result["fan_keys"]


def test_evaluate_fan_context_scores_all_terminals() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "dragon_triplet_count": 0,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 4,
            },
            "tile_keys": [
                "w1",
                "w1",
                "w1",
                "w9",
                "w9",
                "w9",
                "t1",
                "t1",
                "t1",
                "t9",
                "t9",
                "t9",
                "b1",
                "b1",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "b1",
                    "melds": [
                        ["w1", "w1", "w1"],
                        ["w9", "w9", "w9"],
                        ["t1", "t1", "t1"],
                        ["t9", "t9", "t9"],
                    ],
                }
            ],
        }
    )

    assert "all_terminals" in result["fan_keys"]


def test_evaluate_fan_context_scores_all_even_pungs() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "dragon_triplet_count": 0,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
                "pung_hand": True,
            },
            "tile_keys": [
                "w2",
                "w2",
                "w2",
                "w4",
                "w4",
                "w4",
                "t6",
                "t6",
                "t6",
                "b8",
                "b8",
                "b8",
                "w6",
                "w6",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "w6",
                    "melds": [
                        ["w2", "w2", "w2"],
                        ["w4", "w4", "w4"],
                        ["t6", "t6", "t6"],
                        ["b8", "b8", "b8"],
                    ],
                }
            ],
        }
    )

    assert "all_even_pungs" in result["fan_keys"]


def test_evaluate_fan_context_scores_pure_straight() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "dragon_triplet_count": 0,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
            },
            "tile_keys": [
                "w1",
                "w2",
                "w3",
                "w4",
                "w5",
                "w6",
                "w7",
                "w8",
                "w9",
                "t2",
                "t3",
                "t4",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w1", "w2", "w3"],
                        ["w4", "w5", "w6"],
                        ["w7", "w8", "w9"],
                        ["t2", "t3", "t4"],
                    ],
                }
            ],
        }
    )

    assert "pure_straight" in result["fan_keys"]


def test_evaluate_fan_context_scores_mixed_triple_chow() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "dragon_triplet_count": 0,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
            },
            "tile_keys": [
                "w2",
                "w3",
                "w4",
                "t2",
                "t3",
                "t4",
                "b2",
                "b3",
                "b4",
                "w7",
                "w8",
                "w9",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w2", "w3", "w4"],
                        ["t2", "t3", "t4"],
                        ["b2", "b3", "b4"],
                        ["w7", "w8", "w9"],
                    ],
                }
            ],
        }
    )

    assert "mixed_triple_chow" in result["fan_keys"]


def test_evaluate_fan_context_scores_short_straight() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "dragon_triplet_count": 0,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
            },
            "tile_keys": [
                "w1",
                "w2",
                "w3",
                "w4",
                "w5",
                "w6",
                "t2",
                "t3",
                "t4",
                "b2",
                "b3",
                "b4",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w1", "w2", "w3"],
                        ["w4", "w5", "w6"],
                        ["t2", "t3", "t4"],
                        ["b2", "b3", "b4"],
                    ],
                }
            ],
        }
    )

    assert "short_straight" in result["fan_keys"]


def test_evaluate_fan_context_scores_one_voided_suit() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "dragon_triplet_count": 0,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
            },
            "tile_keys": [
                "w1",
                "w2",
                "w3",
                "w4",
                "w5",
                "w6",
                "w7",
                "w8",
                "w9",
                "t2",
                "t3",
                "t4",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w1", "w2", "w3"],
                        ["w4", "w5", "w6"],
                        ["w7", "w8", "w9"],
                        ["t2", "t3", "t4"],
                    ],
                }
            ],
        }
    )

    assert "one_voided_suit" in result["fan_keys"]


def test_evaluate_fan_context_scores_no_honors() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "dragon_triplet_count": 0,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
            },
            "tile_keys": [
                "w1",
                "w1",
                "w1",
                "w4",
                "w5",
                "w6",
                "w7",
                "w8",
                "w9",
                "t2",
                "t3",
                "t4",
                "b5",
                "b5",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "b5",
                    "melds": [
                        ["w1", "w1", "w1"],
                        ["w4", "w5", "w6"],
                        ["w7", "w8", "w9"],
                        ["t2", "t3", "t4"],
                    ],
                }
            ],
        }
    )

    assert "no_honours" in result["fan_keys"]


def test_evaluate_fan_context_scores_mixed_straight() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "dragon_triplet_count": 0,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
            },
            "tile_keys": [
                "w1",
                "w2",
                "w3",
                "t4",
                "t5",
                "t6",
                "b7",
                "b8",
                "b9",
                "w5",
                "w6",
                "w7",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w1", "w2", "w3"],
                        ["t4", "t5", "t6"],
                        ["b7", "b8", "b9"],
                        ["w5", "w6", "w7"],
                    ],
                }
            ],
        }
    )

    assert "mixed_straight" in result["fan_keys"]


def test_evaluate_fan_context_scores_mixed_shifted_chows() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "dragon_triplet_count": 0,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
            },
            "tile_keys": [
                "w2",
                "w3",
                "w4",
                "t3",
                "t4",
                "t5",
                "b4",
                "b5",
                "b6",
                "w7",
                "w8",
                "w9",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w2", "w3", "w4"],
                        ["t3", "t4", "t5"],
                        ["b4", "b5", "b6"],
                        ["w7", "w8", "w9"],
                    ],
                }
            ],
        }
    )

    assert "mixed_shifted_chows" in result["fan_keys"]


def test_evaluate_fan_context_scores_pure_shifted_chows() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "dragon_triplet_count": 0,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
            },
            "tile_keys": [
                "w2",
                "w3",
                "w4",
                "w3",
                "w4",
                "w5",
                "w4",
                "w5",
                "w6",
                "t7",
                "t8",
                "t9",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w2", "w3", "w4"],
                        ["w3", "w4", "w5"],
                        ["w4", "w5", "w6"],
                        ["t7", "t8", "t9"],
                    ],
                }
            ],
        }
    )

    assert "pure_shifted_chows" in result["fan_keys"]


def test_evaluate_fan_context_scores_pure_triple_chow() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "dragon_triplet_count": 0,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
            },
            "tile_keys": [
                "w2",
                "w3",
                "w4",
                "w2",
                "w3",
                "w4",
                "w2",
                "w3",
                "w4",
                "t7",
                "t8",
                "t9",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w2", "w3", "w4"],
                        ["w2", "w3", "w4"],
                        ["w2", "w3", "w4"],
                        ["t7", "t8", "t9"],
                    ],
                }
            ],
        }
    )

    assert "pure_triple_chow" in result["fan_keys"]


def test_evaluate_fan_context_scores_triple_pung() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "dragon_triplet_count": 0,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
                "pung_hand": True,
            },
            "tile_keys": [
                "w2",
                "w2",
                "w2",
                "t2",
                "t2",
                "t2",
                "b2",
                "b2",
                "b2",
                "w7",
                "w7",
                "w7",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w2", "w2", "w2"],
                        ["t2", "t2", "t2"],
                        ["b2", "b2", "b2"],
                        ["w7", "w7", "w7"],
                    ],
                }
            ],
        }
    )

    assert "triple_pung" in result["fan_keys"]


def test_evaluate_fan_context_scores_mixed_shifted_pungs() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "dragon_triplet_count": 0,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
                "pung_hand": True,
            },
            "tile_keys": [
                "w2",
                "w2",
                "w2",
                "t3",
                "t3",
                "t3",
                "b4",
                "b4",
                "b4",
                "w7",
                "w7",
                "w7",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w2", "w2", "w2"],
                        ["t3", "t3", "t3"],
                        ["b4", "b4", "b4"],
                        ["w7", "w7", "w7"],
                    ],
                }
            ],
        }
    )

    assert "mixed_shifted_pungs" in result["fan_keys"]


def test_evaluate_fan_context_scores_pure_shifted_pungs() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "dragon_triplet_count": 0,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
                "pung_hand": True,
            },
            "tile_keys": [
                "w2",
                "w2",
                "w2",
                "w3",
                "w3",
                "w3",
                "w4",
                "w4",
                "w4",
                "t7",
                "t7",
                "t7",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w2", "w2", "w2"],
                        ["w3", "w3", "w3"],
                        ["w4", "w4", "w4"],
                        ["t7", "t7", "t7"],
                    ],
                }
            ],
        }
    )

    assert "pure_shifted_pungs" in result["fan_keys"]


def test_evaluate_fan_context_scores_all_chows() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "incoming_tile": "b6",
            "features": {
                "dragon_triplet_count": 0,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
                "pung_hand": False,
            },
            "tile_keys": [
                "w1",
                "w2",
                "w3",
                "w4",
                "w5",
                "w6",
                "t2",
                "t3",
                "t4",
                "b4",
                "b5",
                "b6",
                "t8",
                "t8",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "t8",
                    "melds": [
                        ["w1", "w2", "w3"],
                        ["w4", "w5", "w6"],
                        ["t2", "t3", "t4"],
                        ["b4", "b5", "b6"],
                    ],
                }
            ],
        }
    )

    assert "all_chows" in result["fan_keys"]


def test_evaluate_fan_context_scores_edge_wait() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "incoming_tile": "w3",
            "features": {
                "dragon_triplet_count": 0,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
                "pung_hand": False,
            },
            "tile_keys": [
                "w1",
                "w2",
                "w3",
                "w4",
                "w5",
                "w6",
                "t2",
                "t3",
                "t4",
                "b4",
                "b5",
                "b6",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w1", "w2", "w3"],
                        ["w4", "w5", "w6"],
                        ["t2", "t3", "t4"],
                        ["b4", "b5", "b6"],
                    ],
                }
            ],
        }
    )

    assert "edge_wait" in result["fan_keys"]


def test_evaluate_fan_context_scores_closed_wait() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "incoming_tile": "w2",
            "features": {
                "dragon_triplet_count": 0,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
                "pung_hand": False,
            },
            "tile_keys": [
                "w1",
                "w2",
                "w3",
                "w4",
                "w5",
                "w6",
                "t2",
                "t3",
                "t4",
                "b4",
                "b5",
                "b6",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w1", "w2", "w3"],
                        ["w4", "w5", "w6"],
                        ["t2", "t3", "t4"],
                        ["b4", "b5", "b6"],
                    ],
                }
            ],
        }
    )

    assert "closed_wait" in result["fan_keys"]


def test_evaluate_fan_context_scores_single_wait() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "incoming_tile": "red",
            "features": {
                "dragon_triplet_count": 0,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
                "pung_hand": False,
            },
            "tile_keys": [
                "w1",
                "w2",
                "w3",
                "w4",
                "w5",
                "w6",
                "t2",
                "t3",
                "t4",
                "b4",
                "b5",
                "b6",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w1", "w2", "w3"],
                        ["w4", "w5", "w6"],
                        ["t2", "t3", "t4"],
                        ["b4", "b5", "b6"],
                    ],
                }
            ],
        }
    )

    assert "single_wait" in result["fan_keys"]


def test_evaluate_fan_context_does_not_score_wait_fan_on_multi_wait() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "incoming_tile": "w1",
            "features": {
                "dragon_triplet_count": 0,
                "seat_wind_triplet": False,
                "round_wind_triplet": False,
                "terminal_triplet_count": 0,
                "pung_hand": False,
            },
            "tile_keys": [
                "w1",
                "w1",
                "w2",
                "w3",
                "w4",
                "w6",
                "w7",
                "w8",
                "t2",
                "t3",
                "t4",
                "b2",
                "b3",
                "b4",
            ],
            "concealed_tile_keys": [
                "w1",
                "w2",
                "w3",
                "w4",
                "w6",
                "w7",
                "w8",
                "t2",
                "t3",
                "t4",
                "b2",
                "b3",
                "b4",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "w1",
                    "melds": [
                        ["w2", "w3", "w4"],
                        ["w1", "w2", "w3"],
                        ["w6", "w7", "w8"],
                        ["t2", "t3", "t4"],
                    ],
                },
                {
                    "kind": "standard",
                    "pair": "w4",
                    "melds": [
                        ["w1", "w2", "w3"],
                        ["w1", "w2", "w3"],
                        ["w6", "w7", "w8"],
                        ["t2", "t3", "t4"],
                    ],
                },
            ],
        }
    )

    assert "single_wait" not in result["fan_keys"]
    assert "edge_wait" not in result["fan_keys"]
    assert "closed_wait" not in result["fan_keys"]


def test_evaluate_fan_context_scores_concealed_kong() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "self_draw",
            "winner_seat": 0,
            "discarder_seat": None,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {"concealed_hand": True},
            "kong_entries": [
                {"kong_type": "concealed_kong", "actor_seat": 0, "payer_seats": [1, 2, 3]},
            ],
            "tile_keys": ["w1"] * 14,
            "meld_tile_key_groups": [],
            "decompositions": [],
        }
    )

    assert "concealed_kong" in result["fan_keys"]


def test_evaluate_fan_context_scores_two_concealed_kongs() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "self_draw",
            "winner_seat": 0,
            "discarder_seat": None,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {"concealed_hand": True},
            "kong_entries": [
                {"kong_type": "concealed_kong", "actor_seat": 0, "payer_seats": [1, 2, 3]},
                {"kong_type": "concealed_kong", "actor_seat": 0, "payer_seats": [1, 2, 3]},
            ],
            "tile_keys": ["w1"] * 14,
            "meld_tile_key_groups": [],
            "decompositions": [],
        }
    )

    assert "two_concealed_kongs" in result["fan_keys"]


def test_evaluate_fan_context_scores_two_melded_kongs() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {"concealed_hand": False},
            "kong_entries": [
                {"kong_type": "exposed_kong", "actor_seat": 0, "payer_seats": [1]},
                {"kong_type": "add_kong", "actor_seat": 0, "payer_seats": [2]},
            ],
            "tile_keys": ["w1"] * 14,
            "meld_tile_key_groups": [],
            "decompositions": [],
        }
    )

    assert "two_melded_kongs" in result["fan_keys"]


def test_evaluate_fan_context_scores_all_types() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "w1",
                "w2",
                "w3",
                "t4",
                "t5",
                "t6",
                "b7",
                "b8",
                "b9",
                "east",
                "east",
                "east",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [],
        }
    )

    assert "all_types" in result["fan_keys"]


def test_evaluate_fan_context_scores_all_fives() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "w3",
                "w4",
                "w5",
                "w4",
                "w5",
                "w6",
                "t5",
                "t5",
                "t5",
                "b4",
                "b5",
                "b6",
                "w5",
                "w5",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "w5",
                    "melds": [
                        ["w3", "w4", "w5"],
                        ["w4", "w5", "w6"],
                        ["t5", "t5", "t5"],
                        ["b4", "b5", "b6"],
                    ],
                }
            ],
        }
    )

    assert "all_fives" in result["fan_keys"]


def test_evaluate_fan_context_scores_upper_four() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "w6",
                "w7",
                "w8",
                "w7",
                "w8",
                "w9",
                "t6",
                "t7",
                "t8",
                "b7",
                "b8",
                "b9",
                "w9",
                "w9",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [],
        }
    )

    assert "upper_four" in result["fan_keys"]


def test_evaluate_fan_context_scores_lower_four() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "w1",
                "w2",
                "w3",
                "w2",
                "w3",
                "w4",
                "t1",
                "t2",
                "t3",
                "b2",
                "b3",
                "b4",
                "w4",
                "w4",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [],
        }
    )

    assert "lower_four" in result["fan_keys"]


def test_evaluate_fan_context_scores_middle_tiles() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "w4",
                "w5",
                "w6",
                "w4",
                "w5",
                "w6",
                "t4",
                "t5",
                "t6",
                "b4",
                "b5",
                "b6",
                "w5",
                "w5",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [],
        }
    )

    assert "middle_tiles" in result["fan_keys"]


def test_evaluate_fan_context_scores_pure_double_chow() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "w2",
                "w3",
                "w4",
                "w2",
                "w3",
                "w4",
                "t2",
                "t3",
                "t4",
                "b7",
                "b8",
                "b9",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w2", "w3", "w4"],
                        ["w2", "w3", "w4"],
                        ["t2", "t3", "t4"],
                        ["b7", "b8", "b9"],
                    ],
                }
            ],
        }
    )

    assert "pure_double_chow" in result["fan_keys"]


def test_evaluate_fan_context_scores_mixed_double_chow() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "w2",
                "w3",
                "w4",
                "t2",
                "t3",
                "t4",
                "b4",
                "b5",
                "b6",
                "b7",
                "b8",
                "b9",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w2", "w3", "w4"],
                        ["t2", "t3", "t4"],
                        ["b4", "b5", "b6"],
                        ["b7", "b8", "b9"],
                    ],
                }
            ],
        }
    )

    assert "mixed_double_chow" in result["fan_keys"]


def test_evaluate_fan_context_scores_tile_hog() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "w2",
                "w2",
                "w2",
                "w2",
                "t2",
                "t3",
                "t4",
                "b4",
                "b5",
                "b6",
                "w7",
                "w8",
                "w9",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [],
            "kong_entries": [],
        }
    )

    assert "tile_hog" in result["fan_keys"]


def test_evaluate_fan_context_scores_double_pung() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "pung_hand": True,
            },
            "tile_keys": [
                "w2",
                "w2",
                "w2",
                "t2",
                "t2",
                "t2",
                "b4",
                "b4",
                "b4",
                "w7",
                "w7",
                "w7",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w2", "w2", "w2"],
                        ["t2", "t2", "t2"],
                        ["b4", "b4", "b4"],
                        ["w7", "w7", "w7"],
                    ],
                }
            ],
        }
    )

    assert "double_pung" in result["fan_keys"]


def test_evaluate_fan_context_scores_outside_hand() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "w1",
                "w2",
                "w3",
                "w7",
                "w8",
                "w9",
                "t1",
                "t2",
                "t3",
                "east",
                "east",
                "east",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w1", "w2", "w3"],
                        ["w7", "w8", "w9"],
                        ["t1", "t2", "t3"],
                        ["east", "east", "east"],
                    ],
                }
            ],
        }
    )

    assert "outside_hand" in result["fan_keys"]


def test_evaluate_fan_context_scores_melded_kong() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": ["w1"] * 14,
            "meld_tile_key_groups": [],
            "decompositions": [],
            "kong_entries": [
                {"kong_type": "exposed_kong", "actor_seat": 0, "payer_seats": [1]},
            ],
        }
    )

    assert "melded_kong" in result["fan_keys"]


def test_evaluate_fan_context_scores_fully_concealed_hand() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "self_draw",
            "winner_seat": 0,
            "discarder_seat": None,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "concealed_hand": True,
            },
            "tile_keys": [
                "w1",
                "w2",
                "w3",
                "w4",
                "w5",
                "w6",
                "t2",
                "t3",
                "t4",
                "b4",
                "b5",
                "b6",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w1", "w2", "w3"],
                        ["w4", "w5", "w6"],
                        ["t2", "t3", "t4"],
                        ["b4", "b5", "b6"],
                    ],
                }
            ],
        }
    )

    assert "fully_concealed_hand" in result["fan_keys"]


def test_evaluate_fan_context_scores_two_terminal_chows() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "w1",
                "w2",
                "w3",
                "w7",
                "w8",
                "w9",
                "t2",
                "t3",
                "t4",
                "b4",
                "b5",
                "b6",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w1", "w2", "w3"],
                        ["w7", "w8", "w9"],
                        ["t2", "t3", "t4"],
                        ["b4", "b5", "b6"],
                    ],
                }
            ],
        }
    )

    assert "two_terminal_chows" in result["fan_keys"]


def test_evaluate_fan_context_scores_three_suited_terminal_chows() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "w1",
                "w2",
                "w3",
                "w7",
                "w8",
                "w9",
                "t1",
                "t2",
                "t3",
                "t7",
                "t8",
                "t9",
                "b5",
                "b5",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "b5",
                    "melds": [
                        ["w1", "w2", "w3"],
                        ["w7", "w8", "w9"],
                        ["t1", "t2", "t3"],
                        ["t7", "t8", "t9"],
                    ],
                }
            ],
        }
    )

    assert "three_suited_terminal_chows" in result["fan_keys"]


def test_evaluate_fan_context_scores_pure_terminal_chows() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "w1",
                "w2",
                "w3",
                "w1",
                "w2",
                "w3",
                "w7",
                "w8",
                "w9",
                "w7",
                "w8",
                "w9",
                "w5",
                "w5",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "w5",
                    "melds": [
                        ["w1", "w2", "w3"],
                        ["w1", "w2", "w3"],
                        ["w7", "w8", "w9"],
                        ["w7", "w8", "w9"],
                    ],
                }
            ],
        }
    )

    assert "pure_terminal_chows" in result["fan_keys"]


def test_evaluate_fan_context_scores_reversible_tiles() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "b2",
                "b2",
                "b2",
                "b4",
                "b5",
                "b6",
                "t1",
                "t2",
                "t3",
                "t4",
                "t5",
                "t5",
                "white",
                "white",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [],
        }
    )

    assert "reversible_tiles" in result["fan_keys"]


def test_evaluate_fan_context_scores_melded_hand() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "concealed_hand": False,
            },
            "tile_keys": [
                "w1",
                "w1",
                "w1",
                "t2",
                "t2",
                "t2",
                "b3",
                "b3",
                "b3",
                "east",
                "east",
                "east",
                "red",
                "red",
            ],
            "concealed_tile_keys": ["red", "red"],
            "meld_tile_key_groups": [
                ["w1", "w1", "w1"],
                ["t2", "t2", "t2"],
                ["b3", "b3", "b3"],
                ["east", "east", "east"],
            ],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w1", "w1", "w1"],
                        ["t2", "t2", "t2"],
                        ["b3", "b3", "b3"],
                        ["east", "east", "east"],
                    ],
                }
            ],
        }
    )

    assert "melded_hand" in result["fan_keys"]


def test_evaluate_fan_context_scores_upper_tiles() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "w7",
                "w8",
                "w9",
                "w7",
                "w8",
                "w9",
                "t7",
                "t8",
                "t9",
                "b7",
                "b8",
                "b9",
                "w9",
                "w9",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [],
        }
    )

    assert "upper_tiles" in result["fan_keys"]


def test_evaluate_fan_context_scores_lower_tiles() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "w1",
                "w2",
                "w3",
                "w1",
                "w2",
                "w3",
                "t1",
                "t2",
                "t3",
                "b1",
                "b2",
                "b3",
                "w3",
                "w3",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [],
        }
    )

    assert "lower_tiles" in result["fan_keys"]


def test_evaluate_fan_context_scores_seven_shifted_pairs() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "self_draw",
            "winner_seat": 0,
            "discarder_seat": None,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "seven_pairs": True,
            },
            "tile_keys": [
                "w1",
                "w1",
                "w2",
                "w2",
                "w3",
                "w3",
                "w4",
                "w4",
                "w5",
                "w5",
                "w6",
                "w6",
                "w7",
                "w7",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "seven_pairs",
                    "pairs": ["w1", "w2", "w3", "w4", "w5", "w6", "w7"],
                }
            ],
        }
    )

    assert "seven_shifted_pairs" in result["fan_keys"]


def test_evaluate_fan_context_scores_four_pure_shifted_chows() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "w1",
                "w2",
                "w3",
                "w2",
                "w3",
                "w4",
                "w3",
                "w4",
                "w5",
                "w4",
                "w5",
                "w6",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w1", "w2", "w3"],
                        ["w2", "w3", "w4"],
                        ["w3", "w4", "w5"],
                        ["w4", "w5", "w6"],
                    ],
                }
            ],
        }
    )

    assert "four_pure_shifted_chows" in result["fan_keys"]


def test_evaluate_fan_context_scores_quadruple_chows() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "w5",
                "w6",
                "w7",
                "w5",
                "w6",
                "w7",
                "w5",
                "w6",
                "w7",
                "w5",
                "w6",
                "w7",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w5", "w6", "w7"],
                        ["w5", "w6", "w7"],
                        ["w5", "w6", "w7"],
                        ["w5", "w6", "w7"],
                    ],
                }
            ],
        }
    )

    assert "quadruple_chow" in result["fan_keys"]


def test_evaluate_fan_context_scores_nine_gates() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "self_draw",
            "winner_seat": 0,
            "discarder_seat": None,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "w1",
                "w1",
                "w1",
                "w2",
                "w3",
                "w4",
                "w5",
                "w5",
                "w6",
                "w7",
                "w8",
                "w9",
                "w9",
                "w9",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [],
        }
    )

    assert "nine_gates" in result["fan_keys"]


def test_evaluate_fan_context_scores_two_concealed_pungs() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "self_draw",
            "winner_seat": 0,
            "discarder_seat": None,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "w1",
                "w1",
                "w1",
                "w2",
                "w2",
                "w2",
                "t1",
                "t2",
                "t3",
                "b4",
                "b5",
                "b6",
                "red",
                "red",
            ],
            "concealed_tile_keys": [
                "w1",
                "w1",
                "w1",
                "w2",
                "w2",
                "w2",
                "t1",
                "t2",
                "t3",
                "b4",
                "b5",
                "b6",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w1", "w1", "w1"],
                        ["w2", "w2", "w2"],
                        ["t1", "t2", "t3"],
                        ["b4", "b5", "b6"],
                    ],
                }
            ],
            "kong_entries": [],
        }
    )

    assert "two_concealed_pungs" in result["fan_keys"]


def test_evaluate_fan_context_scores_three_concealed_pungs() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "self_draw",
            "winner_seat": 0,
            "discarder_seat": None,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "w1",
                "w1",
                "w1",
                "w2",
                "w2",
                "w2",
                "w3",
                "w3",
                "w3",
                "t1",
                "t2",
                "t3",
                "red",
                "red",
            ],
            "concealed_tile_keys": [
                "w1",
                "w1",
                "w1",
                "w2",
                "w2",
                "w2",
                "w3",
                "w3",
                "w3",
                "t1",
                "t2",
                "t3",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w1", "w1", "w1"],
                        ["w2", "w2", "w2"],
                        ["w3", "w3", "w3"],
                        ["t1", "t2", "t3"],
                    ],
                }
            ],
            "kong_entries": [],
        }
    )

    assert "three_concealed_pungs" in result["fan_keys"]


def test_evaluate_fan_context_scores_pung_of_terminals_or_honours() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {
                "terminal_triplet_count": 1,
                "non_seat_non_round_wind_triplet_count": 1,
            },
            "tile_keys": [
                "w1",
                "w1",
                "w1",
                "south",
                "south",
                "south",
                "t2",
                "t3",
                "t4",
                "b5",
                "b6",
                "b7",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w1", "w1", "w1"],
                        ["south", "south", "south"],
                        ["t2", "t3", "t4"],
                        ["b5", "b6", "b7"],
                    ],
                }
            ],
        }
    )

    assert result["fan_keys"].count("pung_of_terminals_or_honours") == 2


def test_evaluate_fan_context_scores_last_tile() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "w1",
                "w2",
                "w3",
                "w4",
                "w5",
                "w6",
                "t2",
                "t3",
                "t4",
                "b4",
                "b5",
                "b6",
                "w5",
                "w5",
            ],
            "concealed_tile_keys": [
                "w1",
                "w2",
                "w3",
                "w4",
                "w5",
                "w6",
                "t2",
                "t3",
                "t4",
                "b4",
                "b5",
                "b6",
                "w5",
            ],
            "meld_tile_key_groups": [],
            "incoming_tile": "w5",
            "visible_tile_keys": ["w5", "w5", "w5"],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "w5",
                    "melds": [
                        ["w1", "w2", "w3"],
                        ["w4", "w5", "w6"],
                        ["t2", "t3", "t4"],
                        ["b4", "b5", "b6"],
                    ],
                }
            ],
        }
    )

    assert "last_tile" in result["fan_keys"]


def test_evaluate_fan_context_scores_robbing_the_kong() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {"robbing_the_kong": True},
            "features": {},
            "tile_keys": [
                "w1",
                "w2",
                "w3",
                "w4",
                "w5",
                "w6",
                "t2",
                "t3",
                "t4",
                "b4",
                "b5",
                "b6",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w1", "w2", "w3"],
                        ["w4", "w5", "w6"],
                        ["t2", "t3", "t4"],
                        ["b4", "b5", "b6"],
                    ],
                }
            ],
        }
    )

    assert "robbing_the_kong" in result["fan_keys"]


def test_evaluate_fan_context_scores_knitted_straight() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "w1",
                "w4",
                "w7",
                "t2",
                "t5",
                "t8",
                "b3",
                "b6",
                "b9",
                "w4",
                "w5",
                "w6",
                "red",
                "red",
            ],
            "concealed_tile_keys": [
                "w1",
                "w4",
                "w7",
                "t2",
                "t5",
                "t8",
                "b3",
                "b6",
                "b9",
                "w4",
                "w5",
                "w6",
                "red",
            ],
            "meld_tile_key_groups": [],
            "incoming_tile": "red",
        }
    )

    assert "knitted_straight" in result["fan_keys"]


def test_evaluate_fan_context_scores_lesser_honours_and_knitted_tiles() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "self_draw",
            "winner_seat": 0,
            "discarder_seat": None,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "w1",
                "w4",
                "w7",
                "t2",
                "t5",
                "t8",
                "b3",
                "b6",
                "b9",
                "east",
                "south",
                "west",
                "north",
                "red",
            ],
            "concealed_tile_keys": [
                "w1",
                "w4",
                "w7",
                "t2",
                "t5",
                "t8",
                "b3",
                "b6",
                "b9",
                "east",
                "south",
                "west",
                "north",
                "red",
            ],
            "meld_tile_key_groups": [],
        }
    )

    assert "lesser_honours_and_knitted_tiles" in result["fan_keys"]


def test_evaluate_fan_context_scores_greater_honours_and_knitted_tiles() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "self_draw",
            "winner_seat": 0,
            "discarder_seat": None,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "w1",
                "w4",
                "w7",
                "t2",
                "t5",
                "t8",
                "b3",
                "east",
                "south",
                "west",
                "north",
                "red",
                "green",
                "white",
            ],
            "concealed_tile_keys": [
                "w1",
                "w4",
                "w7",
                "t2",
                "t5",
                "t8",
                "b3",
                "east",
                "south",
                "west",
                "north",
                "red",
                "green",
                "white",
            ],
            "meld_tile_key_groups": [],
        }
    )

    assert "greater_honours_and_knitted_tiles" in result["fan_keys"]


def test_evaluate_fan_context_scores_three_kongs() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "self_draw",
            "winner_seat": 0,
            "discarder_seat": None,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": ["w1"] * 14,
            "meld_tile_key_groups": [],
            "decompositions": [],
            "kong_entries": [
                {"kong_type": "concealed_kong", "actor_seat": 0, "payer_seats": [1, 2, 3]},
                {"kong_type": "exposed_kong", "actor_seat": 0, "payer_seats": [1]},
                {"kong_type": "add_kong", "actor_seat": 0, "payer_seats": [2]},
            ],
        }
    )

    assert "three_kongs" in result["fan_keys"]


def test_evaluate_fan_context_scores_four_kongs() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "self_draw",
            "winner_seat": 0,
            "discarder_seat": None,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": ["w1"] * 14,
            "meld_tile_key_groups": [],
            "decompositions": [],
            "kong_entries": [
                {"kong_type": "concealed_kong", "actor_seat": 0, "payer_seats": [1, 2, 3]},
                {"kong_type": "exposed_kong", "actor_seat": 0, "payer_seats": [1]},
                {"kong_type": "add_kong", "actor_seat": 0, "payer_seats": [2]},
                {"kong_type": "concealed_kong", "actor_seat": 0, "payer_seats": [1, 2, 3]},
            ],
        }
    )

    assert "four_kongs" in result["fan_keys"]


def test_evaluate_fan_context_scores_four_pure_shifted_pungs() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "self_draw",
            "winner_seat": 0,
            "discarder_seat": None,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {"pung_hand": True},
            "tile_keys": [
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
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w2", "w2", "w2"],
                        ["w3", "w3", "w3"],
                        ["w4", "w4", "w4"],
                        ["w5", "w5", "w5"],
                    ],
                }
            ],
            "kong_entries": [],
        }
    )

    assert "four_pure_shifted_pungs" in result["fan_keys"]


def test_evaluate_fan_context_scores_four_concealed_pungs() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "self_draw",
            "winner_seat": 0,
            "discarder_seat": None,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {"pung_hand": True},
            "tile_keys": [
                "w1",
                "w1",
                "w1",
                "w2",
                "w2",
                "w2",
                "w3",
                "w3",
                "w3",
                "w4",
                "w4",
                "w4",
                "red",
                "red",
            ],
            "concealed_tile_keys": [
                "w1",
                "w1",
                "w1",
                "w2",
                "w2",
                "w2",
                "w3",
                "w3",
                "w3",
                "w4",
                "w4",
                "w4",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w1", "w1", "w1"],
                        ["w2", "w2", "w2"],
                        ["w3", "w3", "w3"],
                        ["w4", "w4", "w4"],
                    ],
                }
            ],
            "kong_entries": [],
        }
    )

    assert "four_concealed_pungs" in result["fan_keys"]


def test_evaluate_fan_context_scores_chicken_hand() -> None:
    result = evaluate_fan_context(
        {
            "win_type": "discard",
            "winner_seat": 0,
            "discarder_seat": 1,
            "seat_count": 4,
            "flower_count": 0,
            "timing": {},
            "features": {},
            "tile_keys": [
                "w1",
                "w2",
                "w3",
                "t4",
                "t5",
                "t6",
                "b3",
                "b4",
                "b5",
                "w6",
                "w7",
                "w8",
                "red",
                "red",
            ],
            "meld_tile_key_groups": [],
            "decompositions": [
                {
                    "kind": "standard",
                    "pair": "red",
                    "melds": [
                        ["w1", "w2", "w3"],
                        ["t4", "t5", "t6"],
                        ["b3", "b4", "b5"],
                        ["w6", "w7", "w8"],
                    ],
                }
            ],
        }
    )

    assert "chicken_hand" in result["fan_keys"]
