import json

from app.domain.claims import compute_claim_window, resolve_claims
from app.domain.models import PlayerState, RoundState, Tile
from app.domain.reducer import apply_claim_action, discard_tile
from app.domain.wall import WallState
import pytest


def _make_suit_tile(tile_key: str, tile_id: str) -> Tile:
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


def _make_honor_tile(tile_key: str, tile_id: str, kind: str) -> Tile:
    return Tile(
        tile_id=tile_id,
        tile_key=tile_key,
        kind=kind,
        suit=None,
        rank=None,
        name=f"Test {tile_key}",
    )


def _make_wall(tiles: list[Tile]) -> WallState:
    if not tiles:
        return WallState(tiles=(), head_index=0, tail_index=-1)
    return WallState(tiles=tuple(tiles), head_index=0, tail_index=len(tiles) - 1)


def _make_round_state(
    players: list[PlayerState],
    last_discard: Tile | None,
    current_actor: int,
) -> RoundState:
    return RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=current_actor,
        wall=_make_wall([]),
        players=tuple(players),
        last_discard=last_discard,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
    )


def test_chow_is_offered_only_to_next_player():
    discard = _make_suit_tile("t3", "t3#discard")
    players = [
        PlayerState(
            seat=0,
            concealed_tiles=(),
            melds=(),
            flowers=(),
            discards=(discard,),
        ),
        PlayerState(
            seat=1,
            concealed_tiles=(
                _make_suit_tile("t1", "t1#p1"),
                _make_suit_tile("t2", "t2#p1"),
            ),
            melds=(),
            flowers=(),
            discards=(),
        ),
        PlayerState(
            seat=2,
            concealed_tiles=(_make_suit_tile("t1", "t1#p2"),),
            melds=(),
            flowers=(),
            discards=(),
        ),
        PlayerState(
            seat=3,
            concealed_tiles=(_make_suit_tile("t2", "t2#p3"),),
            melds=(),
            flowers=(),
            discards=(),
        ),
    ]
    state = _make_round_state(players, discard, current_actor=0)

    claim_window = compute_claim_window(state)

    assert claim_window[1] == {"chow"}
    assert claim_window[2] == set()
    assert claim_window[3] == set()


def test_pung_beats_chow_when_both_are_available():
    winner = resolve_claims(
        [{"seat": 1, "type": "chow"}, {"seat": 2, "type": "pung"}],
        discarder_seat=0,
    )
    assert winner == {"seat": 2, "type": "pung"}


def test_same_priority_claim_uses_nearest_seat():
    winner = resolve_claims(
        [{"seat": 3, "type": "pung"}, {"seat": 1, "type": "pung"}],
        discarder_seat=0,
    )
    assert winner == {"seat": 1, "type": "pung"}


def test_resolve_claims_ignores_non_next_player_chow():
    winner = resolve_claims(
        [{"seat": 2, "type": "chow"}],
        discarder_seat=0,
    )
    assert winner is None


def test_apply_claim_action_pung_merges_discard_into_meld():
    discard = _make_suit_tile("t4", "t4#discard")
    claimant_tiles = (
        _make_suit_tile("t4", "t4#c1"),
        _make_suit_tile("t4", "t4#c2"),
    )
    players = [
        PlayerState(
            seat=0,
            concealed_tiles=(),
            melds=(),
            flowers=(),
            discards=(discard,),
        ),
        PlayerState(
            seat=1,
            concealed_tiles=claimant_tiles,
            melds=(),
            flowers=(),
            discards=(),
        ),
        PlayerState(seat=2, concealed_tiles=(), melds=(), flowers=(), discards=()),
        PlayerState(seat=3, concealed_tiles=(), melds=(), flowers=(), discards=()),
    ]
    claim_window = [[], ["pung"], [], []]
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=_make_wall([]),
        players=tuple(players),
        last_discard=discard,
        pending_action={
            "type": "claim_window",
            "discarder_seat": 0,
            "claim_window": claim_window,
        },
        phase="playing",
        settlement=None,
        version=0,
    )

    next_state, events = apply_claim_action(
        state,
        seat=1,
        action_type="pung",
        tiles=[tile.tile_id for tile in claimant_tiles],
    )

    updated_player = next_state.players[1]
    meld_tile_ids = {tile.tile_id for tile in updated_player.melds[-1]}
    assert meld_tile_ids == {"t4#discard", "t4#c1", "t4#c2"}
    assert all(
        tile.tile_id not in {"t4#c1", "t4#c2"}
        for tile in updated_player.concealed_tiles
    )
    assert next_state.pending_action is None
    assert next_state.current_actor == 1
    assert events[0]["type"] == "claim_made"


