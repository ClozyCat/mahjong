from contextlib import ExitStack
from dataclasses import replace
import time

from fastapi.testclient import TestClient
from sqlalchemy import select

from app.db.models import PlayerSessionRecord, TableSeatRecord
from app.domain.reducer import initialize_round
from app.services import game_service as game_service_module
from app.services import timeout_service


def _join_player(ws, nickname: str) -> dict:
    ws.send_json({"type": "join_table", "payload": {"nickname": nickname}})
    return ws.receive_json()


def _receive_until_snapshot(ws) -> tuple[dict, list[dict]]:
    extras: list[dict] = []
    while True:
        message = ws.receive_json()
        if message["type"] == "room_snapshot":
            return message, extras
        extras.append(message)


def _first_flower_tile_id(snapshot: dict) -> str:
    payload = snapshot["payload"]
    local_seat = payload["local_seat"]
    player = payload["private_state"]["players"][local_seat]
    for tile in player["concealed_tiles"] or []:
        if tile["tile_key"].startswith("f"):
            return tile["tile_id"]
    raise AssertionError("expected a concealed flower tile")


def _advance_opening_flowers_single_socket(ws, snapshot: dict, prompt: dict) -> tuple[dict, dict]:
    while snapshot["payload"]["private_state"]["pending_action"]["type"] == "opening_flowers":
        if prompt["payload"]["options"] == ["flower"]:
            ws.send_json(
                {
                    "type": "action_request",
                    "payload": {
                        "action_type": "flower",
                        "tile_ids": [_first_flower_tile_id(snapshot)],
                    },
                }
            )
        else:
            ws.send_json(
                {
                    "type": "action_request",
                    "payload": {
                        "action_type": "pass",
                        "tile_ids": [],
                    },
                }
            )
        snapshot, _ = _receive_until_snapshot(ws)
        prompt = ws.receive_json()
        assert prompt["type"] == "action_prompt"
    return snapshot, prompt


def _ready_all_and_start(*sockets) -> tuple[dict, dict]:
    for ready_index, ws in enumerate(sockets):
        ws.send_json({"type": "ready", "payload": {"ready": True}})
        ready_snapshot = ws.receive_json()
        assert ready_snapshot["type"] == "room_snapshot"
        assert ready_snapshot["payload"]["seats"][ready_index]["ready"] is True
        for peer in sockets:
            if peer is ws:
                continue
            peer_snapshot = peer.receive_json()
            assert peer_snapshot["type"] == "room_snapshot"
            assert peer_snapshot["payload"]["seats"][ready_index]["ready"] is True

    sockets[0].send_json({"type": "start_match", "payload": {}})
    snapshots: list[dict] = []
    for ws in sockets:
        snapshot, _ = _receive_until_snapshot(ws)
        assert snapshot["type"] == "room_snapshot"
        assert snapshot["payload"]["phase"] == "playing"
        snapshots.append(snapshot)

    while True:
        active_snapshot = next(
            snapshot
            for snapshot in snapshots
            if snapshot["payload"]["private_state"]["pending_action"] is not None
        )
        pending_action = active_snapshot["payload"]["private_state"]["pending_action"]
        seat_index = pending_action["seat_index"]
        prompt = sockets[seat_index].receive_json()
        assert prompt["type"] == "action_prompt"
        if pending_action["type"] == "active_turn":
            return active_snapshot, prompt

        if pending_action["options"] == ["flower"]:
            sockets[seat_index].send_json(
                {
                    "type": "action_request",
                    "payload": {
                        "action_type": "flower",
                        "tile_ids": [_first_flower_tile_id(snapshots[seat_index])],
                    },
                }
            )
        else:
            sockets[seat_index].send_json(
                {
                    "type": "action_request",
                    "payload": {
                        "action_type": "pass",
                        "tile_ids": [],
                    },
                }
            )

        snapshots = []
        for ws in sockets:
            snapshot, _ = _receive_until_snapshot(ws)
            snapshots.append(snapshot)




