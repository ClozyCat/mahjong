import pytest
from sqlalchemy.orm import sessionmaker
from starlette.websockets import WebSocketDisconnect

from app.domain.models import PlayerState, RoundState, Tile
from app.domain.reducer import initialize_round
from app.domain.wall import WallState
from app.services.bot_strategy import BotDecision
from app.services import game_service as game_service_module
from app.services.game_service import GameService, MatchState, RoomState, SeatReservation
from app.services.timeout_service import PendingTimeout


class _DisconnectingWebSocket:
    async def send_json(self, _message):
        raise WebSocketDisconnect(code=1006)


class _ClosedSendWebSocket:
    async def send_json(self, _message):
        raise RuntimeError('Cannot call "send" once a close message has been sent.')


@pytest.mark.asyncio
async def test_send_presence_and_snapshots_ignores_disconnected_peers():
    service = GameService(sessionmaker())

    await service._send_presence_and_snapshots(
        table_code="ROOM42",
        seat_index=0,
        connected=False,
        peer_updates=[
            (_DisconnectingWebSocket(), {"type": "room_snapshot", "payload": {}}),
        ],
    )


@pytest.mark.asyncio
async def test_send_presence_and_snapshots_ignores_peers_after_close_message():
    service = GameService(sessionmaker())

    await service._send_presence_and_snapshots(
        table_code="ROOM42",
        seat_index=0,
        connected=False,
        peer_updates=[
            (_ClosedSendWebSocket(), {"type": "room_snapshot", "payload": {}}),
        ],
    )


class _RecordingWebSocket:
    def __init__(self) -> None:
        self.messages: list[dict] = []

    async def send_json(self, _message):
        self.messages.append(_message)
        return None


def _make_claim_priority_round_state() -> RoundState:
    discard = _make_suit_tile("w5", "w5#discard")
    return RoundState(
        round_id="round-priority",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(
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
                    _make_suit_tile("w5", "w5#p1a"),
                    _make_suit_tile("w5", "w5#p1b"),
                ),
                melds=(),
                flowers=(),
                discards=(),
            ),
            PlayerState(
                seat=2,
                concealed_tiles=(
                    _make_suit_tile("w1", "w1#p2a"),
                    _make_suit_tile("w1", "w1#p2b"),
                    _make_suit_tile("w1", "w1#p2c"),
                    _make_suit_tile("w2", "w2#p2a"),
                    _make_suit_tile("w2", "w2#p2b"),
                    _make_suit_tile("w2", "w2#p2c"),
                    _make_suit_tile("w3", "w3#p2a"),
                    _make_suit_tile("w3", "w3#p2b"),
                    _make_suit_tile("w3", "w3#p2c"),
                    _make_suit_tile("w4", "w4#p2a"),
                    _make_suit_tile("w4", "w4#p2b"),
                    _make_suit_tile("w4", "w4#p2c"),
                    _make_suit_tile("w5", "w5#p2a"),
                ),
                melds=(),
                flowers=(),
                discards=(),
            ),
            PlayerState(seat=3, concealed_tiles=(), melds=(), flowers=(), discards=()),
        ),
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
        score_trackers={"kong_entries": []},
        last_action_context=None,
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


def _make_suit_tile(tile_key: str, tile_id: str) -> Tile:
    suit_map = {"w": "characters", "t": "bamboos", "b": "dots"}
    return Tile(
        tile_id=tile_id,
        tile_key=tile_key,
        kind="suit",
        suit=suit_map[tile_key[0]],
        rank=int(tile_key[1:]),
        name=f"Test {tile_key}",
    )


def _make_settlement_room(
    *,
    table_code: str,
    match_state: MatchState,
    settlement: dict,
    round_wind: str | None = None,
    players: tuple[PlayerState, ...] | None = None,
) -> tuple[RoomState, _RecordingWebSocket]:
    websocket = _RecordingWebSocket()
    room = RoomState(table_code=table_code, phase="settlement")
    for seat in range(4):
        room.seats[seat] = SeatReservation(
            seat_index=seat,
            nickname=f"P{seat}",
            reconnect_token=f"token-{seat}",
            player_session_id=seat + 1,
            websocket=websocket if seat == 0 else _RecordingWebSocket(),
            connected=True,
            ready=True,
        )
    room.match_state = match_state
    room.round_state = RoundState(
        round_id=f"{match_state.prevailing_wind}-{match_state.hand_number}-finished",
        dealer_seat=match_state.dealer_seat,
        current_actor=match_state.dealer_seat,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=players
        or tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(4)
        ),
        last_discard=None,
        pending_action=None,
        phase="settlement",
        settlement=settlement,
        version=0,
        score_trackers={"kong_entries": []},
        last_action_context=None,
        round_wind=round_wind or match_state.prevailing_wind,
    )
    return room, websocket


