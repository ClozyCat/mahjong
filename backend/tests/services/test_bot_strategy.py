from app.domain.models import PlayerState, RoundState, Tile
from app.domain.wall import WallState
from app.services.bot_strategy import (
    _tile_risk_against_opponent,
    choose_active_turn_action,
    choose_claim_action,
    choose_discard,
)


def _make_tile(tile_key: str, tile_id: str) -> Tile:
    if tile_key in {"east", "south", "west", "north"}:
        return Tile(
            tile_id=tile_id,
            tile_key=tile_key,
            kind="wind",
            suit=None,
            rank=None,
            name=tile_key,
        )
    if tile_key in {"red", "green", "white"}:
        return Tile(
            tile_id=tile_id,
            tile_key=tile_key,
            kind="dragon",
            suit=None,
            rank=None,
            name=tile_key,
        )
    return Tile(
        tile_id=tile_id,
        tile_key=tile_key,
        kind="suit",
        suit={"w": "characters", "t": "bamboos", "b": "dots"}[tile_key[0]],
        rank=int(tile_key[1:]),
        name=tile_key,
    )


def _make_tiles(tile_keys: list[str], prefix: str) -> tuple[Tile, ...]:
    return tuple(_make_tile(tile_key, f"{prefix}-{index}-{tile_key}") for index, tile_key in enumerate(tile_keys))


def test_choose_discard_prefers_isolated_honor_over_connected_tiles() -> None:
    player = PlayerState(
        seat=0,
        concealed_tiles=_make_tiles(
            [
                "east",
                "red",
                "red",
                "w2",
                "w3",
                "w4",
                "t2",
                "t3",
                "t4",
                "b3",
                "b4",
                "b5",
                "w7",
                "w8",
            ],
            "discard",
        ),
        melds=(),
        flowers=(),
        discards=(),
    )

    discard = choose_discard(player)

    assert discard.tile_key == "east"