def test_voluntary_leave_during_match_invalidates_reconnect_token(test_app) -> None:
    with TestClient(test_app) as client:
        table_code = client.post("/api/tables").json()["table_code"]

        try:
            with ExitStack() as stack:
                sockets = [
                    stack.enter_context(client.websocket_connect(f"/ws/{table_code}"))
                    for _ in range(4)
                ]

                join_snapshots = []
                for index, ws in enumerate(sockets):
                    join_snapshots.append(_join_player(ws, f"P{index}"))
                    for peer in sockets[:index]:
                        assert peer.receive_json()["type"] == "player_presence"
                        assert peer.receive_json()["type"] == "room_snapshot"

                reconnect_token = join_snapshots[0]["payload"]["reconnect_token"]
                _ready_all_and_start(*sockets)

                sockets[0].send_json({"type": "leave_table", "payload": {}})
                assert sockets[0].receive_json() == {
                    "type": "leave_table_accepted",
                    "payload": {
                        "table_code": table_code,
                        "seat_index": 0,
                    },
                }

                reconnect_socket = stack.enter_context(client.websocket_connect(f"/ws/{table_code}"))
                reconnect_socket.send_json(
                    {"type": "reconnect", "payload": {"reconnect_token": reconnect_token}}
                )

                assert reconnect_socket.receive_json() == {
                    "type": "action_rejected",
                    "payload": {"reason": "invalid_reconnect_token"},
                }
        finally:
            room = test_app.state.game_service._rooms.pop(table_code, None)
            if room is not None and room.timeout_task is not None:
                room.timeout_task.cancel()


def test_four_players_join_same_table_and_receive_room_snapshot(test_app) -> None:
    client = TestClient(test_app)
    create_response = client.post("/api/tables")
    table_code = create_response.json()["table_code"]

    with client.websocket_connect(f"/ws/{table_code}") as ws_0:
        ws_0.send_json({"type": "join_table", "payload": {"nickname": "P0"}})
        first_join = ws_0.receive_json()

        with client.websocket_connect(f"/ws/{table_code}") as ws_1:
            ws_1.send_json({"type": "join_table", "payload": {"nickname": "P1"}})
            second_join = ws_1.receive_json()
            peer_presence = ws_0.receive_json()
            peer_snapshot = ws_0.receive_json()

            with client.websocket_connect(f"/ws/{table_code}") as ws_2:
                ws_2.send_json({"type": "join_table", "payload": {"nickname": "P2"}})
                third_join = ws_2.receive_json()

                with client.websocket_connect(f"/ws/{table_code}") as ws_3:
                    ws_3.send_json({"type": "join_table", "payload": {"nickname": "P3"}})
                    fourth_join = ws_3.receive_json()

    assert first_join["type"] == "room_snapshot"
    assert second_join["type"] == "room_snapshot"
    assert third_join["type"] == "room_snapshot"
    assert fourth_join["type"] == "room_snapshot"
    assert fourth_join["payload"]["phase"] == "waiting"
    assert fourth_join["payload"]["private_state"] is None
    assert peer_presence == {
        "type": "player_presence",
        "payload": {"table_code": table_code, "seat_index": 1, "connected": True},
    }
    assert peer_snapshot["type"] == "room_snapshot"

    room_snapshot = peer_snapshot["payload"]
    assert room_snapshot["table_code"] == table_code
    assert room_snapshot["phase"] == "waiting"
    assert len(room_snapshot["seats"]) == 2
    assert room_snapshot["local_seat"] == 0
    assert room_snapshot["reconnect_token"]
    assert all(seat["ready"] is False for seat in fourth_join["payload"]["seats"])


