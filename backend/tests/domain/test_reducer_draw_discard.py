from app.domain.models import PlayerState, RoundState, Tile
from app.domain.wall import WallState
from app.domain.reducer import (
    apply_claim_action,
    apply_flower_action,
    apply_opening_flowers_pass,
    apply_self_kong_action,
    apply_discard_win,
    apply_self_draw_win,
    can_declare_flower,
    can_declare_hu,
    can_declare_self_kong,
    discard_tile,
    draw_for_turn,
    initialize_round,
    settle_exhaustive_draw,
)


def _force_flower_on_head(state: RoundState) -> RoundState:
    flower_tile = Tile(
        tile_id="f-test#0",
        tile_key="f-test",
        kind="flower",
        suit=None,
        rank=None,
        name="Test Flower",
    )
    replacement_tile = Tile(
        tile_id="t-test#0",
        tile_key="t-test",
        kind="suit",
        suit="bamboos",
        rank=1,
        name="Test Bamboo 1",
    )
    wall = WallState(
        tiles=(flower_tile, replacement_tile),
        head_index=0,
        tail_index=1,
    )
    return RoundState(
        round_id=state.round_id,
        dealer_seat=state.dealer_seat,
        current_actor=state.current_actor,
        wall=wall,
        players=state.players,
        last_discard=state.last_discard,
        pending_action=state.pending_action,
        phase=state.phase,
        settlement=state.settlement,
        version=state.version,
    )


def _make_tile(tile_key: str, tile_id: str) -> Tile:
    if tile_key in {"east", "south", "west", "north"}:
        return Tile(
            tile_id=tile_id,
            tile_key=tile_key,
            kind="wind",
            suit=None,
            rank=None,
            name=f"Test {tile_key}",
        )
    if tile_key in {"red", "green", "white"}:
        return Tile(
            tile_id=tile_id,
            tile_key=tile_key,
            kind="dragon",
            suit=None,
            rank=None,
            name=f"Test {tile_key}",
        )
    if tile_key.startswith("f"):
        return Tile(
            tile_id=tile_id,
            tile_key=tile_key,
            kind="flower",
            suit=None,
            rank=None,
            name=f"Test {tile_key}",
        )
    suit_prefix = tile_key[0]
    rank = int(tile_key[1:])
    suit_map = {"w": "characters", "t": "bamboos", "b": "dots"}
    return Tile(
        tile_id=tile_id,
        tile_key=tile_key,
        kind="suit",
        suit=suit_map[suit_prefix],
        rank=rank,
        name=f"Test {tile_key}",
    )


def make_round_state(
    current_actor: int,
    discarder_seat: int,
    last_discard: str,
    player_hands: dict[int, list[str]],
) -> RoundState:
    players: list[PlayerState] = []
    for seat in range(4):
        tile_keys = player_hands.get(seat, [])
        concealed = tuple(
            _make_tile(tile_key, f"{tile_key}#p{seat}-{idx}")
            for idx, tile_key in enumerate(tile_keys)
        )
        discards = ()
        if seat == discarder_seat:
            discard_tile_obj = _make_tile(last_discard, f"{last_discard}#discard")
            discards = (discard_tile_obj,)
        players.append(
            PlayerState(
                seat=seat,
                concealed_tiles=concealed,
                melds=(),
                flowers=(),
                discards=discards,
            )
        )
    return RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=current_actor,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=tuple(players),
        last_discard=_make_tile(last_discard, f"{last_discard}#last"),
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
    )


def make_exhausted_wall_state() -> RoundState:
    players = tuple(
        PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
        for seat in range(4)
    )
    return RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=1, tail_index=0),
        players=players,
        last_discard=None,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
    )


def test_draw_for_turn_keeps_flower_in_hand_until_declared():
    state = initialize_round(seed=1)
    state = _force_flower_on_head(state)
    next_state, event_log = draw_for_turn(state)
    assert any(event["type"] == "tile_drawn" for event in event_log)
    assert not any(event["type"] == "flower_exposed" for event in event_log)
    assert not any(event["type"] == "replacement_draw" for event in event_log)
    assert next_state.players[next_state.current_actor].concealed_tiles[-1].kind == "flower"
    assert can_declare_flower(next_state, next_state.current_actor) is True