def test_action_prompt_includes_kong_when_current_actor_can_self_kong():
    service = GameService(sessionmaker())
    websocket = _RecordingWebSocket()
    room = RoomState(table_code="ROOM42", phase="playing")
    room.seats[0] = SeatReservation(
        seat_index=0,
        nickname="P0",
        reconnect_token="token",
        player_session_id=1,
        websocket=websocket,
        connected=True,
        ready=True,
    )
    room.round_state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(
            PlayerState(
                seat=0,
                concealed_tiles=(
                    _make_suit_tile("t5", "t5#1"),
                    _make_suit_tile("t5", "t5#2"),
                    _make_suit_tile("t5", "t5#3"),
                    _make_suit_tile("t5", "t5#4"),
                ),
                melds=(),
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
    )
    room.pending_timeout = PendingTimeout(
        kind="active_turn",
        seat_index=0,
        deadline_at=__import__("datetime").datetime.now(
            __import__("datetime").timezone.utc
        ),
        drawn_tile_id="t5#4",
    )

    prompt_targets = service._prompt_targets_locked(room)

    assert prompt_targets[0][1]["payload"]["options"] == ["discard", "kong"]


def test_action_prompt_includes_flower_when_current_actor_can_declare_flower():
    service = GameService(sessionmaker())
    websocket = _RecordingWebSocket()
    room = RoomState(table_code="ROOM42", phase="playing")
    room.seats[0] = SeatReservation(
        seat_index=0,
        nickname="P0",
        reconnect_token="token",
        player_session_id=1,
        websocket=websocket,
        connected=True,
        ready=True,
    )
    room.round_state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(
            PlayerState(
                seat=0,
                concealed_tiles=(
                    _make_honor_tile("f1", "f1#1", "flower"),
                    _make_suit_tile("w1", "w1#1"),
                ),
                melds=(),
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
        last_action_context={
            "kind": "draw",
            "seat": 0,
            "tile_id": "f1#1",
            "from_kong_replacement": False,
            "was_last_live_tile": False,
            "was_last_discard": False,
        },
    )
    room.pending_timeout = PendingTimeout(
        kind="active_turn",
        seat_index=0,
        deadline_at=__import__("datetime").datetime.now(
            __import__("datetime").timezone.utc
        ),
        drawn_tile_id="f1#1",
    )

    prompt_targets = service._prompt_targets_locked(room)

    assert prompt_targets[0][1]["payload"]["options"] == ["discard", "flower"]


def test_action_prompt_offers_pass_during_opening_flowers_when_seat_has_no_flower():
    service = GameService(sessionmaker())
    websocket = _RecordingWebSocket()
    room = RoomState(table_code="ROOM42", phase="playing")
    room.seats[0] = SeatReservation(
        seat_index=0,
        nickname="P0",
        reconnect_token="token",
        player_session_id=1,
        websocket=websocket,
        connected=True,
        ready=True,
    )
    room.round_state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(
            PlayerState(
                seat=0,
                concealed_tiles=(_make_suit_tile("w1", "w1#1"),),
                melds=(),
                flowers=(),
                discards=(),
            ),
        )
        + tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(1, 4)
        ),
        last_discard=None,
        pending_action={"type": "opening_flowers", "dealer_seat": 0},
        phase="playing",
        settlement=None,
        version=0,
        score_trackers={"kong_entries": [], "opening_flowers_completed": False},
    )
    room.pending_timeout = PendingTimeout(
        kind="opening_flowers",
        seat_index=0,
        deadline_at=__import__("datetime").datetime.now(
            __import__("datetime").timezone.utc
        ),
    )

    prompt_targets = service._prompt_targets_locked(room)

    assert prompt_targets[0][1]["payload"]["options"] == ["pass"]


def test_action_prompt_offers_hu_and_pass_for_rob_kong_window():
    service = GameService(sessionmaker())
    websocket = _RecordingWebSocket()
    room = RoomState(table_code="ROOM42", phase="playing")
    room.seats[1] = SeatReservation(
        seat_index=1,
        nickname="P1",
        reconnect_token="token",
        player_session_id=2,
        websocket=websocket,
        connected=True,
        ready=True,
    )
    room.round_state = RoundState(
        round_id="round-test",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(4)
        ),
        last_discard=None,
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
        score_trackers={"kong_entries": []},
        last_action_context=None,
    )
    room.pending_timeout = PendingTimeout(
        kind="claim_window",
        seat_index=0,
        deadline_at=__import__("datetime").datetime.now(
            __import__("datetime").timezone.utc
        ),
    )

    prompt_targets = service._prompt_targets_locked(room)

    assert prompt_targets[0][1]["payload"]["options"] == ["hu", "pass"]


@pytest.mark.asyncio
async def test_send_snapshot_targets_also_sends_match_result_for_settlement():
    service = GameService(sessionmaker())
    websocket = _RecordingWebSocket()

    await service._send_snapshot_targets(
        [
            (
                websocket,
                {
                    "type": "room_snapshot",
                    "payload": {
                        "table_code": "ROOM42",
                        "phase": "settlement",
                        "local_seat": 0,
                        "private_state": {"round_id": "round-42"},
                        "seats": [],
                        "reconnect_token": "token",
                    },
                },
            )
        ]
    )

    assert websocket.messages == [
        {
            "type": "room_snapshot",
            "payload": {
                "table_code": "ROOM42",
                "phase": "settlement",
                "local_seat": 0,
                "private_state": {"round_id": "round-42"},
                "seats": [],
                "reconnect_token": "token",
            },
        },
        {
            "type": "match_result",
            "payload": {
                "table_code": "ROOM42",
                "round_id": "round-42",
                "phase": "settlement",
            },
        },
    ]


@pytest.mark.asyncio
async def test_leave_table_removes_waiting_seat_and_notifies_peers(monkeypatch):
    service = GameService(sessionmaker())
    monkeypatch.setattr(service, "_persist_room_state_locked", lambda _room: None)
    consumed_tokens: list[str] = []
    disconnected_sessions: list[int] = []
    monkeypatch.setattr(service, "_consume_reconnect_token", lambda token: consumed_tokens.append(token))
    monkeypatch.setattr(
        service,
        "_mark_disconnected",
        lambda *, player_session_id: disconnected_sessions.append(player_session_id),
    )

    leaver = _RecordingWebSocket()
    peer = _RecordingWebSocket()
    room = RoomState(table_code="ROOM24", phase="waiting")
    room.seats[0] = SeatReservation(
        seat_index=0,
        nickname="P0",
        reconnect_token="token-0",
        player_session_id=1,
        websocket=leaver,
        connected=True,
        ready=False,
    )
    room.seats[1] = SeatReservation(
        seat_index=1,
        nickname="P1",
        reconnect_token="token-1",
        player_session_id=2,
        websocket=peer,
        connected=True,
        ready=True,
    )
    service._rooms["ROOM24"] = room

    response = await service.leave_table(table_code="ROOM24", websocket=leaver)

    assert response == {
        "type": "leave_table_accepted",
        "payload": {
            "table_code": "ROOM24",
            "seat_index": 0,
        },
    }
    assert 0 not in room.seats
    assert consumed_tokens == ["token-0"]
    assert disconnected_sessions == [1]
    assert peer.messages[0] == {
        "type": "player_presence",
        "payload": {
            "table_code": "ROOM24",
            "seat_index": 0,
            "connected": False,
        },
    }
    assert peer.messages[1]["type"] == "room_snapshot"
    assert peer.messages[1]["payload"]["seats"] == [
        {
            "seat_index": 1,
            "nickname": "P1",
            "connected": True,
            "ready": True,
            "is_bot": False,
        }
    ]