def test_disconnect_and_reconnect_during_active_round_after_server_timeout(
    monkeypatch,
    test_app,
) -> None:
    monkeypatch.setattr(timeout_service, "ACTIVE_TURN_TIMEOUT_SECONDS", 0.25)
    monkeypatch.setattr(game_service_module, "ACTIVE_TURN_TIMEOUT_SECONDS", 0.25)

    client = TestClient(test_app)
    table_code = client.post("/api/tables").json()["table_code"]

    with ExitStack() as ws_0_stack, ExitStack() as stack:
        ws_0 = ws_0_stack.enter_context(client.websocket_connect(f"/ws/{table_code}"))
        ws_1 = stack.enter_context(client.websocket_connect(f"/ws/{table_code}"))
        ws_2 = stack.enter_context(client.websocket_connect(f"/ws/{table_code}"))
        ws_3 = stack.enter_context(client.websocket_connect(f"/ws/{table_code}"))

        first_join = _join_player(ws_0, "P0")
        original_token = first_join["payload"]["reconnect_token"]

        _join_player(ws_1, "P1")
        assert ws_0.receive_json()["type"] == "player_presence"
        assert ws_0.receive_json()["type"] == "room_snapshot"

        _join_player(ws_2, "P2")
        for peer in (ws_0, ws_1):
            assert peer.receive_json()["type"] == "player_presence"
            assert peer.receive_json()["type"] == "room_snapshot"

        fourth_join = _join_player(ws_3, "P3")
        assert fourth_join["payload"]["phase"] == "waiting"
        for peer in (ws_1, ws_2):
            assert peer.receive_json()["type"] == "player_presence"
            assert peer.receive_json()["type"] == "room_snapshot"
        assert ws_0.receive_json()["type"] == "player_presence"
        assert ws_0.receive_json()["type"] == "room_snapshot"

        active_snapshot, action_prompt = _ready_all_and_start(ws_0, ws_1, ws_2, ws_3)

        assert active_snapshot["type"] == "room_snapshot"
        assert active_snapshot["payload"]["phase"] == "playing"
        assert active_snapshot["payload"]["private_state"] is not None
        assert action_prompt["type"] == "action_prompt"
        assert action_prompt["payload"]["seat_index"] == active_snapshot["payload"]["local_seat"]

        ws_0_stack.close()
        time.sleep(0.35)
        ws_1.send_json(
            {
                "type": "heartbeat",
                "payload": {"sent_at": "2026-03-25T00:00:00+00:00"},
            }
        )
        follow_up_messages: list[dict] = []
        for _ in range(12):
            follow_up_messages.append(ws_1.receive_json())
            has_disconnect_presence = any(
                message["type"] == "player_presence"
                and message["payload"]
                == {
                    "table_code": table_code,
                    "seat_index": 0,
                    "connected": False,
                }
                for message in follow_up_messages
            )
            has_disconnect_snapshot = any(
                message["type"] == "room_snapshot"
                and message["payload"]["seats"][0]["connected"] is False
                for message in follow_up_messages
            )
            has_timeout_event = any(
                message["type"] == "round_event" for message in follow_up_messages
            )
            has_timeout_snapshot = any(
                message["type"] == "room_snapshot"
                and message["payload"]["phase"] == "playing"
                for message in follow_up_messages
            )
            has_heartbeat_echo = any(
                message["type"] == "heartbeat" for message in follow_up_messages
            )
            if (
                has_disconnect_presence
                and has_disconnect_snapshot
                and has_timeout_event
                and has_timeout_snapshot
                and has_heartbeat_echo
            ):
                break
        disconnect_presence = next(
            message
            for message in follow_up_messages
            if message["type"] == "player_presence"
            and message["payload"]
            == {
                "table_code": table_code,
                "seat_index": 0,
                "connected": False,
            }
        )
        disconnect_snapshot = next(
            message
            for message in follow_up_messages
            if message["type"] == "room_snapshot"
            and message["payload"]["seats"][0]["connected"] is False
        )
        timeout_event = next(
            message
            for message in follow_up_messages
            if message["type"] == "round_event"
        )
        timeout_snapshot = next(
            message
            for message in follow_up_messages
            if message["type"] == "room_snapshot"
            and message["payload"]["phase"] == "playing"
        )
        next_prompt = next(
            (
                message
                for message in follow_up_messages
                if message["type"] == "action_prompt"
            ),
            None,
        )
        heartbeat_echo = next(
            message
            for message in follow_up_messages
            if message["type"] == "heartbeat"
        )

        ws_reconnect = stack.enter_context(client.websocket_connect(f"/ws/{table_code}"))
        ws_reconnect.send_json(
            {"type": "reconnect", "payload": {"reconnect_token": original_token}}
        )
        reconnect_snapshot = ws_reconnect.receive_json()

    assert disconnect_presence == {
        "type": "player_presence",
        "payload": {"table_code": table_code, "seat_index": 0, "connected": False},
    }
    assert disconnect_snapshot["type"] == "room_snapshot"
    assert timeout_event["type"] == "round_event"
    assert timeout_event["payload"]["event_type"] == "tile_discarded"
    assert timeout_snapshot["type"] == "room_snapshot"
    if next_prompt is not None:
        assert next_prompt["type"] == "action_prompt"
    assert heartbeat_echo == {
        "type": "heartbeat",
        "payload": {"sent_at": "2026-03-25T00:00:00+00:00"},
    }
    assert reconnect_snapshot["type"] == "room_snapshot"
    assert reconnect_snapshot["payload"]["phase"] == "playing"
    assert reconnect_snapshot["payload"]["private_state"] is not None
    assert reconnect_snapshot["payload"]["seats"][0]["connected"] is True
    assert reconnect_snapshot["payload"]["private_state"]["players"][0]["connected"] is True
    assert reconnect_snapshot["payload"]["private_state"]["players"][0]["concealed_tiles"]