def test_apply_claim_action_kong_draws_replacement_from_tail():
    discard = _make_suit_tile("t5", "t5#discard")
    claimant_tiles = (
        _make_suit_tile("t5", "t5#c1"),
        _make_suit_tile("t5", "t5#c2"),
        _make_suit_tile("t5", "t5#c3"),
    )
    replacement_tile = _make_suit_tile("t9", "t9#r1")
    wall = _make_wall([replacement_tile])
    players = [
        PlayerState(
            seat=0,
            concealed_tiles=(),
            melds=(),
            flowers=(),
            discards=(discard,),
        ),
        PlayerState(
            seat=1,
            concealed_tiles=claimant_tiles,
            melds=(),
            flowers=(),
            discards=(),
        ),
        PlayerState(seat=2, concealed_tiles=(), melds=(), flowers=(), discards=()),
        PlayerState(seat=3, concealed_tiles=(), melds=(), flowers=(), discards=()),
    ]
    claim_window = [[], ["kong"], [], []]
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=wall,
        players=tuple(players),
        last_discard=discard,
        pending_action={
            "type": "claim_window",
            "discarder_seat": 0,
            "claim_window": claim_window,
        },
        phase="playing",
        settlement=None,
        version=0,
    )

    next_state, events = apply_claim_action(
        state,
        seat=1,
        action_type="kong",
        tiles=[tile.tile_id for tile in claimant_tiles],
    )

    updated_player = next_state.players[1]
    assert any(tile.tile_id == "t9#r1" for tile in updated_player.concealed_tiles)
    assert next_state.wall.tail_index == -1
    assert any(event["type"] == "replacement_draw" for event in events)


def test_pending_action_claim_window_is_json_friendly():
    discard_tile_obj = _make_suit_tile("t3", "t3#discard")
    hand_tile = _make_suit_tile("b1", "b1#hand")
    chow_tiles = (
        _make_suit_tile("t1", "t1#c1"),
        _make_suit_tile("t2", "t2#c2"),
    )
    players = [
        PlayerState(
            seat=0,
            concealed_tiles=(discard_tile_obj, hand_tile),
            melds=(),
            flowers=(),
            discards=(),
        ),
        PlayerState(
            seat=1,
            concealed_tiles=chow_tiles,
            melds=(),
            flowers=(),
            discards=(),
        ),
        PlayerState(seat=2, concealed_tiles=(), melds=(), flowers=(), discards=()),
        PlayerState(seat=3, concealed_tiles=(), melds=(), flowers=(), discards=()),
    ]
    state = _make_round_state(players, None, current_actor=0)
    next_state, _ = discard_tile(state, seat=0, tile_id="t3#discard")

    serialized = json.loads(json.dumps(next_state.pending_action))
    claim_window = serialized["claim_window"]
    assert isinstance(claim_window, list)
    assert isinstance(claim_window[1], list)
    assert "chow" in claim_window[1]


def test_apply_claim_action_rejects_illegal_claim():
    discard = _make_suit_tile("t6", "t6#discard")
    players = [
        PlayerState(
            seat=0,
            concealed_tiles=(),
            melds=(),
            flowers=(),
            discards=(discard,),
        ),
        PlayerState(
            seat=1,
            concealed_tiles=(),
            melds=(),
            flowers=(),
            discards=(),
        ),
        PlayerState(seat=2, concealed_tiles=(), melds=(), flowers=(), discards=()),
        PlayerState(seat=3, concealed_tiles=(), melds=(), flowers=(), discards=()),
    ]
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=_make_wall([]),
        players=tuple(players),
        last_discard=discard,
        pending_action={
            "type": "claim_window",
            "discarder_seat": 0,
            "claim_window": [[], [], [], []],
        },
        phase="playing",
        settlement=None,
        version=0,
    )

    with pytest.raises(ValueError):
        apply_claim_action(state, seat=1, action_type="pung", tiles=[])