@pytest.mark.asyncio
async def test_leave_table_during_active_match_keeps_seat_but_invalidates_reconnect(monkeypatch):
    service = GameService(sessionmaker())
    monkeypatch.setattr(service, "_persist_room_state_locked", lambda _room: None)
    monkeypatch.setattr(service, "_auto_advance_bot_seats_locked", lambda _room: [])
    monkeypatch.setattr(service, "_sync_timeout_task_locked", lambda _room: None)
    monkeypatch.setattr(service, "_roll_bot_persona", lambda: "defender")
    monkeypatch.setattr(service, "_roll_bot_aggression", lambda _persona=None: 0.42)
    consumed_tokens: list[str] = []
    disconnected_sessions: list[int] = []
    monkeypatch.setattr(service, "_consume_reconnect_token", lambda token: consumed_tokens.append(token))
    monkeypatch.setattr(
        service,
        "_mark_disconnected",
        lambda *, player_session_id: disconnected_sessions.append(player_session_id),
    )

    leaver = _RecordingWebSocket()
    peer = _RecordingWebSocket()
    room = RoomState(table_code="ROOM25", phase="playing")
    room.seats[0] = SeatReservation(
        seat_index=0,
        nickname="P0",
        reconnect_token="token-0",
        player_session_id=1,
        websocket=leaver,
        connected=True,
        ready=True,
    )
    room.seats[1] = SeatReservation(
        seat_index=1,
        nickname="P1",
        reconnect_token="token-1",
        player_session_id=2,
        websocket=peer,
        connected=True,
        ready=True,
    )
    room.round_state = initialize_round(seed=123, dealer_seat=0, round_id="round-25", round_wind="east")
    service._rooms["ROOM25"] = room

    response = await service.leave_table(table_code="ROOM25", websocket=leaver)

    assert response == {
        "type": "leave_table_accepted",
        "payload": {
            "table_code": "ROOM25",
            "seat_index": 0,
        },
    }
    assert room.seats[0].websocket is None
    assert room.seats[0].connected is True
    assert room.seats[0].ready is True
    assert room.seats[0].is_bot is True
    assert room.seats[0].bot_persona == "defender"
    assert room.seats[0].bot_aggression == 0.42
    assert room.seats[0].reconnect_token is None
    assert consumed_tokens == ["token-0"]
    assert disconnected_sessions == [1]
    assert peer.messages[0]["type"] == "room_snapshot"
    assert peer.messages[0]["payload"]["seats"][0] == {
        "seat_index": 0,
        "nickname": "P0",
        "connected": True,
        "ready": True,
        "is_bot": True,
    }


@pytest.mark.asyncio
async def test_leave_table_during_active_turn_lets_bot_play_immediately(monkeypatch):
    service = GameService(sessionmaker())
    monkeypatch.setattr(service, "_persist_room_state_locked", lambda _room: None)
    monkeypatch.setattr(service, "_sync_timeout_task_locked", lambda _room: None)
    monkeypatch.setattr(service, "_consume_reconnect_token", lambda _token: None)
    monkeypatch.setattr(service, "_mark_disconnected", lambda **_kwargs: None)
    monkeypatch.setattr(service, "_roll_bot_persona", lambda: "balanced")
    monkeypatch.setattr(service, "_roll_bot_aggression", lambda _persona=None: 0.37)

    leaver = _RecordingWebSocket()
    peer = _RecordingWebSocket()
    room = RoomState(table_code="ROOM26", phase="playing")
    room.seats[0] = SeatReservation(
        seat_index=0,
        nickname="P0",
        reconnect_token="token-0",
        player_session_id=1,
        websocket=leaver,
        connected=True,
        ready=True,
    )
    room.seats[1] = SeatReservation(
        seat_index=1,
        nickname="P1",
        reconnect_token="token-1",
        player_session_id=2,
        websocket=peer,
        connected=True,
        ready=True,
    )
    room.round_state = initialize_round(seed=123, dealer_seat=0, round_id="round-26", round_wind="east")
    service._rooms["ROOM26"] = room

    await service.leave_table(table_code="ROOM26", websocket=leaver)

    assert room.seats[0].is_bot is True
    assert room.seats[0].bot_persona == "balanced"
    assert room.seats[0].bot_aggression == 0.37
    assert any(message["type"] == "round_event" for message in peer.messages)


def test_add_bot_reservations_assigns_persona_and_aggression(monkeypatch) -> None:
    service = GameService(sessionmaker(), test_mode=True)
    persona_values = iter(["menzen_attacker", "balanced", "defender"])
    aggression_values = iter([0.21, 0.44, 0.68])
    monkeypatch.setattr(service, "_roll_bot_persona", lambda: next(persona_values))
    monkeypatch.setattr(service, "_roll_bot_aggression", lambda _persona=None: next(aggression_values))

    room = RoomState(table_code="ROOM-BOTS", test_mode=True)
    room.seats[0] = SeatReservation(
        seat_index=0,
        nickname="Human",
        reconnect_token="token-0",
        player_session_id=1,
        connected=True,
        ready=True,
        is_bot=False,
    )

    service._add_bot_reservations_locked(room)

    assert room.seats[1].is_bot is True
    assert room.seats[1].bot_persona == "menzen_attacker"
    assert room.seats[1].bot_aggression == 0.21
    assert room.seats[2].bot_persona == "balanced"
    assert room.seats[2].bot_aggression == 0.44
    assert room.seats[3].bot_persona == "defender"
    assert room.seats[3].bot_aggression == 0.68