def test_active_player_can_discard_and_broadcast_updated_snapshot(test_app) -> None:
    client = TestClient(test_app)
    table_code = client.post("/api/tables").json()["table_code"]

    with ExitStack() as stack:
        ws_0 = stack.enter_context(client.websocket_connect(f"/ws/{table_code}"))
        ws_1 = stack.enter_context(client.websocket_connect(f"/ws/{table_code}"))
        ws_2 = stack.enter_context(client.websocket_connect(f"/ws/{table_code}"))
        ws_3 = stack.enter_context(client.websocket_connect(f"/ws/{table_code}"))

        _join_player(ws_0, "P0")
        _join_player(ws_1, "P1")
        assert ws_0.receive_json()["type"] == "player_presence"
        assert ws_0.receive_json()["type"] == "room_snapshot"

        _join_player(ws_2, "P2")
        for peer in (ws_0, ws_1):
            assert peer.receive_json()["type"] == "player_presence"
            assert peer.receive_json()["type"] == "room_snapshot"

        fourth_join = _join_player(ws_3, "P3")
        assert fourth_join["payload"]["phase"] == "waiting"
        for peer in (ws_1, ws_2):
            assert peer.receive_json()["type"] == "player_presence"
            assert peer.receive_json()["type"] == "room_snapshot"
        assert ws_0.receive_json()["type"] == "player_presence"
        assert ws_0.receive_json()["type"] == "room_snapshot"
        active_snapshot, action_prompt = _ready_all_and_start(ws_0, ws_1, ws_2, ws_3)

        drawn_tile_id = active_snapshot["payload"]["private_state"]["pending_action"][
            "drawn_tile_id"
        ]
        active_seat = active_snapshot["payload"]["local_seat"]
        sockets = (ws_0, ws_1, ws_2, ws_3)
        active_socket = sockets[active_seat]
        peer_socket = next(ws for index, ws in enumerate(sockets) if index != active_seat)
        if active_snapshot["payload"]["private_state"] is not None:
            assert (
                active_snapshot["payload"]["private_state"]["current_actor"]
                == active_snapshot["payload"]["private_state"]["dealer_seat"]
            )

        active_socket.send_json(
            {
                "type": "action_request",
                "payload": {
                    "action_type": "discard",
                    "tile_ids": [drawn_tile_id],
                },
            }
        )

        discard_event = active_socket.receive_json()
        next_snapshot = active_socket.receive_json()
        peer_message = peer_socket.receive_json()

    assert discard_event["type"] == "round_event"
    assert discard_event["payload"]["event_type"] == "tile_discarded"
    assert next_snapshot["type"] == "room_snapshot"
    assert next_snapshot["payload"]["private_state"]["last_discard"] is not None
    assert next_snapshot["payload"]["private_state"]["players"][0]["concealed_count"] == 13
    assert len(next_snapshot["payload"]["private_state"]["players"][0]["discards"]) == 1
    assert peer_message["type"] in {"round_event", "room_snapshot", "action_prompt"}