def test_initialize_round_keeps_opening_flowers_in_concealed_hand(monkeypatch):
    flower_tile = Tile(
        tile_id="f-test#0",
        tile_key="f-test",
        kind="flower",
        suit=None,
        rank=None,
        name="Test Flower",
    )
    replacement_tile = Tile(
        tile_id="t-test#0",
        tile_key="t-test",
        kind="suit",
        suit="bamboos",
        rank=1,
        name="Test Bamboo 1",
    )

    def make_suit_tile(index: int) -> Tile:
        return Tile(
            tile_id=f"suit{index}#0",
            tile_key=f"suit{index}",
            kind="suit",
            suit="characters",
            rank=1,
            name=f"Test Suit {index}",
        )

    tiles = [flower_tile] + [make_suit_tile(i) for i in range(1, 70)]
    tiles[-1] = replacement_tile
    wall = WallState(
        tiles=tuple(tiles),
        head_index=0,
        tail_index=len(tiles) - 1,
    )

    monkeypatch.setattr("app.domain.reducer.build_wall", lambda seed: wall)

    state = initialize_round(seed=1)
    player0 = state.players[0]

    assert player0.flowers == ()
    assert player0.concealed_tiles[0] is flower_tile
    assert replacement_tile not in player0.concealed_tiles


def test_discard_tile_sets_last_discard_and_advances_version():
    state = initialize_round(seed=3)
    state, _ = draw_for_turn(state)
    tile_id = state.players[state.current_actor].concealed_tiles[0].tile_id
    next_state, events = discard_tile(state, state.current_actor, tile_id)
    assert next_state.last_discard is not None
    assert next_state.version == state.version + 1
    assert events[-1]["type"] == "tile_discarded"


def test_draw_for_turn_increments_version():
    state = initialize_round(seed=4)
    next_state, _ = draw_for_turn(state)
    assert next_state.version == state.version + 1


def test_apply_discard_win_transitions_round_to_settlement():
    winning_state = make_round_state(
        current_actor=1,
        discarder_seat=0,
        last_discard="w5",
        player_hands={
            1: [
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
                "w5",
            ],
        },
    )
    winning_state = RoundState(
        round_id=winning_state.round_id,
        dealer_seat=winning_state.dealer_seat,
        current_actor=winning_state.current_actor,
        wall=winning_state.wall,
        players=winning_state.players,
        last_discard=winning_state.last_discard,
        pending_action={
            "type": "claim_window",
            "discarder_seat": 0,
            "claim_window": [[], ["hu"], [], []],
        },
        phase=winning_state.phase,
        settlement=winning_state.settlement,
        version=winning_state.version,
    )
    next_state, events = apply_discard_win(winning_state, winner_seat=1)
    assert next_state.phase == "settlement"
    assert events[-1]["type"] == "settlement_ready"
    assert next_state.settlement["win_type"] == "discard"
    assert next_state.settlement["winner_seat"] == 1
    assert next_state.settlement["fan_breakdown"]
    assert next_state.settlement["kong_score_detail"] == []


def test_settle_exhaustive_draw_marks_drawn_round():
    state = make_exhausted_wall_state()
    next_state, events = settle_exhaustive_draw(state)
    assert next_state.phase == "settlement"
    assert events[-1]["type"] == "round_drawn"


def test_apply_discard_win_uses_authoritative_discarder_seat():
    winning_state = make_round_state(
        current_actor=1,
        discarder_seat=0,
        last_discard="w5",
        player_hands={
            1: [
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
                "w5",
            ],
        },
    )
    players = list(winning_state.players)
    player2 = players[2]
    players[2] = PlayerState(
        seat=player2.seat,
        concealed_tiles=player2.concealed_tiles,
        melds=player2.melds,
        flowers=player2.flowers,
        discards=(_make_tile("east", "east#p2-old"),),
    )
    winning_state = RoundState(
        round_id=winning_state.round_id,
        dealer_seat=winning_state.dealer_seat,
        current_actor=winning_state.current_actor,
        wall=winning_state.wall,
        players=tuple(players),
        last_discard=winning_state.last_discard,
        pending_action={
            "type": "claim_window",
            "discarder_seat": 0,
            "claim_window": [[], ["hu"], [], []],
        },
        phase=winning_state.phase,
        settlement=winning_state.settlement,
        version=winning_state.version,
    )

    next_state, _ = apply_discard_win(winning_state, winner_seat=1)
    assert next_state.settlement["discarder_seat"] == 0