def test_auto_advance_bot_seats_passes_persona_into_active_turn_strategy(monkeypatch) -> None:
    service = GameService(sessionmaker())
    room = RoomState(table_code="ROOM-PERSONA-ACTIVE", phase="playing")
    room.seats[0] = SeatReservation(
        seat_index=0,
        nickname="Bot 0",
        reconnect_token=None,
        player_session_id=-1,
        connected=True,
        ready=True,
        is_bot=True,
        bot_persona="defender",
        bot_aggression=0.31,
    )
    room.round_state = RoundState(
        round_id="room-persona-active",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(
            PlayerState(
                seat=0,
                concealed_tiles=(_make_suit_tile("w1", "w1#self"),),
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
    )
    room.pending_timeout = PendingTimeout(
        kind="active_turn",
        seat_index=0,
        deadline_at=__import__("datetime").datetime.now(__import__("datetime").timezone.utc),
    )

    captured: dict[str, object] = {}

    def fake_choose_active_turn_action(state, seat_index, aggression=0.5, persona="balanced"):
        captured["seat_index"] = seat_index
        captured["aggression"] = aggression
        captured["persona"] = persona
        return BotDecision(action_type="discard", tile_ids=["w1#self"])

    monkeypatch.setattr(game_service_module, "choose_active_turn_action", fake_choose_active_turn_action)
    monkeypatch.setattr(
        service,
        "_resolve_action_locked",
        lambda state, **_kwargs: (__import__("dataclasses").replace(state, phase="settlement"), []),
    )
    monkeypatch.setattr(service, "_sync_timeout_task_locked", lambda _room: None)

    service._auto_advance_bot_seats_locked(room)

    assert captured == {
        "seat_index": 0,
        "aggression": 0.31,
        "persona": "defender",
    }


def test_auto_resolve_claim_window_passes_persona_into_claim_strategy(monkeypatch) -> None:
    service = GameService(sessionmaker())
    room = RoomState(table_code="ROOM-PERSONA-CLAIM", phase="playing")
    room.seats[0] = SeatReservation(
        seat_index=0,
        nickname="Human",
        reconnect_token="token-0",
        player_session_id=1,
        connected=True,
        ready=True,
    )
    room.seats[1] = SeatReservation(
        seat_index=1,
        nickname="Bot 1",
        reconnect_token=None,
        player_session_id=-2,
        connected=True,
        ready=True,
        is_bot=True,
        bot_persona="menzen_attacker",
        bot_aggression=0.77,
    )
    room.round_state = RoundState(
        round_id="room-persona-claim",
        dealer_seat=0,
        current_actor=3,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(4)
        ),
        last_discard=_make_suit_tile("w5", "w5#discard"),
        pending_action={
            "type": "claim_window",
            "discarder_seat": 3,
            "claim_window": [[], ["pung"], [], []],
            "responded_seats": [],
        },
        phase="playing",
        settlement=None,
        version=0,
        score_trackers={"kong_entries": []},
        last_action_context=None,
    )
    room.pending_timeout = PendingTimeout(
        kind="claim_window",
        seat_index=3,
        deadline_at=__import__("datetime").datetime.now(__import__("datetime").timezone.utc),
    )

    captured: dict[str, object] = {}

    def fake_choose_claim_action(state, seat_index, aggression=0.5, persona="balanced"):
        captured["seat_index"] = seat_index
        captured["aggression"] = aggression
        captured["persona"] = persona
        return BotDecision(action_type="pass", tile_ids=[])

    monkeypatch.setattr(game_service_module, "choose_claim_action", fake_choose_claim_action)
    monkeypatch.setattr(
        service,
        "_resolve_action_locked",
        lambda state, **_kwargs: (
            __import__("dataclasses").replace(
                state,
                pending_action={
                    **(state.pending_action or {}),
                    "responded_seats": [1],
                },
            ),
            [],
        ),
    )
    monkeypatch.setattr(service, "_advance_round_locked", lambda _room: None)

    service._auto_resolve_claim_window_locked(room)

    assert captured == {
        "seat_index": 1,
        "aggression": 0.77,
        "persona": "menzen_attacker",
    }


def test_auto_pass_claim_window_in_test_mode_skips_human_hu_prompt() -> None:
    service = GameService(sessionmaker())
    room = RoomState(table_code="ROOMHU", phase="playing", test_mode=True)
    room.seats[0] = SeatReservation(
        seat_index=0,
        nickname="Human",
        reconnect_token="token-0",
        player_session_id=1,
        websocket=_RecordingWebSocket(),
        connected=True,
        ready=True,
        is_bot=False,
    )
    room.seats[1] = SeatReservation(
        seat_index=1,
        nickname="Bot 1",
        reconnect_token=None,
        player_session_id=-2,
        connected=True,
        ready=True,
        is_bot=True,
    )
    room.seats[2] = SeatReservation(
        seat_index=2,
        nickname="Bot 2",
        reconnect_token=None,
        player_session_id=-3,
        connected=True,
        ready=True,
        is_bot=True,
    )
    room.seats[3] = SeatReservation(
        seat_index=3,
        nickname="Bot 3",
        reconnect_token=None,
        player_session_id=-4,
        connected=True,
        ready=True,
        is_bot=True,
    )
    room.round_state = RoundState(
        round_id="round-claim-human",
        dealer_seat=0,
        current_actor=3,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(4)
        ),
        last_discard=_make_suit_tile("w5", "w5#discard"),
        pending_action={
            "type": "claim_window",
            "discarder_seat": 3,
            "claim_window": [["hu"], ["pung"], [], []],
            "responded_seats": [],
        },
        phase="playing",
        settlement=None,
        version=0,
        score_trackers={"kong_entries": []},
        last_action_context=None,
    )
    room.pending_timeout = PendingTimeout(
        kind="claim_window",
        seat_index=3,
        deadline_at=__import__("datetime").datetime.now(__import__("datetime").timezone.utc),
    )

    messages = service._auto_resolve_claim_window_locked(room)

    assert messages == []
    assert room.pending_timeout is not None
    assert room.round_state is not None
    assert room.round_state.pending_action is not None
    assert room.round_state.pending_action["type"] == "claim_window"
    assert room.round_state.pending_action["responded_seats"] == [1]