def test_test_mode_single_player_join_starts_round_immediately(test_mode_app) -> None:
    client = TestClient(test_mode_app)
    table_code = client.post("/api/tables").json()["table_code"]

    with client.websocket_connect(f"/ws/{table_code}") as ws:
        ws.send_json({"type": "join_table", "payload": {"nickname": "Solo"}})
        snapshot = ws.receive_json()

    assert snapshot["type"] == "room_snapshot"
    assert snapshot["payload"]["phase"] == "playing"
    assert snapshot["payload"]["private_state"] is not None
    assert snapshot["payload"]["local_seat"] == 0
    assert len(snapshot["payload"]["seats"]) == 4
    assert snapshot["payload"]["private_state"]["current_actor"] == 0
    assert snapshot["payload"]["private_state"]["pending_action"]["type"] in {
        "opening_flowers",
        "active_turn",
    }


def test_reconnect_during_settlement_receives_match_result(test_app) -> None:
    client = TestClient(test_app)
    table_code = client.post("/api/tables").json()["table_code"]

    with client.websocket_connect(f"/ws/{table_code}") as ws:
        ws.send_json({"type": "join_table", "payload": {"nickname": "P0"}})
        join_snapshot = ws.receive_json()

    reconnect_token = join_snapshot["payload"]["reconnect_token"]
    room = test_app.state.game_service._rooms[table_code]
    settled_round = replace(
        initialize_round(seed=7),
        phase="settlement",
        pending_action=None,
        settlement={
            "winner_seat": 0,
            "win_type": "self_draw",
            "fan_total": 8,
            "fan_keys": ["men_qian_qing", "zi_mo"],
            "fan_breakdown": [
                {"fan_key": "men_qian_qing", "fan_value": 2},
                {"fan_key": "zi_mo", "fan_value": 1},
            ],
            "kong_score_detail": [],
            "score_delta": {
                "provisional": True,
                "basic_points": 8,
                "base_points": 8,
                "fan_total": 8,
                "minimum_qualifying_fan_total": 8,
                "fan_delta_by_seat": {"0": 48, "1": -16, "2": -16, "3": -16},
                "kong_delta_by_seat": {"0": 0, "1": 0, "2": 0, "3": 0},
                "total_delta_by_seat": {"0": 48, "1": -16, "2": -16, "3": -16},
            },
        },
    )
    room.round_state = settled_round
    room.phase = "settlement"
    room.pending_timeout = None

    with client.websocket_connect(f"/ws/{table_code}") as ws_reconnect:
        ws_reconnect.send_json(
            {"type": "reconnect", "payload": {"reconnect_token": reconnect_token}}
        )
        reconnect_snapshot = ws_reconnect.receive_json()
        settlement_message = ws_reconnect.receive_json()

    assert reconnect_snapshot["type"] == "room_snapshot"
    assert reconnect_snapshot["payload"]["phase"] == "settlement"
    assert settlement_message == {
        "type": "match_result",
        "payload": {
            "table_code": table_code,
            "round_id": settled_round.round_id,
            "phase": "settlement",
            "winner_seat": 0,
            "win_type": "self_draw",
            "fan_total": 8,
            "fan_keys": ["men_qian_qing", "zi_mo"],
            "fan_breakdown": [
                {"fan_key": "men_qian_qing", "fan_value": 2},
                {"fan_key": "zi_mo", "fan_value": 1},
            ],
            "kong_score_detail": [],
            "score_delta": {
                "provisional": True,
                "basic_points": 8,
                "base_points": 8,
                "fan_total": 8,
                "minimum_qualifying_fan_total": 8,
                "fan_delta_by_seat": {"0": 48, "1": -16, "2": -16, "3": -16},
                "kong_delta_by_seat": {"0": 0, "1": 0, "2": 0, "3": 0},
                "total_delta_by_seat": {"0": 48, "1": -16, "2": -16, "3": -16},
            },
        },
    }


