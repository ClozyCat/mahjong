import importlib

import app.domain.fan_eval as fan_eval_module
from app.domain.fan_eval import evaluate_fans


def test_evaluate_fans_delegates_to_fan_engine(monkeypatch) -> None:
    engine_module = importlib.import_module("app.domain.fans.engine")
    expected = {
        "fan_total": 0,
        "fan_keys": [],
        "fan_breakdown": [],
        "score_delta": {
            "provisional": True,
            "fan_total": 0,
            "fan_delta_by_seat": {0: 0, 1: 0, 2: 0, 3: 0},
            "kong_delta_by_seat": {0: 0, 1: 0, 2: 0, 3: 0},
            "total_delta_by_seat": {0: 0, 1: 0, 2: 0, 3: 0},
        },
        "kong_score_detail": [],
        "provisional": True,
    }
    captured: dict = {}

    def fake_evaluate_fan_context(context: dict) -> dict:
        captured.update(context)
        return expected

    monkeypatch.setattr(engine_module, "evaluate_fan_context", fake_evaluate_fan_context)

    result = fan_eval_module.evaluate_fans(
        win_type="self_draw",
        winner_seat=0,
        discarder_seat=None,
        flower_count=0,
        seat_count=4,
        features={},
        kong_entries=[],
    )

    assert result == expected
    assert captured["win_type"] == "self_draw"
    assert captured["winner_seat"] == 0


def test_evaluate_fans_awards_common_features_and_flowers() -> None:
    result = evaluate_fans(
        win_type="self_draw",
        winner_seat=0,
        discarder_seat=None,
        flower_count=2,
        seat_count=4,
        features={
            "concealed_hand": True,
            "pung_hand": True,
            "mixed_one_suit": True,
            "pure_one_suit": False,
            "seven_pairs": False,
        },
        kong_entries=[],
    )

    assert result["fan_keys"] == [
        "half_flush",
        "all_pungs",
        "fully_concealed_hand",
        "flower_tiles",
    ]
    assert result["fan_total"] == 18
    assert result["score_delta"]["fan_total"] == 18
    assert result["score_delta"]["fan_delta_by_seat"] == {0: 78, 1: -26, 2: -26, 3: -26}


def test_evaluate_fans_prefers_pure_one_suit_and_seven_pairs() -> None:
    result = evaluate_fans(
        win_type="discard",
        winner_seat=1,
        discarder_seat=3,
        flower_count=0,
        seat_count=4,
        features={
            "concealed_hand": True,
            "pung_hand": True,
            "mixed_one_suit": True,
            "pure_one_suit": True,
            "seven_pairs": True,
        },
        kong_entries=[],
    )

    assert result["fan_keys"] == ["full_flush", "seven_pairs"]
    assert result["fan_total"] == 48
    assert result["score_delta"]["fan_delta_by_seat"] == {0: -8, 1: 72, 2: -8, 3: -56}


def test_evaluate_fans_includes_visible_kong_score_detail() -> None:
    result = evaluate_fans(
        win_type="self_draw",
        winner_seat=0,
        discarder_seat=None,
        flower_count=0,
        seat_count=4,
        features={
            "concealed_hand": False,
            "pung_hand": False,
            "mixed_one_suit": False,
            "pure_one_suit": False,
            "seven_pairs": False,
        },
        kong_entries=[
            {"kong_type": "exposed_kong", "actor_seat": 0, "payer_seats": [2]},
            {"kong_type": "concealed_kong", "actor_seat": 0, "payer_seats": [1, 2, 3]},
            {"kong_type": "add_kong", "actor_seat": 0, "payer_seats": [3]},
        ],
    )

    assert [entry["kong_type"] for entry in result["kong_score_detail"]] == [
        "exposed_kong",
        "concealed_kong",
        "add_kong",
    ]
    assert "concealed_kong" in result["fan_keys"]
    assert "melded_kong" in result["fan_keys"]
    assert "two_melded_kongs" in result["fan_keys"]
    assert "three_kongs" in result["fan_keys"]
    assert result["score_delta"]["kong_delta_by_seat"] == {0: 5, 1: -1, 2: -2, 3: -2}
    assert result["score_delta"]["total_delta_by_seat"] == {0: 149, 1: -49, 2: -50, 3: -50}


def test_evaluate_fans_awards_ping_hu_yi_ban_gao_and_duan_yao() -> None:
    result = evaluate_fans(
        win_type="discard",
        winner_seat=1,
        discarder_seat=3,
        flower_count=0,
        seat_count=4,
        features={
            "concealed_hand": True,
            "pung_hand": False,
            "mixed_one_suit": False,
            "pure_one_suit": False,
            "seven_pairs": False,
            "ping_hu": True,
            "yi_ban_gao": True,
            "duan_yao": True,
            "hun_yao_jiu": False,
            "qing_yao_jiu": False,
        },
        kong_entries=[],
    )

    assert result["fan_keys"] == [
        "all_simples",
        "concealed_hand",
    ]