def test_apply_self_draw_win_requires_current_actor():
    state = make_round_state(
        current_actor=1,
        discarder_seat=0,
        last_discard="east",
        player_hands={
            1: [
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
        },
    )
    next_state, events = apply_self_draw_win(state, winner_seat=1)
    assert next_state.phase == "settlement"
    assert events[-1]["type"] == "settlement_ready"
    assert next_state.settlement["win_type"] == "self_draw"
    assert next_state.settlement["winner_seat"] == 1
    assert next_state.settlement["fan_total"] >= 1
    assert next_state.settlement["fan_breakdown"]


def test_can_declare_hu_requires_minimum_eight_fan():
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(
            PlayerState(
                seat=0,
                concealed_tiles=(
                    _make_tile("w1", "w1#0"),
                    _make_tile("w2", "w2#0"),
                    _make_tile("w3", "w3#0"),
                    _make_tile("t4", "t4#0"),
                    _make_tile("t5", "t5#0"),
                    _make_tile("t6", "t6#0"),
                    _make_tile("b2", "b2#0"),
                    _make_tile("b3", "b3#0"),
                    _make_tile("b4", "b4#0"),
                    _make_tile("red", "red#0"),
                    _make_tile("red", "red#1"),
                ),
                melds=((
                    _make_tile("w7", "w7#m0"),
                    _make_tile("w8", "w8#m1"),
                    _make_tile("w9", "w9#m2"),
                ),),
                flowers=(),
                discards=(),
            ),
        )
        + tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(1, 4)
        ),
        last_discard=None,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
        score_trackers=None,
        last_action_context=None,
        round_wind="east",
    )

    assert can_declare_hu(state, 0, None) is False


def test_can_declare_hu_accepts_supported_eight_fan_hand():
    state = make_round_state(
        current_actor=0,
        discarder_seat=0,
        last_discard="w1",
        player_hands={
            0: [
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
        },
    )
    state = RoundState(
        round_id=state.round_id,
        dealer_seat=state.dealer_seat,
        current_actor=state.current_actor,
        wall=state.wall,
        players=state.players,
        last_discard=None,
        pending_action=None,
        phase=state.phase,
        settlement=state.settlement,
        version=state.version,
        score_trackers=state.score_trackers,
    )

    assert can_declare_hu(state, 0, None) is True


def test_can_declare_hu_allows_low_fan_win_when_eight_fan_rule_is_disabled():
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(
            PlayerState(
                seat=0,
                concealed_tiles=(
                    _make_tile("w1", "w1#0"),
                    _make_tile("w2", "w2#0"),
                    _make_tile("w3", "w3#0"),
                    _make_tile("t4", "t4#0"),
                    _make_tile("t5", "t5#0"),
                    _make_tile("t6", "t6#0"),
                    _make_tile("b2", "b2#0"),
                    _make_tile("b3", "b3#0"),
                    _make_tile("b4", "b4#0"),
                    _make_tile("red", "red#0"),
                    _make_tile("red", "red#1"),
                ),
                melds=((
                    _make_tile("w7", "w7#m0"),
                    _make_tile("w8", "w8#m1"),
                    _make_tile("w9", "w9#m2"),
                ),),
                flowers=(),
                discards=(),
            ),
        )
        + tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(1, 4)
        ),
        last_discard=None,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
        score_trackers=None,
        last_action_context=None,
        round_wind="east",
        enforce_minimum_eight_fan=False,
    )

    assert can_declare_hu(state, 0, None) is True