def test_finished_room_can_restart_match(test_app, monkeypatch) -> None:
    client = TestClient(test_app)
    table_code = client.post("/api/tables").json()["table_code"]
    monkeypatch.setattr(game_service_module.random, "choice", lambda seats: 2)

    with ExitStack() as stack:
        sockets = [
            stack.enter_context(client.websocket_connect(f"/ws/{table_code}"))
            for _ in range(4)
        ]

        for index, ws in enumerate(sockets):
            _join_player(ws, f"P{index}")
            for peer in sockets[:index]:
                assert peer.receive_json()["type"] == "player_presence"
                assert peer.receive_json()["type"] == "room_snapshot"

        room = test_app.state.game_service._rooms[table_code]
        room.phase = "finished"
        room.match_state = game_service_module.MatchState(
            prevailing_wind="north",
            hand_number=4,
            dealer_seat=3,
            cumulative_scores={0: 20, 1: -10, 2: -5, 3: -5},
            match_finished=True,
            last_completed_round_id="done",
        )

        sockets[0].send_json({"type": "restart_match", "payload": {}})
        restart_snapshot = sockets[0].receive_json()

    assert restart_snapshot["type"] == "room_snapshot"
    assert restart_snapshot["payload"]["phase"] == "playing"
    assert restart_snapshot["payload"]["match_state"]["prevailing_wind"] == "east"
    assert restart_snapshot["payload"]["match_state"]["hand_number"] == 1
    assert restart_snapshot["payload"]["match_state"]["dealer_seat"] == 2
    assert restart_snapshot["payload"]["match_state"]["cumulative_scores"] == {
        "0": 0,
        "1": 0,
        "2": 0,
        "3": 0,
    }


def test_test_mode_bots_auto_advance_after_human_discard(test_mode_app) -> None:
    client = TestClient(test_mode_app)
    table_code = client.post("/api/tables").json()["table_code"]

    with client.websocket_connect(f"/ws/{table_code}") as ws:
        ws.send_json({"type": "join_table", "payload": {"nickname": "Solo"}})
        opening_snapshot = ws.receive_json()
        opening_prompt = ws.receive_json()
        if opening_snapshot["payload"]["private_state"]["pending_action"]["type"] == "opening_flowers":
            opening_snapshot, opening_prompt = _advance_opening_flowers_single_socket(
                ws,
                opening_snapshot,
                opening_prompt,
            )

        drawn_tile_id = opening_snapshot["payload"]["private_state"]["pending_action"][
            "drawn_tile_id"
        ]
        ws.send_json(
            {
                "type": "action_request",
                "payload": {
                    "action_type": "discard",
                    "tile_ids": [drawn_tile_id],
                },
            }
        )

        messages = [ws.receive_json() for _ in range(6)]

    assert opening_prompt["type"] == "action_prompt"
    assert messages[0]["type"] == "round_event"
    assert messages[0]["payload"]["event_type"] == "tile_discarded"
    assert any(
        message["type"] == "action_prompt"
        and message["payload"]["seat_index"] == 0
        for message in messages
    )
    latest_snapshot = next(
        message for message in reversed(messages) if message["type"] == "room_snapshot"
    )
    assert latest_snapshot["payload"]["private_state"]["current_actor"] == 0


def test_reconnect_token_for_wrong_table_is_rejected(test_app) -> None:
    client = TestClient(test_app)
    table_code = client.post("/api/tables").json()["table_code"]
    other_table_code = client.post("/api/tables").json()["table_code"]

    with client.websocket_connect(f"/ws/{table_code}") as ws:
        ws.send_json({"type": "join_table", "payload": {"nickname": "P0"}})
        join_snapshot = ws.receive_json()

    reconnect_token = join_snapshot["payload"]["reconnect_token"]

    with client.websocket_connect(f"/ws/{other_table_code}") as ws:
        ws.send_json(
            {"type": "reconnect", "payload": {"reconnect_token": reconnect_token}}
        )
        response = ws.receive_json()

    assert response == {
        "type": "action_rejected",
        "payload": {"reason": "table_not_found"},
    }


def test_join_table_for_unknown_table_is_rejected(test_app) -> None:
    client = TestClient(test_app)

    with client.websocket_connect("/ws/NOPE42") as ws:
        ws.send_json({"type": "join_table", "payload": {"nickname": "P0"}})
        response = ws.receive_json()

    assert response == {
        "type": "action_rejected",
        "payload": {"reason": "table_not_found"},
    }


