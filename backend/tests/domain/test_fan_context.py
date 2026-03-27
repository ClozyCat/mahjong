from app.domain.fans.context import build_fan_context


def test_build_fan_context_normalizes_scoring_inputs() -> None:
    context = build_fan_context(
        win_type="self_draw",
        winner_seat=0,
        discarder_seat=None,
        seat_count=4,
        flower_count=1,
        features={"concealed_hand": True},
        timing={"gang_shang_hua": False},
        tile_keys=[
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
        meld_tile_key_groups=[],
        seat_wind_key="east",
        round_wind_key="east",
        decompositions=[
            {"kind": "seven_pairs", "pairs": ["w1", "w2", "w3", "w4", "w5", "w6", "w7"]},
            {
                "kind": "standard",
                "pair": "w7",
                "melds": [
                    ["w1", "w2", "w3"],
                    ["w1", "w2", "w3"],
                    ["w4", "w5", "w6"],
                    ["w4", "w5", "w6"],
                ],
            },
        ],
    )

    assert context["is_self_draw"] is True
    assert context["is_discard_win"] is False
    assert context["is_concealed"] is True
    assert context["decomposition_kinds"] == ["seven_pairs", "standard"]
    assert context["seat_wind_key"] == "east"
    assert context["round_wind_key"] == "east"
