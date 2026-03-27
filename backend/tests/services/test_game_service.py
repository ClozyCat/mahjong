import pytest
from sqlalchemy.orm import sessionmaker
from starlette.websockets import WebSocketDisconnect

from app.domain.models import PlayerState, RoundState, Tile
from app.domain.reducer import initialize_round
from app.domain.wall import WallState
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
        }
    ]


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
async def test_restart_match_resets_match_state_after_finish(monkeypatch):
    service = GameService(sessionmaker())
    monkeypatch.setattr(service, "_persist_room_state_locked", lambda _room: None)
    monkeypatch.setattr(service, "_sync_timeout_task_locked", lambda _room: None)

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
    assert room.match_state.dealer_seat == 0
    assert room.match_state.cumulative_scores == {0: 0, 1: 0, 2: 0, 3: 0}
    assert room.match_state.match_finished is False
    assert room.round_state is not None
    assert room.round_state.round_wind == "east"


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

    assert score_state["flower_count_by_seat"] == {0: 1, 1: 0, 2: 0, 3: 0}
    assert score_state["kong_delta_by_seat"] == {0: 2, 1: 0, 2: -1, 3: -1}
    assert score_state["current_round_delta_by_seat"] == {0: 2, 1: 0, 2: -1, 3: -1}
    assert score_state["base_cumulative_scores"] == {0: 10, 1: -10, 2: 0, 3: 0}
    assert score_state["projected_cumulative_scores"] == {0: 12, 1: -10, 2: -1, 3: -1}
    assert len(score_state["kong_score_detail"]) == 2
