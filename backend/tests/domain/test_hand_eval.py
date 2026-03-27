from app.domain.hand_eval import decompose_winning_hand, is_winning_hand


def test_standard_four_melds_plus_pair_is_winning():
    hand = [
        "w1",
        "w1",
        "w1",
        "w2",
        "w3",
        "w4",
        "t2",
        "t3",
        "t4",
        "b5",
        "b6",
        "b7",
        "east",
        "east",
    ]
    assert is_winning_hand(hand) is True


def test_seven_pairs_is_winning():
    hand = [
        "w1",
        "w1",
        "w9",
        "w9",
        "t1",
        "t1",
        "t9",
        "t9",
        "b1",
        "b1",
        "b9",
        "b9",
        "red",
        "red",
    ]
    assert is_winning_hand(hand) is True


def test_thirteen_orphans_is_winning():
    hand = [
        "w1",
        "w9",
        "t1",
        "t9",
        "b1",
        "b9",
        "east",
        "south",
        "west",
        "north",
        "red",
        "green",
        "white",
        "white",
    ]
    assert is_winning_hand(hand) is True


def test_knitted_straight_is_winning():
    hand = [
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
    ]
    assert is_winning_hand(hand) is True
    assert "knitted_straight" in {decomposition["kind"] for decomposition in decompose_winning_hand(hand)}


def test_lesser_honours_and_knitted_tiles_is_winning():
    hand = [
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
    ]
    assert is_winning_hand(hand) is True
    kinds = {decomposition["kind"] for decomposition in decompose_winning_hand(hand)}
    assert "lesser_honours_and_knitted_tiles" in kinds


def test_greater_honours_and_knitted_tiles_is_winning():
    hand = [
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
    ]
    assert is_winning_hand(hand) is True
    assert "greater_honours_and_knitted_tiles" in {
        decomposition["kind"] for decomposition in decompose_winning_hand(hand)
    }


def test_open_hand_with_melds_is_winning():
    hand = [
        "w1",
        "w1",
        "w1",
        "t2",
        "t3",
        "t4",
        "b5",
        "b6",
        "b7",
        "east",
        "east",
        "east",
        "red",
        "red",
    ]
    assert is_winning_hand(hand) is True


def test_decompose_winning_hand_returns_standard_hand_structure():
    hand = [
        "w1",
        "w1",
        "w1",
        "w2",
        "w3",
        "w4",
        "t2",
        "t3",
        "t4",
        "b5",
        "b6",
        "b7",
        "east",
        "east",
    ]

    decompositions = decompose_winning_hand(hand)

    standard = next(
        decomposition for decomposition in decompositions if decomposition["kind"] == "standard"
    )
    assert standard["pair"] == "east"
    assert ["w1", "w1", "w1"] in standard["melds"]
    assert ["w2", "w3", "w4"] in standard["melds"]


def test_decompose_winning_hand_returns_multiple_candidate_kinds_for_ambiguous_hand():
    hand = [
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
    ]

    decompositions = decompose_winning_hand(hand)

    assert {decomposition["kind"] for decomposition in decompositions} == {
        "seven_pairs",
        "standard",
    }