def test_apply_self_draw_win_includes_visible_kong_score_detail():
    state = make_round_state(
        current_actor=0,
        discarder_seat=0,
        last_discard="east",
        player_hands={
            0: [
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
        },
    )
    state = RoundState(
        round_id=state.round_id,
        dealer_seat=state.dealer_seat,
        current_actor=state.current_actor,
        wall=state.wall,
        players=state.players,
        last_discard=state.last_discard,
        pending_action=state.pending_action,
        phase=state.phase,
        settlement=state.settlement,
        version=state.version,
        score_trackers={
            "kong_entries": [
                {"kong_type": "concealed_kong", "actor_seat": 0, "payer_seats": [1, 2, 3]}
            ]
        },
    )

    next_state, _ = apply_self_draw_win(state, winner_seat=0)

    assert next_state.settlement["kong_score_detail"][0]["kong_type"] == "concealed_kong"
    assert next_state.settlement["score_delta"]["kong_delta_by_seat"] == {
        0: 3,
        1: -1,
        2: -1,
        3: -1,
    }


def test_apply_self_draw_win_includes_ping_hu_yi_ban_gao_and_duan_yao():
    state = make_round_state(
        current_actor=0,
        discarder_seat=0,
        last_discard="w1",
        player_hands={
            0: [
                "w2",
                "w3",
                "w4",
                "w2",
                "w3",
                "w4",
                "t4",
                "t5",
                "t6",
                "b4",
                "b5",
                "b6",
                "t8",
                "t8",
            ],
        },
    )
    state = RoundState(
        round_id=state.round_id,
        dealer_seat=state.dealer_seat,
        current_actor=state.current_actor,
        wall=state.wall,
        players=state.players,
        last_discard=None,
        pending_action=None,
        phase=state.phase,
        settlement=state.settlement,
        version=state.version,
        score_trackers=state.score_trackers,
    )

    next_state, _ = apply_self_draw_win(state, winner_seat=0)

    assert "all_simples" in next_state.settlement["fan_keys"]
    assert "pure_double_chow" in next_state.settlement["fan_keys"]
    assert "mixed_double_chow" in next_state.settlement["fan_keys"]
    assert "all_chows" in next_state.settlement["fan_keys"]


def test_apply_discard_win_includes_all_terminals() -> None:
    winning_state = make_round_state(
        current_actor=1,
        discarder_seat=0,
        last_discard="b1",
        player_hands={
            1: [
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
            ],
        },
    )
    winning_state = RoundState(
        round_id=winning_state.round_id,
        dealer_seat=winning_state.dealer_seat,
        current_actor=winning_state.current_actor,
        wall=winning_state.wall,
        players=winning_state.players,
        last_discard=winning_state.last_discard,
        pending_action={
            "type": "claim_window",
            "discarder_seat": 0,
            "claim_window": [[], ["hu"], [], []],
        },
        phase=winning_state.phase,
        settlement=winning_state.settlement,
        version=winning_state.version,
        score_trackers=winning_state.score_trackers,
    )

    next_state, _ = apply_discard_win(winning_state, winner_seat=1)

    assert "all_terminals" in next_state.settlement["fan_keys"]
    assert "all_terminals_and_honours" not in next_state.settlement["fan_keys"]


def test_apply_self_draw_win_includes_seat_and_round_wind_triplets():
    state = make_round_state(
        current_actor=0,
        discarder_seat=0,
        last_discard="w1",
        player_hands={
            0: [
                "east",
                "east",
                "east",
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
        },
    )
    state = RoundState(
        round_id=state.round_id,
        dealer_seat=state.dealer_seat,
        current_actor=state.current_actor,
        wall=state.wall,
        players=state.players,
        last_discard=None,
        pending_action=None,
        phase=state.phase,
        settlement=state.settlement,
        version=state.version,
        score_trackers=state.score_trackers,
    )

    next_state, _ = apply_self_draw_win(state, winner_seat=0)

    assert "seat_wind" in next_state.settlement["fan_keys"]
    assert "prevalent_wind" in next_state.settlement["fan_keys"]


def test_apply_self_draw_win_uses_rotating_seat_wind_from_dealer() -> None:
    state = make_round_state(
        current_actor=0,
        discarder_seat=0,
        last_discard="w1",
        player_hands={
            0: [
                "north",
                "north",
                "north",
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
        },
    )
    state = RoundState(
        round_id=state.round_id,
        dealer_seat=1,
        current_actor=state.current_actor,
        wall=state.wall,
        players=state.players,
        last_discard=None,
        pending_action=None,
        phase=state.phase,
        settlement=state.settlement,
        version=state.version,
        score_trackers=state.score_trackers,
        round_wind="east",
    )

    next_state, _ = apply_self_draw_win(state, winner_seat=0)

    assert "seat_wind" in next_state.settlement["fan_keys"]
    assert "prevalent_wind" not in next_state.settlement["fan_keys"]


def test_apply_discard_win_includes_dragon_and_terminal_triplet_fans() -> None:
    winning_state = make_round_state(
        current_actor=1,
        discarder_seat=0,
        last_discard="east",
        player_hands={
            1: [
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
            ],
        },
    )
    winning_state = RoundState(
        round_id=winning_state.round_id,
        dealer_seat=winning_state.dealer_seat,
        current_actor=winning_state.current_actor,
        wall=winning_state.wall,
        players=winning_state.players,
        last_discard=winning_state.last_discard,
        pending_action={
            "type": "claim_window",
            "discarder_seat": 0,
            "claim_window": [[], ["hu"], [], []],
        },
        phase=winning_state.phase,
        settlement=winning_state.settlement,
        version=winning_state.version,
        score_trackers=winning_state.score_trackers,
    )

    next_state, _ = apply_discard_win(winning_state, winner_seat=1)

    assert "two_dragon_pungs" in next_state.settlement["fan_keys"]
    assert "all_terminals_and_honours" in next_state.settlement["fan_keys"]


def test_apply_self_draw_win_includes_gang_shang_hua() -> None:
    state = make_round_state(
        current_actor=0,
        discarder_seat=0,
        last_discard="east",
        player_hands={
            0: [
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
        },
    )
    state = RoundState(
        round_id=state.round_id,
        dealer_seat=state.dealer_seat,
        current_actor=state.current_actor,
        wall=state.wall,
        players=state.players,
        last_discard=None,
        pending_action=None,
        phase=state.phase,
        settlement=state.settlement,
        version=state.version,
        score_trackers=state.score_trackers,
        last_action_context={
            "kind": "replacement_draw",
            "seat": 0,
            "tile_id": "east#draw",
            "from_kong_replacement": True,
            "was_last_live_tile": True,
            "was_last_discard": False,
        },
    )

    next_state, _ = apply_self_draw_win(state, winner_seat=0)

    assert "out_with_replacement_tile" in next_state.settlement["fan_keys"]
    assert "last_tile_draw" not in next_state.settlement["fan_keys"]


def test_apply_self_draw_win_includes_hai_di_lao_yue() -> None:
    state = make_round_state(
        current_actor=0,
        discarder_seat=0,
        last_discard="east",
        player_hands={
            0: [
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
        },
    )
    state = RoundState(
        round_id=state.round_id,
        dealer_seat=state.dealer_seat,
        current_actor=state.current_actor,
        wall=state.wall,
        players=state.players,
        last_discard=None,
        pending_action=None,
        phase=state.phase,
        settlement=state.settlement,
        version=state.version,
        score_trackers=state.score_trackers,
        last_action_context={
            "kind": "draw",
            "seat": 0,
            "tile_id": "east#draw",
            "from_kong_replacement": False,
            "was_last_live_tile": True,
            "was_last_discard": False,
        },
    )

    next_state, _ = apply_self_draw_win(state, winner_seat=0)

    assert "last_tile_draw" in next_state.settlement["fan_keys"]


def test_apply_discard_win_includes_he_di_lao_yu() -> None:
    winning_state = make_round_state(
        current_actor=1,
        discarder_seat=0,
        last_discard="w5",
        player_hands={
            1: [
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
                "w5",
            ],
        },
    )
    winning_state = RoundState(
        round_id=winning_state.round_id,
        dealer_seat=winning_state.dealer_seat,
        current_actor=winning_state.current_actor,
        wall=winning_state.wall,
        players=winning_state.players,
        last_discard=winning_state.last_discard,
        pending_action={
            "type": "claim_window",
            "discarder_seat": 0,
            "claim_window": [[], ["hu"], [], []],
        },
        phase=winning_state.phase,
        settlement=winning_state.settlement,
        version=winning_state.version,
        score_trackers=winning_state.score_trackers,
        last_action_context={
            "kind": "discard",
            "seat": 0,
            "tile_id": "east#discard",
            "from_kong_replacement": False,
            "was_last_live_tile": False,
            "was_last_discard": True,
        },
    )

    next_state, _ = apply_discard_win(winning_state, winner_seat=1)

    assert "last_tile_claim" in next_state.settlement["fan_keys"]


def test_apply_discard_win_rejects_missing_hu_offer():
    winning_state = make_round_state(
        current_actor=1,
        discarder_seat=0,
        last_discard="east",
        player_hands={
            1: [
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
            ],
        },
    )
    winning_state = RoundState(
        round_id=winning_state.round_id,
        dealer_seat=winning_state.dealer_seat,
        current_actor=winning_state.current_actor,
        wall=winning_state.wall,
        players=winning_state.players,
        last_discard=winning_state.last_discard,
        pending_action={"type": "claim_window", "discarder_seat": 0, "claim_window": []},
        phase=winning_state.phase,
        settlement=winning_state.settlement,
        version=winning_state.version,
    )

    import pytest

    with pytest.raises(ValueError):
        apply_discard_win(winning_state, winner_seat=1)


def test_apply_self_draw_win_rejects_wrong_actor():
    state = make_round_state(
        current_actor=0,
        discarder_seat=0,
        last_discard="east",
        player_hands={
            1: [
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
            ],
        },
    )
    import pytest

    with pytest.raises(ValueError):
        apply_self_draw_win(state, winner_seat=1)


def test_apply_self_draw_win_with_melds_is_legal():
    player_meld = (
        _make_tile("w1", "w1#m1"),
        _make_tile("w1", "w1#m2"),
        _make_tile("w1", "w1#m3"),
    )
    player = PlayerState(
        seat=0,
            concealed_tiles=(
                _make_tile("w2", "w2#c1"),
                _make_tile("w2", "w2#c2"),
                _make_tile("w2", "w2#c3"),
                _make_tile("w3", "w3#c4"),
                _make_tile("w3", "w3#c5"),
                _make_tile("w3", "w3#c6"),
                _make_tile("w4", "w4#c7"),
                _make_tile("w4", "w4#c8"),
                _make_tile("w4", "w4#c9"),
                _make_tile("w5", "w5#c10"),
                _make_tile("w5", "w5#c11"),
            ),
            melds=(player_meld,),
            flowers=(),
            discards=(),
    )
    players = (player,) + tuple(
        PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
        for seat in range(1, 4)
    )
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=players,
        last_discard=None,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
    )

    next_state, _ = apply_self_draw_win(state, winner_seat=0)
    assert next_state.phase == "settlement"


def test_apply_self_draw_win_with_kong_meld_is_legal():
    player_meld = (
        _make_tile("w1", "w1#m1"),
        _make_tile("w1", "w1#m2"),
        _make_tile("w1", "w1#m3"),
        _make_tile("w1", "w1#m4"),
    )
    player = PlayerState(
        seat=0,
            concealed_tiles=(
                _make_tile("w2", "w2#c1"),
                _make_tile("w2", "w2#c2"),
                _make_tile("w2", "w2#c3"),
                _make_tile("w3", "w3#c4"),
                _make_tile("w3", "w3#c5"),
                _make_tile("w3", "w3#c6"),
                _make_tile("w4", "w4#c7"),
                _make_tile("w4", "w4#c8"),
                _make_tile("w4", "w4#c9"),
                _make_tile("w5", "w5#c10"),
                _make_tile("w5", "w5#c11"),
            ),
            melds=(player_meld,),
            flowers=(),
            discards=(),
    )
    players = (player,) + tuple(
        PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
        for seat in range(1, 4)
    )
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=players,
        last_discard=None,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
    )

    next_state, _ = apply_self_draw_win(state, winner_seat=0)
    assert next_state.phase == "settlement"


def test_settle_exhaustive_draw_rejects_non_exhausted_wall():
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=0),
        players=tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(4)
        ),
        last_discard=None,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
    )

    import pytest

    with pytest.raises(ValueError):
        settle_exhaustive_draw(state)