def test_apply_claim_action_rejects_invalid_pass():
    state = _make_round_state(
        [
            PlayerState(seat=0, concealed_tiles=(), melds=(), flowers=(), discards=()),
            PlayerState(seat=1, concealed_tiles=(), melds=(), flowers=(), discards=()),
            PlayerState(seat=2, concealed_tiles=(), melds=(), flowers=(), discards=()),
            PlayerState(seat=3, concealed_tiles=(), melds=(), flowers=(), discards=()),
        ],
        None,
        current_actor=0,
    )

    with pytest.raises(ValueError):
        apply_claim_action(state, seat=1, action_type="pass", tiles=[])


def test_apply_claim_action_rejects_pass_without_offer():
    discard = _make_suit_tile("t2", "t2#discard")
    players = [
        PlayerState(
            seat=0,
            concealed_tiles=(),
            melds=(),
            flowers=(),
            discards=(discard,),
        ),
        PlayerState(seat=1, concealed_tiles=(), melds=(), flowers=(), discards=()),
        PlayerState(seat=2, concealed_tiles=(), melds=(), flowers=(), discards=()),
        PlayerState(seat=3, concealed_tiles=(), melds=(), flowers=(), discards=()),
    ]
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=_make_wall([]),
        players=tuple(players),
        last_discard=discard,
        pending_action={
            "type": "claim_window",
            "discarder_seat": 0,
            "claim_window": [[], ["pung"], [], []],
        },
        phase="playing",
        settlement=None,
        version=0,
    )

    with pytest.raises(ValueError):
        apply_claim_action(state, seat=2, action_type="pass", tiles=[])


def test_pass_keeps_claim_window_open_until_all_offers_are_resolved():
    discard = _make_suit_tile("t2", "t2#discard")
    players = [
        PlayerState(
            seat=0,
            concealed_tiles=(),
            melds=(),
            flowers=(),
            discards=(discard,),
        ),
        PlayerState(seat=1, concealed_tiles=(), melds=(), flowers=(), discards=()),
        PlayerState(seat=2, concealed_tiles=(), melds=(), flowers=(), discards=()),
        PlayerState(seat=3, concealed_tiles=(), melds=(), flowers=(), discards=()),
    ]
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=_make_wall([]),
        players=tuple(players),
        last_discard=discard,
        pending_action={
            "type": "claim_window",
            "discarder_seat": 0,
            "claim_window": [[], ["pung"], ["hu"], []],
            "responded_seats": [],
        },
        phase="playing",
        settlement=None,
        version=0,
    )

    next_state, events = apply_claim_action(state, seat=1, action_type="pass", tiles=[])

    assert events == []
    assert next_state.current_actor == 0
    assert next_state.pending_action == {
        "type": "claim_window",
        "discarder_seat": 0,
        "claim_window": [[], ["pung"], ["hu"], []],
        "responded_seats": [1],
        "claim_responses": [],
    }


def test_apply_claim_action_rejects_claim_after_seat_already_passed():
    discard = _make_suit_tile("t4", "t4#discard")
    players = [
        PlayerState(
            seat=0,
            concealed_tiles=(),
            melds=(),
            flowers=(),
            discards=(discard,),
        ),
        PlayerState(
            seat=1,
            concealed_tiles=(
                _make_suit_tile("t4", "t4#c1"),
                _make_suit_tile("t4", "t4#c2"),
            ),
            melds=(),
            flowers=(),
            discards=(),
        ),
        PlayerState(seat=2, concealed_tiles=(), melds=(), flowers=(), discards=()),
        PlayerState(seat=3, concealed_tiles=(), melds=(), flowers=(), discards=()),
    ]
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=_make_wall([]),
        players=tuple(players),
        last_discard=discard,
        pending_action={
            "type": "claim_window",
            "discarder_seat": 0,
            "claim_window": [[], ["pung"], [], []],
            "responded_seats": [1],
        },
        phase="playing",
        settlement=None,
        version=0,
    )

    with pytest.raises(ValueError):
        apply_claim_action(
            state,
            seat=1,
            action_type="pung",
            tiles=["t4#c1", "t4#c2"],
        )