def test_disconnect_keeps_seat_reserved_and_reconnect_marks_player_connected(
    test_app,
) -> None:
    client = TestClient(test_app)
    table_code = client.post("/api/tables").json()["table_code"]

    with client.websocket_connect(f"/ws/{table_code}") as ws_0:
        ws_0.send_json({"type": "join_table", "payload": {"nickname": "P0"}})
        first_snapshot = ws_0.receive_json()

    reconnect_token = first_snapshot["payload"]["reconnect_token"]

    with client.websocket_connect(f"/ws/{table_code}") as ws_1:
        ws_1.send_json({"type": "join_table", "payload": {"nickname": "P1"}})
        second_snapshot = ws_1.receive_json()
        assert second_snapshot["payload"]["local_seat"] == 1

        session_factory = test_app.state.session_factory
        with session_factory() as session:
            connected_flags = session.scalars(
                select(PlayerSessionRecord.connected).order_by(PlayerSessionRecord.id)
            ).all()
        assert connected_flags == [False, True]

        with client.websocket_connect(f"/ws/{table_code}") as ws_reconnect:
            ws_reconnect.send_json(
                {"type": "reconnect", "payload": {"reconnect_token": reconnect_token}}
            )
            reconnect_snapshot = ws_reconnect.receive_json()
            assert reconnect_snapshot["type"] == "room_snapshot"
            assert reconnect_snapshot["payload"]["local_seat"] == 0

            with session_factory() as session:
                connected_flags = session.scalars(
                    select(PlayerSessionRecord.connected).order_by(PlayerSessionRecord.id)
                ).all()
            assert connected_flags == [True, True]


def test_reconnect_token_restores_original_player_session_identity(test_app) -> None:
    client = TestClient(test_app)
    table_code = client.post("/api/tables").json()["table_code"]

    with client.websocket_connect(f"/ws/{table_code}") as ws_0:
        ws_0.send_json({"type": "join_table", "payload": {"nickname": "P0"}})
        join_0 = ws_0.receive_json()

    with client.websocket_connect(f"/ws/{table_code}") as ws_1:
        ws_1.send_json({"type": "join_table", "payload": {"nickname": "P1"}})
        ws_1.receive_json()

    reconnect_token = join_0["payload"]["reconnect_token"]
    session_factory = test_app.state.session_factory
    with session_factory() as session:
        first_player, second_player = session.scalars(
            select(PlayerSessionRecord).order_by(PlayerSessionRecord.id)
        ).all()
        seat_zero = session.scalar(
            select(TableSeatRecord).where(TableSeatRecord.seat_index == 0)
        )
        assert seat_zero is not None
        seat_zero.player_session_id = second_player.id
        session.commit()

    test_app.state.game_service._rooms.clear()

    with client.websocket_connect(f"/ws/{table_code}") as ws_reconnect:
        ws_reconnect.send_json(
            {"type": "reconnect", "payload": {"reconnect_token": reconnect_token}}
        )
        reconnect_snapshot = ws_reconnect.receive_json()

    assert reconnect_snapshot["type"] == "room_snapshot"
    assert reconnect_snapshot["payload"]["local_seat"] == 0
    assert reconnect_snapshot["payload"]["seats"][0]["nickname"] == first_player.nickname
    assert reconnect_snapshot["payload"]["reconnect_token"] != reconnect_token


def test_reconnect_rejects_when_in_memory_reservation_identity_mismatches(test_app) -> None:
    client = TestClient(test_app)
    table_code = client.post("/api/tables").json()["table_code"]

    with client.websocket_connect(f"/ws/{table_code}") as ws_0:
        ws_0.send_json({"type": "join_table", "payload": {"nickname": "P0"}})
        join_0 = ws_0.receive_json()

    with client.websocket_connect(f"/ws/{table_code}") as ws_1:
        ws_1.send_json({"type": "join_table", "payload": {"nickname": "P1"}})
        ws_1.receive_json()

    reconnect_token = join_0["payload"]["reconnect_token"]
    room = test_app.state.game_service._rooms[table_code]
    room.seats[0].player_session_id = room.seats[1].player_session_id

    with client.websocket_connect(f"/ws/{table_code}") as ws_reconnect:
        ws_reconnect.send_json(
            {"type": "reconnect", "payload": {"reconnect_token": reconnect_token}}
        )
        response = ws_reconnect.receive_json()

    assert response == {
        "type": "action_rejected",
        "payload": {"reason": "invalid_reconnect_token"},
    }