def test_draw_for_turn_settles_exhaustive_draw_when_live_wall_is_empty():
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(4)
        ),
        last_discard=None,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
    )

    next_state, events = draw_for_turn(state)

    assert next_state.phase == "settlement"
    assert next_state.settlement["draw_type"] == "exhaustive"
    assert events[-1]["type"] == "round_drawn"


def test_apply_self_kong_action_supports_concealed_kong():
    replacement_tile = _make_tile("b9", "b9#replacement")
    player = PlayerState(
        seat=0,
        concealed_tiles=(
            _make_tile("t5", "t5#1"),
            _make_tile("t5", "t5#2"),
            _make_tile("t5", "t5#3"),
            _make_tile("t5", "t5#4"),
        ),
        melds=(),
        flowers=(),
        discards=(),
    )
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(replacement_tile,), head_index=0, tail_index=0),
        players=(player,) + tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(1, 4)
        ),
        last_discard=None,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
    )

    assert can_declare_self_kong(state, 0) is True

    next_state, events = apply_self_kong_action(
        state,
        seat=0,
        tile_ids=["t5#1", "t5#2", "t5#3", "t5#4"],
    )

    assert any(event["type"] == "self_kong_declared" for event in events)
    assert any(event["type"] == "replacement_draw" for event in events)
    assert len(next_state.players[0].melds[0]) == 4