def test_choose_claim_action_takes_hu_when_available() -> None:
    discard = _make_tile("w5", "discard-w5")
    state = RoundState(
        round_id="claim-hu",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(
            PlayerState(seat=0, concealed_tiles=(), melds=(), flowers=(), discards=(discard,)),
            PlayerState(
                seat=1,
                concealed_tiles=_make_tiles(
                    [
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
                    "hu",
                ),
                melds=(),
                flowers=(),
                discards=(),
            ),
            PlayerState(seat=2, concealed_tiles=(), melds=(), flowers=(), discards=()),
            PlayerState(seat=3, concealed_tiles=(), melds=(), flowers=(), discards=()),
        ),
        last_discard=discard,
        pending_action={
            "type": "claim_window",
            "discarder_seat": 0,
            "claim_window": [[], ["hu"], [], []],
            "responded_seats": [],
        },
        phase="playing",
        settlement=None,
        version=0,
        score_trackers={"kong_entries": []},
        last_action_context=None,
        round_wind="east",
        enforce_minimum_eight_fan=False,
    )

    decision = choose_claim_action(state, 1)

    assert decision.action_type == "hu"
    assert decision.tile_ids == []


def test_choose_claim_action_pungs_value_honor_when_shape_stays_good() -> None:
    discard = _make_tile("red", "discard-red")
    state = RoundState(
        round_id="claim-pung",
        dealer_seat=0,
        current_actor=2,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(
            PlayerState(seat=0, concealed_tiles=(), melds=(), flowers=(), discards=()),
            PlayerState(
                seat=1,
                concealed_tiles=_make_tiles(
                    ["red", "red", "w1", "w1", "w1", "w2", "w2", "w2", "t4", "t5"],
                    "pung",
                ),
                melds=(_make_tiles(["b1", "b2", "b3"], "meld"),),
                flowers=(),
                discards=(),
            ),
            PlayerState(
                seat=2,
                concealed_tiles=(),
                melds=(),
                flowers=(),
                discards=(discard,),
            ),
            PlayerState(seat=3, concealed_tiles=(), melds=(), flowers=(), discards=()),
        ),
        last_discard=discard,
        pending_action={
            "type": "claim_window",
            "discarder_seat": 2,
            "claim_window": [[], ["pung"], [], []],
            "responded_seats": [],
        },
        phase="playing",
        settlement=None,
        version=0,
        score_trackers={"kong_entries": []},
        last_action_context=None,
        round_wind="east",
    )

    decision = choose_claim_action(state, 1)

    assert decision.action_type == "pung"
    assert len(decision.tile_ids) == 2


def test_choose_claim_action_chows_when_it_clearly_improves_hand() -> None:
    discard = _make_tile("w4", "discard-w4")
    state = RoundState(
        round_id="claim-chow",
        dealer_seat=0,
        current_actor=3,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(
            PlayerState(seat=0, concealed_tiles=(), melds=(), flowers=(), discards=()),
            PlayerState(
                seat=1,
                concealed_tiles=_make_tiles(
                    [
                        "w1",
                        "w1",
                        "w1",
                        "w2",
                        "w3",
                        "t4",
                        "t5",
                        "t6",
                        "b7",
                        "b8",
                        "east",
                        "east",
                        "south",
                    ],
                    "chow",
                ),
                melds=(),
                flowers=(),
                discards=(),
            ),
            PlayerState(seat=2, concealed_tiles=(), melds=(), flowers=(), discards=()),
            PlayerState(
                seat=3,
                concealed_tiles=(),
                melds=(),
                flowers=(),
                discards=(discard,),
            ),
        ),
        last_discard=discard,
        pending_action={
            "type": "claim_window",
            "discarder_seat": 3,
            "claim_window": [[], ["chow"], [], []],
            "responded_seats": [],
        },
        phase="playing",
        settlement=None,
        version=0,
        score_trackers={"kong_entries": []},
        last_action_context=None,
        round_wind="east",
    )

    decision = choose_claim_action(state, 1)

    assert decision.action_type == "chow"
    assert len(decision.tile_ids) == 2


def test_choose_active_turn_action_uses_defense_against_threatening_open_hand() -> None:
    concealed_tiles = _make_tiles(
        [
            "east",
            "east",
            "red",
            "red",
            "t2",
            "t3",
            "t4",
            "b2",
            "b3",
            "b4",
            "w2",
            "w3",
            "w5",
            "w5",
        ],
        "defense",
    )
    state = RoundState(
        round_id="defense-discard",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(
            PlayerState(seat=0, concealed_tiles=concealed_tiles, melds=(), flowers=(), discards=()),
            PlayerState(
                seat=1,
                concealed_tiles=(),
                melds=(
                    _make_tiles(["w3", "w4", "w5"], "threat-m1"),
                    _make_tiles(["t4", "t5", "t6"], "threat-m2"),
                ),
                flowers=(),
                discards=_make_tiles(["east", "w1", "w9", "b1", "b9", "t1", "t9"], "threat-d"),
            ),
            PlayerState(
                seat=2,
                concealed_tiles=(),
                melds=(),
                flowers=(),
                discards=_make_tiles(["w2", "w8", "b2", "b8"], "p2d"),
            ),
            PlayerState(
                seat=3,
                concealed_tiles=(),
                melds=(),
                flowers=(),
                discards=_make_tiles(["t2", "t8", "w6"], "p3d"),
            ),
        ),
        last_discard=None,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
        score_trackers={"kong_entries": []},
        last_action_context=None,
        round_wind="east",
        enforce_minimum_eight_fan=False,
    )

    plain_discard = choose_discard(state.players[0])
    defensive_decision = choose_active_turn_action(state, 0, aggression=0.2)
    defensive_tile = next(
        tile for tile in state.players[0].concealed_tiles if tile.tile_id == defensive_decision.tile_ids[0]
    )

    assert plain_discard.tile_key == "w5"
    assert defensive_tile.tile_key == "east"


def test_tile_risk_model_orders_genbutsu_suji_and_live_tiles() -> None:
    state = RoundState(
        round_id="risk-genbutsu-suji",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(
            PlayerState(
                seat=0,
                concealed_tiles=_make_tiles(["t5", "w4", "w5"], "risk-self"),
                melds=(),
                flowers=(),
                discards=(),
            ),
            PlayerState(
                seat=1,
                concealed_tiles=(),
                melds=(
                    _make_tiles(["b2", "b3", "b4"], "risk-m1"),
                    _make_tiles(["t3", "t4", "t5"], "risk-m2"),
                ),
                flowers=(),
                discards=_make_tiles(["t5", "w1", "w7", "b1", "b9", "t1", "t9"], "risk-d"),
            ),
            PlayerState(seat=2, concealed_tiles=(), melds=(), flowers=(), discards=()),
            PlayerState(seat=3, concealed_tiles=(), melds=(), flowers=(), discards=()),
        ),
        last_discard=None,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
        score_trackers={"kong_entries": []},
        last_action_context=None,
        round_wind="east",
        enforce_minimum_eight_fan=False,
    )

    genbutsu_risk = _tile_risk_against_opponent(state, seat_index=0, opponent_seat=1, tile=_make_tile("t5", "t5#genbutsu"))
    suji_risk = _tile_risk_against_opponent(state, seat_index=0, opponent_seat=1, tile=_make_tile("w4", "w4#suji"))
    live_risk = _tile_risk_against_opponent(state, seat_index=0, opponent_seat=1, tile=_make_tile("w5", "w5#live"))

    assert genbutsu_risk < suji_risk < live_risk


def test_tile_risk_model_distinguishes_complete_kabe_and_one_chance() -> None:
    state = RoundState(
        round_id="risk-kabe",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(
            PlayerState(
                seat=0,
                concealed_tiles=_make_tiles(["w1", "t1", "b1"], "kabe-self"),
                melds=(),
                flowers=(),
                discards=(),
            ),
            PlayerState(
                seat=1,
                concealed_tiles=(),
                melds=(
                    _make_tiles(["w3", "w4", "w5"], "kabe-threat-1"),
                    _make_tiles(["t3", "t4", "t5"], "kabe-threat-2"),
                ),
                flowers=(),
                discards=_make_tiles(["w9", "t9", "b9", "east", "south", "west"], "kabe-threat-d"),
            ),
            PlayerState(
                seat=2,
                concealed_tiles=(),
                melds=(_make_tiles(["w2", "w2", "w2"], "full-kabe"),),
                flowers=(),
                discards=(_make_tile("w2", "full-kabe-discard"),),
            ),
            PlayerState(
                seat=3,
                concealed_tiles=(),
                melds=(_make_tiles(["t2", "t2", "t2"], "one-chance"),),
                flowers=(),
                discards=(),
            ),
        ),
        last_discard=None,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
        score_trackers={"kong_entries": []},
        last_action_context=None,
        round_wind="east",
        enforce_minimum_eight_fan=False,
    )

    full_kabe_risk = _tile_risk_against_opponent(state, seat_index=0, opponent_seat=1, tile=_make_tile("w1", "w1#full-kabe"))
    one_chance_risk = _tile_risk_against_opponent(state, seat_index=0, opponent_seat=1, tile=_make_tile("t1", "t1#one-chance"))
    live_risk = _tile_risk_against_opponent(state, seat_index=0, opponent_seat=1, tile=_make_tile("b1", "b1#live"))

    assert full_kabe_risk < one_chance_risk < live_risk


def test_choose_active_turn_action_personas_push_and_fold_differently() -> None:
    concealed_tiles = _make_tiles(
        [
            "east",
            "east",
            "red",
            "red",
            "t2",
            "t3",
            "t4",
            "b2",
            "b3",
            "b4",
            "w2",
            "w3",
            "w5",
            "w5",
        ],
        "persona-defense",
    )
    state = RoundState(
        round_id="persona-defense-discard",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(
            PlayerState(seat=0, concealed_tiles=concealed_tiles, melds=(), flowers=(), discards=()),
            PlayerState(
                seat=1,
                concealed_tiles=(),
                melds=(
                    _make_tiles(["w3", "w4", "w5"], "persona-threat-m1"),
                    _make_tiles(["t4", "t5", "t6"], "persona-threat-m2"),
                ),
                flowers=(),
                discards=_make_tiles(["east", "w1", "w9", "b1", "b9", "t1", "t9"], "persona-threat-d"),
            ),
            PlayerState(
                seat=2,
                concealed_tiles=(),
                melds=(),
                flowers=(),
                discards=_make_tiles(["w2", "w8", "b2", "b8"], "persona-p2d"),
            ),
            PlayerState(
                seat=3,
                concealed_tiles=(),
                melds=(),
                flowers=(),
                discards=_make_tiles(["t2", "t8", "w6"], "persona-p3d"),
            ),
        ),
        last_discard=None,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
        score_trackers={"kong_entries": []},
        last_action_context=None,
        round_wind="east",
        enforce_minimum_eight_fan=False,
    )

    attacker = choose_active_turn_action(state, 0, aggression=0.85, persona="menzen_attacker")
    balanced = choose_active_turn_action(state, 0, aggression=0.5, persona="balanced")
    defender = choose_active_turn_action(state, 0, aggression=0.2, persona="defender")

    attacker_tile = next(tile for tile in concealed_tiles if tile.tile_id == attacker.tile_ids[0])
    balanced_tile = next(tile for tile in concealed_tiles if tile.tile_id == balanced.tile_ids[0])
    defender_tile = next(tile for tile in concealed_tiles if tile.tile_id == defender.tile_ids[0])

    assert attacker_tile.tile_key == "w5"
    assert balanced_tile.tile_key == "east"
    assert defender_tile.tile_key == "east"


def test_choose_claim_action_uses_aggression_to_break_ties_under_pressure() -> None:
    discard = _make_tile("w4", "discard-pressure-w4")
    state = RoundState(
        round_id="claim-pressure",
        dealer_seat=0,
        current_actor=3,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(
            PlayerState(
                seat=0,
                concealed_tiles=(),
                melds=(
                    _make_tiles(["b1", "b2", "b3"], "threat-open-1"),
                    _make_tiles(["t1", "t2", "t3"], "threat-open-2"),
                ),
                flowers=(),
                discards=_make_tiles(["east", "w1", "w9", "b1", "b9", "t1", "t9"], "threat-open-d"),
            ),
            PlayerState(
                seat=1,
                concealed_tiles=_make_tiles(
                    ["w1", "w1", "w1", "w2", "w3", "t4", "t5", "t6", "b7", "b8", "east", "w5", "w7"],
                    "claim-pressure",
                ),
                melds=(),
                flowers=(),
                discards=(),
            ),
            PlayerState(seat=2, concealed_tiles=(), melds=(), flowers=(), discards=()),
            PlayerState(
                seat=3,
                concealed_tiles=(),
                melds=(),
                flowers=(),
                discards=(discard,),
            ),
        ),
        last_discard=discard,
        pending_action={
            "type": "claim_window",
            "discarder_seat": 3,
            "claim_window": [[], ["chow"], [], []],
            "responded_seats": [],
        },
        phase="playing",
        settlement=None,
        version=0,
        score_trackers={"kong_entries": []},
        last_action_context=None,
        round_wind="east",
        enforce_minimum_eight_fan=False,
    )

    cautious = choose_claim_action(state, 1, aggression=0.2)
    aggressive = choose_claim_action(state, 1, aggression=0.85)

    assert cautious.action_type == "pass"
    assert aggressive.action_type == "chow"


def test_choose_claim_action_personas_diverge_on_marginal_chow_under_pressure() -> None:
    discard = _make_tile("w4", "discard-persona-w4")
    state = RoundState(
        round_id="claim-persona-pressure",
        dealer_seat=0,
        current_actor=3,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(
            PlayerState(
                seat=0,
                concealed_tiles=(),
                melds=(
                    _make_tiles(["b1", "b2", "b3"], "claim-persona-threat-1"),
                    _make_tiles(["t1", "t2", "t3"], "claim-persona-threat-2"),
                ),
                flowers=(),
                discards=_make_tiles(["east", "w1", "w9", "b1", "b9", "t1", "t9"], "claim-persona-threat-d"),
            ),
            PlayerState(
                seat=1,
                concealed_tiles=_make_tiles(
                    ["w1", "w1", "w1", "w2", "w3", "t4", "t5", "t6", "b7", "b8", "east", "w5", "w7"],
                    "claim-persona",
                ),
                melds=(),
                flowers=(),
                discards=(),
            ),
            PlayerState(seat=2, concealed_tiles=(), melds=(), flowers=(), discards=()),
            PlayerState(
                seat=3,
                concealed_tiles=(),
                melds=(),
                flowers=(),
                discards=(discard,),
            ),
        ),
        last_discard=discard,
        pending_action={
            "type": "claim_window",
            "discarder_seat": 3,
            "claim_window": [[], ["chow"], [], []],
            "responded_seats": [],
        },
        phase="playing",
        settlement=None,
        version=0,
        score_trackers={"kong_entries": []},
        last_action_context=None,
        round_wind="east",
        enforce_minimum_eight_fan=False,
    )

    attacker = choose_claim_action(state, 1, aggression=0.85, persona="menzen_attacker")
    balanced = choose_claim_action(state, 1, aggression=0.85, persona="balanced")
    defender = choose_claim_action(state, 1, aggression=0.85, persona="defender")

    assert attacker.action_type == "pass"
    assert balanced.action_type == "chow"
    assert defender.action_type == "pass"


def test_choose_active_turn_action_declares_self_hu_before_discarding() -> None:
    state = RoundState(
        round_id="self-hu",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(
            PlayerState(
                seat=0,
                concealed_tiles=_make_tiles(
                    [
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
                        "w5",
                    ],
                    "self-hu",
                ),
                melds=(),
                flowers=(),
                discards=(),
            ),
            PlayerState(seat=1, concealed_tiles=(), melds=(), flowers=(), discards=()),
            PlayerState(seat=2, concealed_tiles=(), melds=(), flowers=(), discards=()),
            PlayerState(seat=3, concealed_tiles=(), melds=(), flowers=(), discards=()),
        ),
        last_discard=None,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
        score_trackers={"kong_entries": []},
        last_action_context=None,
        round_wind="east",
        enforce_minimum_eight_fan=False,
    )

    decision = choose_active_turn_action(state, 0)

    assert decision.action_type == "hu"
    assert decision.tile_ids == []


def test_choose_active_turn_action_avoids_restricted_same_turn_discard() -> None:
    state = RoundState(
        round_id="restricted-discard",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(
            PlayerState(
                seat=0,
                concealed_tiles=_make_tiles(["w6", "b1"], "restricted"),
                melds=(_make_tiles(["w4", "w5", "w6"], "claim-meld"),),
                flowers=(),
                discards=(),
            ),
            PlayerState(seat=1, concealed_tiles=(), melds=(), flowers=(), discards=()),
            PlayerState(seat=2, concealed_tiles=(), melds=(), flowers=(), discards=()),
            PlayerState(seat=3, concealed_tiles=(), melds=(), flowers=(), discards=()),
        ),
        last_discard=None,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
        score_trackers={"kong_entries": []},
        last_action_context=None,
        round_wind="east",
        enforce_minimum_eight_fan=False,
        restricted_discard_tile_key="w6",
    )

    decision = choose_active_turn_action(state, 0)

    assert decision.action_type == "discard"
    assert decision.tile_ids == ["restricted-1-b1"]