def test_evaluate_fans_uses_official_keys_instead_of_legacy_aliases() -> None:
    result = evaluate_fans(
        win_type="discard",
        winner_seat=0,
        discarder_seat=2,
        flower_count=0,
        seat_count=4,
        features={
            "concealed_hand": False,
            "pung_hand": True,
            "mixed_one_suit": False,
            "pure_one_suit": False,
            "seven_pairs": False,
            "ping_hu": False,
            "yi_ban_gao": False,
            "duan_yao": True,
            "hun_yao_jiu": True,
            "qing_yao_jiu": True,
        },
        kong_entries=[],
    )

    assert result["fan_keys"] == ["all_pungs"]


def test_evaluate_fans_does_not_combine_ping_hu_with_pung_hand() -> None:
    result = evaluate_fans(
        win_type="self_draw",
        winner_seat=0,
        discarder_seat=None,
        flower_count=0,
        seat_count=4,
        features={
            "concealed_hand": True,
            "pung_hand": True,
            "mixed_one_suit": False,
            "pure_one_suit": False,
            "seven_pairs": False,
            "ping_hu": True,
            "yi_ban_gao": False,
            "duan_yao": False,
            "hun_yao_jiu": False,
            "qing_yao_jiu": False,
        },
        kong_entries=[],
    )

    assert "ping_hu" not in result["fan_keys"]
    assert "all_pungs" in result["fan_keys"]


def test_evaluate_fans_allows_east_player_to_score_both_seat_and_round_wind() -> None:
    result = evaluate_fans(
        win_type="discard",
        winner_seat=0,
        discarder_seat=2,
        flower_count=0,
        seat_count=4,
        features={
            "concealed_hand": False,
            "pung_hand": True,
            "mixed_one_suit": False,
            "pure_one_suit": False,
            "seven_pairs": False,
            "ping_hu": False,
            "yi_ban_gao": False,
            "duan_yao": False,
            "hun_yao_jiu": False,
            "qing_yao_jiu": False,
            "seat_wind_triplet": True,
            "round_wind_triplet": True,
            "dragon_triplet_count": 0,
            "terminal_triplet_count": 0,
        },
        kong_entries=[],
    )

    assert "seat_wind" in result["fan_keys"]
    assert "prevalent_wind" in result["fan_keys"]


def test_evaluate_fans_counts_multiple_dragons_and_terminals() -> None:
    result = evaluate_fans(
        win_type="self_draw",
        winner_seat=1,
        discarder_seat=None,
        flower_count=0,
        seat_count=4,
        features={
            "concealed_hand": False,
            "pung_hand": True,
            "mixed_one_suit": False,
            "pure_one_suit": False,
            "seven_pairs": False,
            "ping_hu": False,
            "yi_ban_gao": False,
            "duan_yao": False,
            "hun_yao_jiu": False,
            "qing_yao_jiu": False,
            "seat_wind_triplet": False,
            "round_wind_triplet": False,
            "dragon_triplet_count": 2,
            "terminal_triplet_count": 2,
        },
        kong_entries=[],
    )

    assert result["fan_keys"].count("dragon_pung") == 2
    assert result["fan_keys"].count("pung_of_terminals_or_honours") == 2


def test_evaluate_fans_awards_gang_shang_hua() -> None:
    result = evaluate_fans(
        win_type="self_draw",
        winner_seat=0,
        discarder_seat=None,
        flower_count=0,
        seat_count=4,
        features={"concealed_hand": False},
        timing={
            "gang_shang_hua": True,
            "hai_di_lao_yue": True,
            "he_di_lao_yu": False,
        },
        kong_entries=[],
    )

    assert "out_with_replacement_tile" in result["fan_keys"]
    assert "last_tile_draw" not in result["fan_keys"]


def test_evaluate_fans_awards_hai_di_lao_yue() -> None:
    result = evaluate_fans(
        win_type="self_draw",
        winner_seat=0,
        discarder_seat=None,
        flower_count=0,
        seat_count=4,
        features={"concealed_hand": False},
        timing={
            "gang_shang_hua": False,
            "hai_di_lao_yue": True,
            "he_di_lao_yu": False,
        },
        kong_entries=[],
    )

    assert "last_tile_draw" in result["fan_keys"]


def test_evaluate_fans_awards_he_di_lao_yu() -> None:
    result = evaluate_fans(
        win_type="discard",
        winner_seat=1,
        discarder_seat=3,
        flower_count=0,
        seat_count=4,
        features={"concealed_hand": False},
        timing={
            "gang_shang_hua": False,
            "hai_di_lao_yue": False,
            "he_di_lao_yu": True,
        },
        kong_entries=[],
    )

    assert "last_tile_claim" in result["fan_keys"]


def test_evaluate_fans_scores_thirteen_orphans_as_limit_hand() -> None:
    result = evaluate_fans(
        win_type="self_draw",
        winner_seat=0,
        discarder_seat=None,
        flower_count=0,
        seat_count=4,
        features={
            "concealed_hand": True,
            "thirteen_orphans": True,
        },
        kong_entries=[],
    )

    assert "thirteen_orphans" in result["fan_keys"]
    assert result["fan_total"] >= 88