def test_can_declare_self_kong_is_blocked_after_last_live_tile_draw():
    player = PlayerState(
        seat=0,
        concealed_tiles=(
            _make_tile("t5", "t5#1"),
            _make_tile("t5", "t5#2"),
            _make_tile("t5", "t5#3"),
            _make_tile("t5", "t5#4"),
        ),
        melds=(),
        flowers=(),
        discards=(),
    )
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=1, tail_index=0),
        players=(player,) + tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(1, 4)
        ),
        last_discard=None,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
        last_action_context={
            "kind": "draw",
            "seat": 0,
            "tile_id": "t5#draw",
            "from_kong_replacement": False,
            "was_last_live_tile": True,
            "was_last_discard": False,
        },
    )

    assert can_declare_self_kong(state, 0) is False


def test_can_declare_flower_is_blocked_after_last_live_tile_draw():
    player = PlayerState(
        seat=0,
        concealed_tiles=(
            _make_tile("f1", "f1#1"),
            _make_tile("w1", "w1#1"),
        ),
        melds=(),
        flowers=(),
        discards=(),
    )
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=1, tail_index=0),
        players=(player,) + tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(1, 4)
        ),
        last_discard=None,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
        last_action_context={
            "kind": "draw",
            "seat": 0,
            "tile_id": "f1#1",
            "from_kong_replacement": False,
            "was_last_live_tile": True,
            "was_last_discard": False,
        },
    )

    assert can_declare_flower(state, 0) is False


