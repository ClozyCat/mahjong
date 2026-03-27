import app.domain.hand_features as hand_features_module
from app.domain.hand_features import extract_hand_features


def test_extract_hand_features_detects_ping_hu_and_yi_ban_gao() -> None:
    features = extract_hand_features(
        concealed_tile_keys=[
            "w1",
            "w2",
            "w3",
            "w1",
            "w2",
            "w3",
            "t4",
            "t5",
            "t6",
            "b4",
            "b5",
            "b6",
            "t8",
            "t8",
        ],
        meld_tile_key_groups=[],
        incoming_tile=None,
    )

    assert features["ping_hu"] is True
    assert features["yi_ban_gao"] is True


def test_extract_hand_features_detects_duan_yao() -> None:
    features = extract_hand_features(
        concealed_tile_keys=[
            "w2",
            "w3",
            "w4",
            "w4",
            "w5",
            "w6",
            "t2",
            "t3",
            "t4",
            "b6",
            "b7",
            "b8",
            "t5",
            "t5",
        ],
        meld_tile_key_groups=[],
        incoming_tile=None,
    )

    assert features["duan_yao"] is True
    assert features["hun_yao_jiu"] is False
    assert features["qing_yao_jiu"] is False


def test_extract_hand_features_detects_hun_yao_jiu() -> None:
    features = extract_hand_features(
        concealed_tile_keys=[
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
        meld_tile_key_groups=[],
        incoming_tile=None,
    )

    assert features["hun_yao_jiu"] is True
    assert features["qing_yao_jiu"] is False


def test_extract_hand_features_detects_qing_yao_jiu() -> None:
    features = extract_hand_features(
        concealed_tile_keys=[
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
        meld_tile_key_groups=[],
        incoming_tile=None,
    )

    assert features["qing_yao_jiu"] is True
    assert features["hun_yao_jiu"] is False


def test_extract_hand_features_detects_seat_and_round_wind_triplets() -> None:
    features = extract_hand_features(
        concealed_tile_keys=[
            "east",
            "east",
            "east",
            "w2",
            "w3",
            "w4",
            "t2",
            "t3",
            "t4",
            "b2",
            "b3",
            "b4",
            "red",
            "red",
        ],
        meld_tile_key_groups=[],
        incoming_tile=None,
        seat_wind_key="east",
        round_wind_key="east",
    )

    assert features["seat_wind_triplet"] is True
    assert features["round_wind_triplet"] is True


def test_extract_hand_features_counts_dragons_and_terminals() -> None:
    features = extract_hand_features(
        concealed_tile_keys=[
            "red",
            "red",
            "red",
            "white",
            "white",
            "white",
            "w1",
            "w1",
            "w1",
            "t9",
            "t9",
            "t9",
            "east",
            "east",
        ],
        meld_tile_key_groups=[],
        incoming_tile=None,
        seat_wind_key="south",
        round_wind_key="east",
    )

    assert features["dragon_triplet_count"] == 2
    assert features["terminal_triplet_count"] == 2


def test_extract_hand_features_treats_kong_as_triplet_content() -> None:
    features = extract_hand_features(
        concealed_tile_keys=[
            "w2",
            "w3",
            "w4",
            "t2",
            "t3",
            "t4",
            "b2",
            "b3",
            "b4",
            "red",
            "red",
        ],
        meld_tile_key_groups=[
            ["east", "east", "east", "east"],
            ["green", "green", "green", "green"],
        ],
        incoming_tile=None,
        seat_wind_key="east",
        round_wind_key="east",
    )

    assert features["seat_wind_triplet"] is True
    assert features["round_wind_triplet"] is True
    assert features["dragon_triplet_count"] == 1


def test_extract_hand_features_can_use_precomputed_decompositions(monkeypatch) -> None:
    def fail_if_called(_tile_keys):
        raise AssertionError("expected precomputed decomposition to be used")

    monkeypatch.setattr(hand_features_module, "_decompose_standard_hand", fail_if_called)

    features = extract_hand_features(
        concealed_tile_keys=[
            "w1",
            "w2",
            "w3",
            "w1",
            "w2",
            "w3",
            "t4",
            "t5",
            "t6",
            "b4",
            "b5",
            "b6",
            "t8",
            "t8",
        ],
        meld_tile_key_groups=[],
        incoming_tile=None,
        decompositions=[
            {
                "kind": "standard",
                "pair": "t8",
                "melds": [
                    ["w1", "w2", "w3"],
                    ["w1", "w2", "w3"],
                    ["t4", "t5", "t6"],
                    ["b4", "b5", "b6"],
                ],
            }
        ],
    )

    assert features["ping_hu"] is True
    assert features["yi_ban_gao"] is True


def test_extract_hand_features_keeps_concealed_hand_when_only_kong_is_concealed() -> None:
    features = extract_hand_features(
        concealed_tile_keys=[
            "t2",
            "t3",
            "t4",
            "b2",
            "b3",
            "b4",
            "t5",
            "t6",
            "t7",
            "red",
            "red",
        ],
        meld_tile_key_groups=[
            ["w1", "w1", "w1", "w1"],
        ],
        meld_open_flags=[False],
        incoming_tile=None,
    )

    assert features["concealed_hand"] is True