def test_claim_keeps_discard_in_river():
    discard = _make_suit_tile("t7", "t7#discard")
    claimant_tiles = (
        _make_suit_tile("t7", "t7#c1"),
        _make_suit_tile("t7", "t7#c2"),
    )
    players = [
        PlayerState(
            seat=0,
            concealed_tiles=(),
            melds=(),
            flowers=(),
            discards=(discard,),
        ),
        PlayerState(
            seat=1,
            concealed_tiles=claimant_tiles,
            melds=(),
            flowers=(),
            discards=(),
        ),
        PlayerState(seat=2, concealed_tiles=(), melds=(), flowers=(), discards=()),
        PlayerState(seat=3, concealed_tiles=(), melds=(), flowers=(), discards=()),
    ]
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=_make_wall([]),
        players=tuple(players),
        last_discard=discard,
        pending_action={
            "type": "claim_window",
            "discarder_seat": 0,
            "claim_window": [[], ["pung"], [], []],
        },
        phase="playing",
        settlement=None,
        version=0,
    )

    next_state, _ = apply_claim_action(
        state,
        seat=1,
        action_type="pung",
        tiles=[tile.tile_id for tile in claimant_tiles],
    )

    assert next_state.players[0].discards == (discard,)


def test_claim_window_offers_hu_when_hand_is_winning():
    discard = _make_suit_tile("w7", "w7#discard")
    players = [
        PlayerState(
            seat=0,
            concealed_tiles=(),
            melds=(),
            flowers=(),
            discards=(discard,),
        ),
        PlayerState(
            seat=1,
            concealed_tiles=(
                _make_suit_tile("w1", "w1#p1"),
                _make_suit_tile("w1", "w1#p1b"),
                _make_suit_tile("w2", "w2#p1"),
                _make_suit_tile("w2", "w2#p1b"),
                _make_suit_tile("w3", "w3#p1"),
                _make_suit_tile("w3", "w3#p1b"),
                _make_suit_tile("w4", "w4#p1"),
                _make_suit_tile("w4", "w4#p1b"),
                _make_suit_tile("w5", "w5#p1"),
                _make_suit_tile("w5", "w5#p1b"),
                _make_suit_tile("w6", "w6#p1"),
                _make_suit_tile("w6", "w6#p1b"),
                _make_suit_tile("w7", "w7#p1"),
            ),
            melds=(),
            flowers=(),
            discards=(),
        ),
        PlayerState(seat=2, concealed_tiles=(), melds=(), flowers=(), discards=()),
        PlayerState(seat=3, concealed_tiles=(), melds=(), flowers=(), discards=()),
    ]
    state = _make_round_state(players, discard, current_actor=0)

    claim_window = compute_claim_window(state)

    assert "hu" in claim_window[1]


def test_claim_window_does_not_offer_hu_below_eight_fan_minimum():
    discard = _make_honor_tile("red", "red#discard", "dragon")
    players = [
        PlayerState(
            seat=0,
            concealed_tiles=(),
            melds=(),
            flowers=(),
            discards=(discard,),
        ),
        PlayerState(
            seat=1,
            concealed_tiles=(
                _make_suit_tile("w1", "w1#p1"),
                _make_suit_tile("w2", "w2#p1"),
                _make_suit_tile("w3", "w3#p1"),
                _make_suit_tile("t4", "t4#p1"),
                _make_suit_tile("t5", "t5#p1"),
                _make_suit_tile("t6", "t6#p1"),
                _make_suit_tile("b2", "b2#p1"),
                _make_suit_tile("b3", "b3#p1"),
                _make_suit_tile("b4", "b4#p1"),
                _make_honor_tile("red", "red#p1", "dragon"),
            ),
            melds=((
                _make_suit_tile("w7", "w7#m1"),
                _make_suit_tile("w8", "w8#m2"),
                _make_suit_tile("w9", "w9#m3"),
            ),),
            flowers=(),
            discards=(),
        ),
        PlayerState(seat=2, concealed_tiles=(), melds=(), flowers=(), discards=()),
        PlayerState(seat=3, concealed_tiles=(), melds=(), flowers=(), discards=()),
    ]
    state = _make_round_state(players, discard, current_actor=0)

    claim_window = compute_claim_window(state)

    assert "hu" not in claim_window[1]