def test_apply_flower_action_exposes_selected_flower_and_draws_from_tail():
    player = PlayerState(
        seat=0,
        concealed_tiles=(
            _make_tile("w1", "w1#1"),
            _make_tile("f1", "f1#1"),
        ),
        melds=(),
        flowers=(),
        discards=(),
    )
    replacement_tile = _make_tile("b9", "b9#replacement")
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(replacement_tile,), head_index=0, tail_index=0),
        players=(player,) + tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(1, 4)
        ),
        last_discard=None,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
        score_trackers={"kong_entries": []},
        last_action_context={
            "kind": "draw",
            "seat": 0,
            "tile_id": "f1#1",
            "from_kong_replacement": False,
            "was_last_live_tile": False,
            "was_last_discard": False,
        },
    )

    next_state, events = apply_flower_action(state, seat=0, tile_ids=["f1#1"])

    assert events[0]["type"] == "flower_exposed"
    assert events[1]["type"] == "replacement_draw"
    assert next_state.players[0].flowers == (_make_tile("f1", "f1#1"),)
    assert next_state.players[0].concealed_tiles[-1].tile_id == "b9#replacement"
    assert next_state.wall.tail_index == -1


def test_apply_opening_flowers_pass_advances_to_next_seat():
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(
            PlayerState(
                seat=0,
                concealed_tiles=(_make_tile("w1", "w1#1"),),
                melds=(),
                flowers=(),
                discards=(),
            ),
            PlayerState(
                seat=1,
                concealed_tiles=(_make_tile("f1", "f1#1"),),
                melds=(),
                flowers=(),
                discards=(),
            ),
        )
        + tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(2, 4)
        ),
        last_discard=None,
        pending_action={"type": "opening_flowers", "dealer_seat": 0},
        phase="playing",
        settlement=None,
        version=0,
        score_trackers={"kong_entries": [], "opening_flowers_completed": False},
    )

    next_state, events = apply_opening_flowers_pass(state, seat=0)

    assert events == []
    assert next_state.current_actor == 1
    assert next_state.pending_action == {"type": "opening_flowers", "dealer_seat": 0}


def test_apply_self_draw_win_keeps_fully_concealed_hand_with_concealed_kong():
    concealed_kong = (
        _make_tile("w1", "w1#m1"),
        _make_tile("w1", "w1#m2"),
        _make_tile("w1", "w1#m3"),
        _make_tile("w1", "w1#m4"),
    )
    player = PlayerState(
        seat=0,
        concealed_tiles=(
            _make_tile("t2", "t2#1"),
            _make_tile("t3", "t3#1"),
            _make_tile("t4", "t4#1"),
            _make_tile("b2", "b2#1"),
            _make_tile("b3", "b3#1"),
            _make_tile("b4", "b4#1"),
            _make_tile("t5", "t5#1"),
            _make_tile("t6", "t6#1"),
            _make_tile("t7", "t7#1"),
            _make_tile("red", "red#1"),
            _make_tile("red", "red#2"),
        ),
        melds=(concealed_kong,),
        flowers=(),
        discards=(),
    )
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(player,) + tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(1, 4)
        ),
        last_discard=None,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
        score_trackers={
            "kong_entries": [
                {
                    "kong_type": "concealed_kong",
                    "actor_seat": 0,
                    "payer_seats": [1, 2, 3],
                    "tile_key": "w1",
                }
            ]
        },
    )

    next_state, _ = apply_self_draw_win(state, winner_seat=0)

    assert "fully_concealed_hand" in next_state.settlement["fan_keys"]
    assert "concealed_hand" not in next_state.settlement["fan_keys"]
    assert "concealed_kong" in next_state.settlement["fan_keys"]


def test_apply_self_kong_action_supports_add_kong():
    replacement_tile = _make_tile("b9", "b9#replacement")
    pung_meld = (
        _make_tile("w3", "w3#m1"),
        _make_tile("w3", "w3#m2"),
        _make_tile("w3", "w3#m3"),
    )
    player = PlayerState(
        seat=0,
        concealed_tiles=(
            _make_tile("w3", "w3#add"),
            _make_tile("t1", "t1#1"),
        ),
        melds=(pung_meld,),
        flowers=(),
        discards=(),
    )
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(replacement_tile,), head_index=0, tail_index=0),
        players=(player,) + tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(1, 4)
        ),
        last_discard=None,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
    )

    assert can_declare_self_kong(state, 0) is True

    next_state, events = apply_self_kong_action(
        state,
        seat=0,
        tile_ids=["w3#add"],
    )

    assert any(event["type"] == "self_kong_declared" for event in events)
    assert len(next_state.players[0].melds[0]) == 4
    assert next_state.players[0].melds[0][-1].tile_id == "w3#add"