def test_auto_advance_bot_seats_stops_when_only_human_claims_remain(monkeypatch) -> None:
    service = GameService(sessionmaker())
    room = RoomState(table_code="ROOMHU2", phase="playing", test_mode=True)
    room.seats[0] = SeatReservation(
        seat_index=0,
        nickname="Human",
        reconnect_token="token-0",
        player_session_id=1,
        websocket=_RecordingWebSocket(),
        connected=True,
        ready=True,
        is_bot=False,
    )
    room.seats[1] = SeatReservation(
        seat_index=1,
        nickname="Bot 1",
        reconnect_token=None,
        player_session_id=-2,
        connected=True,
        ready=True,
        is_bot=True,
    )
    room.seats[2] = SeatReservation(
        seat_index=2,
        nickname="Bot 2",
        reconnect_token=None,
        player_session_id=-3,
        connected=True,
        ready=True,
        is_bot=True,
    )
    room.seats[3] = SeatReservation(
        seat_index=3,
        nickname="Bot 3",
        reconnect_token=None,
        player_session_id=-4,
        connected=True,
        ready=True,
        is_bot=True,
    )
    room.round_state = RoundState(
        round_id="round-claim-human-2",
        dealer_seat=0,
        current_actor=3,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(4)
        ),
        last_discard=_make_suit_tile("w5", "w5#discard"),
        pending_action={
            "type": "claim_window",
            "discarder_seat": 3,
            "claim_window": [["hu"], ["pung"], [], []],
            "responded_seats": [],
        },
        phase="playing",
        settlement=None,
        version=0,
        score_trackers={"kong_entries": []},
        last_action_context=None,
    )
    room.pending_timeout = PendingTimeout(
        kind="claim_window",
        seat_index=3,
        deadline_at=__import__("datetime").datetime.now(__import__("datetime").timezone.utc),
    )

    original = service._auto_resolve_claim_window_locked
    call_count = 0

    def wrapped(target_room):
        nonlocal call_count
        call_count += 1
        if call_count > 2:
            raise AssertionError("claim window auto-pass loop did not stop")
        return original(target_room)

    monkeypatch.setattr(service, "_auto_resolve_claim_window_locked", wrapped)

    messages = service._auto_advance_bot_seats_locked(room)

    assert messages == []
    assert call_count <= 2
    assert room.pending_timeout is not None
    assert room.pending_timeout.kind == "claim_window"
    assert room.round_state is not None
    assert room.round_state.pending_action is not None
    assert room.round_state.pending_action["responded_seats"] == [1]


@pytest.mark.asyncio
async def test_handle_action_request_resolves_claims_by_priority_after_all_responses(
    monkeypatch,
):
    service = GameService(sessionmaker())
    monkeypatch.setattr(service, "_persist_room_state_locked", lambda _room: None)
    monkeypatch.setattr(service, "_sync_timeout_task_locked", lambda _room: None)

    room = RoomState(table_code="ROOM42", phase="playing")
    websockets = [_RecordingWebSocket() for _ in range(4)]
    for seat_index, websocket in enumerate(websockets):
        room.seats[seat_index] = SeatReservation(
            seat_index=seat_index,
            nickname=f"P{seat_index}",
            reconnect_token=f"token-{seat_index}",
            player_session_id=seat_index + 1,
            websocket=websocket,
            connected=True,
            ready=True,
        )
    room.round_state = _make_claim_priority_round_state()
    service._rooms["ROOM42"] = room

    await service.handle_action_request(
        "ROOM42",
        websockets[1],
        {"action_type": "pung", "tile_ids": ["w5#p1a", "w5#p1b"]},
    )
    await service.handle_action_request(
        "ROOM42",
        websockets[2],
        {"action_type": "hu", "tile_ids": []},
    )

    assert room.round_state is not None
    assert room.round_state.phase == "settlement"
    assert room.round_state.settlement["winner_seat"] == 2
    assert room.round_state.settlement["discarder_seat"] == 0


@pytest.mark.asyncio
async def test_handle_action_request_resolves_immediately_when_higher_priority_claim_blocks_others(
    monkeypatch,
):
    service = GameService(sessionmaker())
    monkeypatch.setattr(service, "_persist_room_state_locked", lambda _room: None)
    monkeypatch.setattr(service, "_sync_timeout_task_locked", lambda _room: None)

    room = RoomState(table_code="ROOM43", phase="playing")
    websockets = [_RecordingWebSocket() for _ in range(4)]
    for seat_index, websocket in enumerate(websockets):
        room.seats[seat_index] = SeatReservation(
            seat_index=seat_index,
            nickname=f"P{seat_index}",
            reconnect_token=f"token-{seat_index}",
            player_session_id=seat_index + 1,
            websocket=websocket,
            connected=True,
            ready=True,
        )
    room.round_state = _make_claim_priority_round_state()
    service._rooms["ROOM43"] = room

    await service.handle_action_request(
        "ROOM43",
        websockets[2],
        {"action_type": "hu", "tile_ids": []},
    )

    assert room.round_state is not None
    assert room.round_state.phase == "settlement"
    assert room.round_state.settlement["winner_seat"] == 2
    assert room.round_state.settlement["discarder_seat"] == 0