def test_claim_window_blocks_non_hu_claims_after_last_live_tile_discard():
    discard = _make_suit_tile("w3", "w3#discard")
    players = [
        PlayerState(
            seat=0,
            concealed_tiles=(),
            melds=(),
            flowers=(),
            discards=(discard,),
        ),
        PlayerState(
            seat=1,
            concealed_tiles=(
                _make_suit_tile("w1", "w1#p1"),
                _make_suit_tile("w2", "w2#p1"),
                _make_suit_tile("w3", "w3#p1a"),
                _make_suit_tile("w3", "w3#p1b"),
            ),
            melds=(),
            flowers=(),
            discards=(),
        ),
        PlayerState(seat=2, concealed_tiles=(), melds=(), flowers=(), discards=()),
        PlayerState(seat=3, concealed_tiles=(), melds=(), flowers=(), discards=()),
    ]
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=_make_wall([]),
        players=tuple(players),
        last_discard=discard,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
        last_action_context={
            "kind": "discard",
            "seat": 0,
            "tile_id": discard.tile_id,
            "from_kong_replacement": False,
            "was_last_live_tile": False,
            "was_last_discard": True,
        },
    )

    claim_window = compute_claim_window(state)

    assert claim_window[1] == set()


def test_apply_claim_action_rejects_invalid_chow_sequence():
    discard = _make_suit_tile("t3", "t3#discard")
    players = [
        PlayerState(
            seat=0,
            concealed_tiles=(),
            melds=(),
            flowers=(),
            discards=(discard,),
        ),
        PlayerState(
            seat=1,
            concealed_tiles=(
                _make_suit_tile("t1", "t1#c1"),
                _make_suit_tile("t4", "t4#c2"),
            ),
            melds=(),
            flowers=(),
            discards=(),
        ),
        PlayerState(seat=2, concealed_tiles=(), melds=(), flowers=(), discards=()),
        PlayerState(seat=3, concealed_tiles=(), melds=(), flowers=(), discards=()),
    ]
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=_make_wall([]),
        players=tuple(players),
        last_discard=discard,
        pending_action={
            "type": "claim_window",
            "discarder_seat": 0,
            "claim_window": [[], ["chow"], [], []],
        },
        phase="playing",
        settlement=None,
        version=0,
    )

    with pytest.raises(ValueError):
        apply_claim_action(
            state,
            seat=1,
            action_type="chow",
            tiles=["t1#c1", "t4#c2"],
        )


def test_apply_claim_action_rejects_mismatched_pung_tiles():
    discard = _make_suit_tile("t8", "t8#discard")
    players = [
        PlayerState(
            seat=0,
            concealed_tiles=(),
            melds=(),
            flowers=(),
            discards=(discard,),
        ),
        PlayerState(
            seat=1,
            concealed_tiles=(
                _make_suit_tile("t8", "t8#c1"),
                _make_suit_tile("t9", "t9#c2"),
            ),
            melds=(),
            flowers=(),
            discards=(),
        ),
        PlayerState(seat=2, concealed_tiles=(), melds=(), flowers=(), discards=()),
        PlayerState(seat=3, concealed_tiles=(), melds=(), flowers=(), discards=()),
    ]
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=_make_wall([]),
        players=tuple(players),
        last_discard=discard,
        pending_action={
            "type": "claim_window",
            "discarder_seat": 0,
            "claim_window": [[], ["pung"], [], []],
        },
        phase="playing",
        settlement=None,
        version=0,
    )

    with pytest.raises(ValueError):
        apply_claim_action(
            state,
            seat=1,
            action_type="pung",
            tiles=["t8#c1", "t9#c2"],
        )


def test_apply_claim_action_rejects_mismatched_kong_tiles():
    discard = _make_suit_tile("t6", "t6#discard")
    players = [
        PlayerState(
            seat=0,
            concealed_tiles=(),
            melds=(),
            flowers=(),
            discards=(discard,),
        ),
        PlayerState(
            seat=1,
            concealed_tiles=(
                _make_suit_tile("t6", "t6#c1"),
                _make_suit_tile("t6", "t6#c2"),
                _make_suit_tile("t7", "t7#c3"),
            ),
            melds=(),
            flowers=(),
            discards=(),
        ),
        PlayerState(seat=2, concealed_tiles=(), melds=(), flowers=(), discards=()),
        PlayerState(seat=3, concealed_tiles=(), melds=(), flowers=(), discards=()),
    ]
    state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=_make_wall([]),
        players=tuple(players),
        last_discard=discard,
        pending_action={
            "type": "claim_window",
            "discarder_seat": 0,
            "claim_window": [[], ["kong"], [], []],
        },
        phase="playing",
        settlement=None,
        version=0,
    )

    with pytest.raises(ValueError):
        apply_claim_action(
            state,
            seat=1,
            action_type="kong",
            tiles=["t6#c1", "t6#c2", "t7#c3"],
        )