def test_add_kong_enters_rob_kong_window_when_opponent_can_win():
    pung_meld = (
        _make_tile("w5", "w5#m1"),
        _make_tile("w5", "w5#m2"),
        _make_tile("w5", "w5#m3"),
    )
    actor = PlayerState(
        seat=0,
        concealed_tiles=(
            _make_tile("w5", "w5#add"),
            _make_tile("w2", "w2#a"),
        ),
        melds=(pung_meld,),
        flowers=(),
        discards=(),
    )
    winner = PlayerState(
        seat=1,
        concealed_tiles=(
            _make_tile("w1", "w1#1"),
            _make_tile("w1", "w1#2"),
            _make_tile("w1", "w1#3"),
            _make_tile("w2", "w2#1"),
            _make_tile("w2", "w2#2"),
            _make_tile("w2", "w2#3"),
            _make_tile("w3", "w3#1"),
            _make_tile("w3", "w3#2"),
            _make_tile("w3", "w3#3"),
            _make_tile("w4", "w4#1"),
            _make_tile("w4", "w4#2"),
            _make_tile("w4", "w4#3"),
            _make_tile("w5", "w5#pair"),
        ),
        melds=(),
        flowers=(),
        discards=(),
    )
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(actor, winner) + tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(2, 4)
        ),
        last_discard=None,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
        score_trackers=_empty_score_trackers_for_test(),
        last_action_context=None,
    )

    next_state, events = apply_self_kong_action(state, seat=0, tile_ids=["w5#add"])

    assert events[0]["type"] == "self_kong_declared"
    assert next_state.pending_action is not None
    assert next_state.pending_action["type"] == "rob_kong_window"
    assert next_state.pending_action["actor_seat"] == 0
    assert next_state.pending_action["offered_hu_seats"] == [1]
    assert len(next_state.players[0].melds[0]) == 3


def test_rob_kong_hu_cancels_add_kong_and_settles_as_discard_win():
    actor = PlayerState(
        seat=0,
        concealed_tiles=(
            _make_tile("w5", "w5#add"),
            _make_tile("w2", "w2#a"),
        ),
        melds=((
            _make_tile("w5", "w5#m1"),
            _make_tile("w5", "w5#m2"),
            _make_tile("w5", "w5#m3"),
        ),),
        flowers=(),
        discards=(),
    )
    winner = PlayerState(
        seat=1,
        concealed_tiles=(
            _make_tile("w1", "w1#1"),
            _make_tile("w1", "w1#2"),
            _make_tile("w1", "w1#3"),
            _make_tile("w2", "w2#1"),
            _make_tile("w2", "w2#2"),
            _make_tile("w2", "w2#3"),
            _make_tile("w3", "w3#1"),
            _make_tile("w3", "w3#2"),
            _make_tile("w3", "w3#3"),
            _make_tile("w4", "w4#1"),
            _make_tile("w4", "w4#2"),
            _make_tile("w4", "w4#3"),
            _make_tile("w5", "w5#pair"),
        ),
        melds=(),
        flowers=(),
        discards=(),
    )
    robbed_tile = _make_tile("w5", "w5#add")
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(actor, winner) + tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(2, 4)
        ),
        last_discard=robbed_tile,
        pending_action={
            "type": "rob_kong_window",
            "actor_seat": 0,
            "tile_id": "w5#add",
            "tile_key": "w5",
            "meld_index": 0,
            "offered_hu_seats": [1],
            "responded_seats": [],
        },
        phase="playing",
        settlement=None,
        version=0,
        score_trackers=_empty_score_trackers_for_test(),
        last_action_context=None,
    )

    next_state, events = apply_discard_win(state, winner_seat=1)

    assert next_state.phase == "settlement"
    assert next_state.settlement["discarder_seat"] == 0
    assert events[-1]["type"] == "settlement_ready"


def test_all_passes_complete_add_kong_and_draw_replacement():
    actor = PlayerState(
        seat=0,
        concealed_tiles=(
            _make_tile("east", "east#add"),
            _make_tile("w2", "w2#a"),
        ),
        melds=((
            _make_tile("east", "east#m1"),
            _make_tile("east", "east#m2"),
            _make_tile("east", "east#m3"),
        ),),
        flowers=(),
        discards=(),
    )
    robbed_tile = _make_tile("east", "east#add")
    replacement_tile = _make_tile("b9", "b9#replacement")
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(replacement_tile,), head_index=0, tail_index=0),
        players=(actor,) + tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(1, 4)
        ),
        last_discard=robbed_tile,
        pending_action={
            "type": "rob_kong_window",
            "actor_seat": 0,
            "tile_id": "east#add",
            "tile_key": "east",
            "meld_index": 0,
            "offered_hu_seats": [1],
            "responded_seats": [],
        },
        phase="playing",
        settlement=None,
        version=0,
        score_trackers=_empty_score_trackers_for_test(),
        last_action_context=None,
    )

    next_state, events = apply_claim_action(state, seat=1, action_type="pass", tiles=[])

    assert next_state.pending_action is None
    assert len(next_state.players[0].melds[0]) == 4
    assert any(event["type"] == "replacement_draw" for event in events)


def _empty_score_trackers_for_test() -> dict:
    return {"kong_entries": []}