@pytest.mark.asyncio
async def test_handle_action_request_broadcasts_self_hu_declared_before_settlement(
    monkeypatch,
):
    service = GameService(sessionmaker())
    monkeypatch.setattr(service, "_persist_room_state_locked", lambda _room: None)
    monkeypatch.setattr(service, "_sync_timeout_task_locked", lambda _room: None)

    room = RoomState(table_code="ROOM44", phase="playing")
    websockets = [_RecordingWebSocket() for _ in range(4)]
    for seat_index, websocket in enumerate(websockets):
        room.seats[seat_index] = SeatReservation(
            seat_index=seat_index,
            nickname=f"P{seat_index}",
            reconnect_token=f"token-{seat_index}",
            player_session_id=seat_index + 1,
            websocket=websocket,
            connected=True,
            ready=True,
        )

    room.round_state = RoundState(
        round_id="round-self-hu",
        dealer_seat=0,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(
            PlayerState(
                seat=0,
                concealed_tiles=(
                    _make_suit_tile("w1", "w1#0"),
                    _make_suit_tile("w1", "w1#1"),
                    _make_suit_tile("w2", "w2#0"),
                    _make_suit_tile("w2", "w2#1"),
                    _make_suit_tile("w3", "w3#0"),
                    _make_suit_tile("w3", "w3#1"),
                    _make_suit_tile("w4", "w4#0"),
                    _make_suit_tile("w4", "w4#1"),
                    _make_suit_tile("w5", "w5#0"),
                    _make_suit_tile("w5", "w5#1"),
                    _make_suit_tile("w6", "w6#0"),
                    _make_suit_tile("w6", "w6#1"),
                    _make_suit_tile("w7", "w7#0"),
                    _make_suit_tile("w7", "w7#1"),
                ),
                melds=(),
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
        score_trackers={"kong_entries": []},
        last_action_context={
            "kind": "draw",
            "seat": 0,
            "tile_id": "w7#1",
            "from_kong_replacement": False,
            "was_last_live_tile": False,
            "was_last_discard": False,
        },
        round_wind="east",
    )
    room.pending_timeout = PendingTimeout(
        kind="active_turn",
        seat_index=0,
        deadline_at=__import__("datetime").datetime.now(
            __import__("datetime").timezone.utc
        ),
        drawn_tile_id="w7#1",
    )
    service._rooms["ROOM44"] = room

    await service.handle_action_request(
        "ROOM44",
        websockets[0],
        {"action_type": "hu", "tile_ids": []},
    )

    assert room.round_state is not None
    assert room.round_state.phase == "settlement"
    assert room.round_state.settlement["win_type"] == "self_draw"
    assert any(
        message["type"] == "round_event"
        and message["payload"]["event_type"] == "self_hu_declared"
        and message["payload"]["event"]["seat"] == 0
        for message in websockets[0].messages
    )
    assert any(
        message["type"] == "round_event"
        and message["payload"]["event_type"] == "settlement_ready"
        for message in websockets[0].messages
    )


@pytest.mark.asyncio
async def test_start_next_round_rotates_dealer_and_keeps_match_scores(monkeypatch):
    service = GameService(sessionmaker())
    monkeypatch.setattr(service, "_persist_room_state_locked", lambda _room: None)
    monkeypatch.setattr(service, "_sync_timeout_task_locked", lambda _room: None)

    websocket = _RecordingWebSocket()
    room = RoomState(table_code="ROOM99", phase="settlement")
    room.seats[0] = SeatReservation(
        seat_index=0,
        nickname="P0",
        reconnect_token="token-0",
        player_session_id=1,
        websocket=websocket,
        connected=True,
        ready=True,
    )
    room.seats[1] = SeatReservation(
        seat_index=1,
        nickname="P1",
        reconnect_token="token-1",
        player_session_id=2,
        websocket=_RecordingWebSocket(),
        connected=True,
        ready=True,
    )
    room.seats[2] = SeatReservation(
        seat_index=2,
        nickname="P2",
        reconnect_token="token-2",
        player_session_id=3,
        websocket=_RecordingWebSocket(),
        connected=True,
        ready=True,
    )
    room.seats[3] = SeatReservation(
        seat_index=3,
        nickname="P3",
        reconnect_token="token-3",
        player_session_id=4,
        websocket=_RecordingWebSocket(),
        connected=True,
        ready=True,
    )
    room.match_state = MatchState(
        prevailing_wind="east",
        hand_number=1,
        dealer_seat=0,
        cumulative_scores={0: 0, 1: 8, 2: 0, 3: -8},
    )
    room.round_state = RoundState(
        round_id="round-finished",
        dealer_seat=0,
        current_actor=2,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(4)
        ),
        last_discard=None,
        pending_action=None,
        phase="settlement",
        settlement={
            "win_type": "discard",
            "winner_seat": 2,
            "discarder_seat": 3,
            "fan_total": 8,
            "fan_keys": ["test-eight-fan"],
            "fan_breakdown": [],
            "flower_count": 0,
            "kong_score_detail": [],
            "score_delta": {
                "provisional": True,
                "fan_total": 8,
                "fan_delta_by_seat": {0: 0, 1: 0, 2: 8, 3: -8},
                "kong_delta_by_seat": {0: 0, 1: 0, 2: 0, 3: 0},
                "total_delta_by_seat": {0: 0, 1: 0, 2: 8, 3: -8},
            },
        },
        version=0,
        score_trackers={"kong_entries": []},
        last_action_context=None,
    )
    service._rooms["ROOM99"] = room

    response = await service.start_next_round(table_code="ROOM99", websocket=websocket)

    assert response["type"] == "room_snapshot"
    assert response["payload"]["phase"] == "playing"
    assert response["payload"]["match_state"]["dealer_seat"] == 1
    assert response["payload"]["match_state"]["hand_number"] == 2
    assert response["payload"]["private_state"]["dealer_seat"] == 1
    assert response["payload"]["private_state"]["round_wind"] == "east"


@pytest.mark.asyncio
async def test_start_next_round_rotates_dealer_after_dealer_self_draw(monkeypatch):
    service = GameService(sessionmaker())
    monkeypatch.setattr(service, "_persist_room_state_locked", lambda _room: None)
    monkeypatch.setattr(service, "_sync_timeout_task_locked", lambda _room: None)

    room, websocket = _make_settlement_room(
        table_code="ROOM100",
        match_state=MatchState(
            prevailing_wind="east",
            hand_number=1,
            dealer_seat=0,
            cumulative_scores={0: 12, 1: -4, 2: -4, 3: -4},
        ),
        settlement={
            "win_type": "self_draw",
            "winner_seat": 0,
            "fan_total": 8,
            "fan_keys": ["test-eight-fan"],
            "fan_breakdown": [],
            "flower_count": 0,
            "kong_score_detail": [],
            "score_delta": {
                "provisional": True,
                "fan_total": 8,
                "fan_delta_by_seat": {0: 48, 1: -16, 2: -16, 3: -16},
                "kong_delta_by_seat": {0: 0, 1: 0, 2: 0, 3: 0},
                "total_delta_by_seat": {0: 48, 1: -16, 2: -16, 3: -16},
            },
        },
    )
    service._rooms["ROOM100"] = room

    response = await service.start_next_round(table_code="ROOM100", websocket=websocket)

    assert response["payload"]["match_state"]["dealer_seat"] == 1
    assert response["payload"]["match_state"]["hand_number"] == 2
    assert response["payload"]["match_state"]["cumulative_scores"] == {
        0: 60,
        1: -20,
        2: -20,
        3: -20,
    }
    assert response["payload"]["private_state"]["round_wind"] == "east"


@pytest.mark.asyncio
async def test_start_next_round_advances_round_wind_after_fourth_hand_draw(monkeypatch):
    service = GameService(sessionmaker())
    monkeypatch.setattr(service, "_persist_room_state_locked", lambda _room: None)
    monkeypatch.setattr(service, "_sync_timeout_task_locked", lambda _room: None)

    room, websocket = _make_settlement_room(
        table_code="ROOM101",
        match_state=MatchState(
            prevailing_wind="east",
            hand_number=4,
            dealer_seat=3,
            cumulative_scores={0: 5, 1: -1, 2: -2, 3: -2},
        ),
        settlement={
            "win_type": "draw",
            "fan_total": 0,
            "fan_keys": [],
            "fan_breakdown": [],
            "flower_count": 0,
            "kong_score_detail": [],
            "score_delta": {
                "provisional": True,
                "fan_total": 0,
                "fan_delta_by_seat": {0: 0, 1: 0, 2: 0, 3: 0},
                "kong_delta_by_seat": {0: 0, 1: 0, 2: 0, 3: 0},
                "total_delta_by_seat": {0: 0, 1: 0, 2: 0, 3: 0},
            },
        },
    )
    service._rooms["ROOM101"] = room

    response = await service.start_next_round(table_code="ROOM101", websocket=websocket)

    assert response["payload"]["match_state"]["prevailing_wind"] == "south"
    assert response["payload"]["match_state"]["hand_number"] == 1
    assert response["payload"]["match_state"]["dealer_seat"] == 0
    assert response["payload"]["private_state"]["round_wind"] == "south"
    assert response["payload"]["private_state"]["dealer_seat"] == 0


@pytest.mark.asyncio
async def test_start_next_round_finishes_match_after_north_four(monkeypatch):
    service = GameService(sessionmaker())
    monkeypatch.setattr(service, "_persist_room_state_locked", lambda _room: None)
    monkeypatch.setattr(service, "_sync_timeout_task_locked", lambda _room: None)

    room, websocket = _make_settlement_room(
        table_code="ROOM102",
        match_state=MatchState(
            prevailing_wind="north",
            hand_number=4,
            dealer_seat=3,
            cumulative_scores={0: 10, 1: -3, 2: -3, 3: -4},
        ),
        settlement={
            "win_type": "discard",
            "winner_seat": 1,
            "discarder_seat": 2,
            "fan_total": 8,
            "fan_keys": ["test-eight-fan"],
            "fan_breakdown": [],
            "flower_count": 0,
            "kong_score_detail": [],
            "score_delta": {
                "provisional": True,
                "fan_total": 8,
                "fan_delta_by_seat": {0: 0, 1: 8, 2: -8, 3: 0},
                "kong_delta_by_seat": {0: 0, 1: 0, 2: 0, 3: 0},
                "total_delta_by_seat": {0: 0, 1: 8, 2: -8, 3: 0},
            },
        },
    )
    service._rooms["ROOM102"] = room

    response = await service.start_next_round(table_code="ROOM102", websocket=websocket)

    assert response["payload"]["phase"] == "finished"
    assert response["payload"]["match_state"]["match_finished"] is True
    assert response["payload"]["match_state"]["prevailing_wind"] == "north"
    assert response["payload"]["match_state"]["hand_number"] == 4
    assert response["payload"]["match_state"]["dealer_seat"] == 3
    assert response["payload"]["match_state"]["cumulative_scores"] == {
        0: 10,
        1: 5,
        2: -11,
        3: -4,
    }


@pytest.mark.asyncio
async def test_start_match_uses_random_initial_dealer(monkeypatch):
    service = GameService(sessionmaker())
    monkeypatch.setattr(service, "_persist_room_state_locked", lambda _room: None)
    monkeypatch.setattr(service, "_sync_timeout_task_locked", lambda _room: None)
    monkeypatch.setattr(game_service_module.random, "choice", lambda seats: 2)

    websocket = _RecordingWebSocket()
    room = RoomState(table_code="ROOM70", phase="waiting")
    for seat in range(4):
        room.seats[seat] = SeatReservation(
            seat_index=seat,
            nickname=f"P{seat}",
            reconnect_token=f"token-{seat}",
            player_session_id=seat + 1,
            websocket=websocket if seat == 0 else _RecordingWebSocket(),
            connected=True,
            ready=True,
        )
    service._rooms["ROOM70"] = room

    response = await service.start_match(table_code="ROOM70", websocket=websocket)

    assert response == {"type": "start_match_accepted", "payload": {}}
    assert room.phase == "playing"
    assert room.match_state is not None
    assert room.match_state.dealer_seat == 2
    assert room.round_state is not None
    assert room.round_state.dealer_seat == 2
    assert room.round_state.current_actor == 2


@pytest.mark.asyncio
async def test_restart_match_resets_match_state_after_finish(monkeypatch):
    service = GameService(sessionmaker())
    monkeypatch.setattr(service, "_persist_room_state_locked", lambda _room: None)
    monkeypatch.setattr(service, "_sync_timeout_task_locked", lambda _room: None)
    monkeypatch.setattr(game_service_module.random, "choice", lambda seats: 2)

    websocket = _RecordingWebSocket()
    room = RoomState(table_code="ROOM77", phase="finished")
    for seat in range(4):
        room.seats[seat] = SeatReservation(
            seat_index=seat,
            nickname=f"P{seat}",
            reconnect_token=f"token-{seat}",
            player_session_id=seat + 1,
            websocket=websocket if seat == 0 else _RecordingWebSocket(),
            connected=True,
            ready=True,
        )
    room.match_state = MatchState(
        prevailing_wind="north",
        hand_number=4,
        dealer_seat=3,
        cumulative_scores={0: 32, 1: -8, 2: -8, 3: -16},
        match_finished=True,
        last_completed_round_id="finished-round",
    )
    room.round_state = RoundState(
        round_id="finished-round",
        dealer_seat=3,
        current_actor=3,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(4)
        ),
        last_discard=None,
        pending_action=None,
        phase="settlement",
        settlement={"win_type": "draw"},
        version=0,
        score_trackers={"kong_entries": []},
        last_action_context=None,
        round_wind="north",
    )
    service._rooms["ROOM77"] = room

    response = await service.restart_match(table_code="ROOM77", websocket=websocket)

    assert response == {"type": "restart_match_accepted", "payload": {}}
    assert room.phase == "playing"
    assert room.match_state is not None
    assert room.match_state.prevailing_wind == "east"
    assert room.match_state.hand_number == 1
    assert room.match_state.dealer_seat == 2
    assert room.match_state.cumulative_scores == {0: 0, 1: 0, 2: 0, 3: 0}
    assert room.match_state.match_finished is False
    assert room.round_state is not None
    assert room.round_state.round_wind == "east"
    assert room.round_state.dealer_seat == 2


def test_room_snapshot_exposes_in_round_score_state() -> None:
    service = GameService(sessionmaker())
    room = RoomState(table_code="ROOM55", phase="playing")
    for seat in range(4):
        room.seats[seat] = SeatReservation(
            seat_index=seat,
            nickname=f"P{seat}",
            reconnect_token=f"token-{seat}",
            player_session_id=seat + 1,
            websocket=_RecordingWebSocket(),
            connected=True,
            ready=True,
        )
    room.match_state = MatchState(
        prevailing_wind="east",
        hand_number=2,
        dealer_seat=1,
        cumulative_scores={0: 10, 1: -10, 2: 0, 3: 0},
    )
    room.round_state = RoundState(
        round_id="round-score",
        dealer_seat=1,
        current_actor=0,
        wall=WallState(tiles=(), head_index=0, tail_index=-1),
        players=(
            PlayerState(seat=0, concealed_tiles=(), melds=(), flowers=(_make_honor_tile("f1", "f1#0", "flower"),), discards=()),
            PlayerState(seat=1, concealed_tiles=(), melds=(), flowers=(), discards=()),
            PlayerState(seat=2, concealed_tiles=(), melds=(), flowers=(), discards=()),
            PlayerState(seat=3, concealed_tiles=(), melds=(), flowers=(), discards=()),
        ),
        last_discard=None,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
        score_trackers={
            "kong_entries": [
                {"kong_type": "concealed_kong", "actor_seat": 0, "payer_seats": [1, 2, 3]},
                {"kong_type": "exposed_kong", "actor_seat": 1, "payer_seats": [0]},
            ],
            "opening_flowers_completed": True,
        },
        last_action_context=None,
        round_wind="east",
    )

    snapshot = service._room_snapshot(room=room, local_seat=0)
    score_state = snapshot["payload"]["private_state"]["score_state"]

    assert snapshot["payload"]["private_state"]["wall_tiles_remaining"] == 0
    assert score_state["flower_count_by_seat"] == {0: 1, 1: 0, 2: 0, 3: 0}
    assert score_state["kong_delta_by_seat"] == {0: 2, 1: 0, 2: -1, 3: -1}
    assert score_state["current_round_delta_by_seat"] == {0: 2, 1: 0, 2: -1, 3: -1}
    assert score_state["base_cumulative_scores"] == {0: 10, 1: -10, 2: 0, 3: 0}
    assert score_state["projected_cumulative_scores"] == {0: 12, 1: -10, 2: -1, 3: -1}
    assert len(score_state["kong_score_detail"]) == 2


def test_room_snapshot_exposes_wall_tiles_remaining_count() -> None:
    service = GameService(sessionmaker())
    room = RoomState(table_code="ROOM56", phase="playing")
    for seat in range(4):
        room.seats[seat] = SeatReservation(
            seat_index=seat,
            nickname=f"P{seat}",
            reconnect_token=f"token-{seat}",
            player_session_id=seat + 1,
            websocket=_RecordingWebSocket(),
            connected=True,
            ready=True,
        )

    wall_tiles = (
        _make_suit_tile("w1", "w1#0"),
        _make_suit_tile("w2", "w2#0"),
        _make_suit_tile("w3", "w3#0"),
        _make_suit_tile("w4", "w4#0"),
        _make_suit_tile("w5", "w5#0"),
    )
    room.round_state = RoundState(
        round_id="round-wall-count",
        dealer_seat=0,
        current_actor=1,
        wall=WallState(tiles=wall_tiles, head_index=1, tail_index=3),
        players=tuple(
            PlayerState(seat=seat, concealed_tiles=(), melds=(), flowers=(), discards=())
            for seat in range(4)
        ),
        last_discard=None,
        pending_action=None,
        phase="playing",
        settlement=None,
        version=0,
        score_trackers={"kong_entries": []},
        last_action_context=None,
        round_wind="east",
    )

    snapshot = service._room_snapshot(room=room, local_seat=0)

    assert snapshot["payload"]["private_state"]["wall_tiles_remaining"] == 3


def test_settlement_snapshot_reveals_all_players_concealed_tiles() -> None:
    service = GameService(sessionmaker())
    players = tuple(
        PlayerState(
            seat=seat,
            concealed_tiles=(
                _make_suit_tile(f"w{seat + 1}", f"w{seat + 1}#a"),
                _make_suit_tile(f"b{seat + 1}", f"b{seat + 1}#b"),
            ),
            melds=(),
            flowers=(),
            discards=(),
        )
        for seat in range(4)
    )
    room, _websocket = _make_settlement_room(
        table_code="ROOM-HANDS",
        match_state=MatchState(
            prevailing_wind="east",
            hand_number=1,
            dealer_seat=0,
            cumulative_scores={0: 0, 1: 0, 2: 0, 3: 0},
        ),
        settlement={
            "win_type": "draw",
            "fan_total": 0,
            "fan_keys": [],
            "fan_breakdown": [],
            "flower_count": 0,
            "kong_score_detail": [],
            "score_delta": {
                "provisional": True,
                "fan_total": 0,
                "fan_delta_by_seat": {0: 0, 1: 0, 2: 0, 3: 0},
                "kong_delta_by_seat": {0: 0, 1: 0, 2: 0, 3: 0},
                "total_delta_by_seat": {0: 0, 1: 0, 2: 0, 3: 0},
            },
        },
        players=players,
    )

    snapshot = service._room_snapshot(room=room, local_seat=0)
    private_players = snapshot["payload"]["private_state"]["players"]

    assert [player["concealed_tiles"] for player in private_players] == [
        [
            {"tile_id": "w1#a", "tile_key": "w1"},
            {"tile_id": "b1#b", "tile_key": "b1"},
        ],
        [
            {"tile_id": "w2#a", "tile_key": "w2"},
            {"tile_id": "b2#b", "tile_key": "b2"},
        ],
        [
            {"tile_id": "w3#a", "tile_key": "w3"},
            {"tile_id": "b3#b", "tile_key": "b3"},
        ],
        [
            {"tile_id": "w4#a", "tile_key": "w4"},
            {"tile_id": "b4#b", "tile_key": "b4"},
        ],
    ]
