from contextlib import ExitStack

from fastapi.testclient import TestClient


def _join_player(ws, nickname: str) -> dict:
    ws.send_json({"type": "join_table", "payload": {"nickname": nickname}})
    return ws.receive_json()


def _mark_ready(ws) -> dict:
    ws.send_json({"type": "ready", "payload": {"ready": True}})
    return ws.receive_json()


def test_four_players_join_room_without_auto_start(test_app) -> None:
    client = TestClient(test_app)
    table_code = client.post("/api/tables").json()["table_code"]

    with ExitStack() as stack:
        sockets = [
            stack.enter_context(client.websocket_connect(f"/ws/{table_code}"))
            for _ in range(4)
        ]

        first_join = _join_player(sockets[0], "P0")
        second_join = _join_player(sockets[1], "P1")
        assert sockets[0].receive_json()["type"] == "player_presence"
        assert sockets[0].receive_json()["type"] == "room_snapshot"

        third_join = _join_player(sockets[2], "P2")
        for peer in sockets[:2]:
            assert peer.receive_json()["type"] == "player_presence"
            assert peer.receive_json()["type"] == "room_snapshot"

        fourth_join = _join_player(sockets[3], "P3")

    assert first_join["payload"]["phase"] == "waiting"
    assert second_join["payload"]["phase"] == "waiting"
    assert third_join["payload"]["phase"] == "waiting"
    assert fourth_join["payload"]["phase"] == "waiting"
    assert fourth_join["payload"]["private_state"] is None
    assert all(seat["ready"] is False for seat in fourth_join["payload"]["seats"])


def test_room_starts_only_after_all_players_ready_and_start_request(test_app) -> None:
    client = TestClient(test_app)
    table_code = client.post("/api/tables").json()["table_code"]

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

        for ready_index, ws in enumerate(sockets):
            ready_snapshot = _mark_ready(ws)
            assert ready_snapshot["type"] == "room_snapshot"
            assert ready_snapshot["payload"]["seats"][ready_index]["ready"] is True
            for peer in sockets:
                if peer is ws:
                    continue
                peer_snapshot = peer.receive_json()
                assert peer_snapshot["type"] == "room_snapshot"
                assert peer_snapshot["payload"]["seats"][ready_index]["ready"] is True

        sockets[0].send_json({"type": "start_match", "payload": {}})
        start_snapshot = sockets[0].receive_json()
        start_prompt = sockets[0].receive_json()
        peer_snapshots = [ws.receive_json() for ws in sockets[1:]]

    assert start_snapshot["type"] == "room_snapshot"
    assert start_snapshot["payload"]["phase"] == "playing"
    assert start_snapshot["payload"]["private_state"] is not None
    assert start_prompt["type"] == "action_prompt"
    assert all(message["type"] == "room_snapshot" for message in peer_snapshots)
    assert all(message["payload"]["phase"] == "playing" for message in peer_snapshots)


def test_start_match_rejects_when_not_all_players_ready(test_app) -> None:
    client = TestClient(test_app)
    table_code = client.post("/api/tables").json()["table_code"]

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

        _mark_ready(sockets[0])
        for ws in sockets[1:]:
            assert ws.receive_json()["type"] == "room_snapshot"

        sockets[0].send_json({"type": "start_match", "payload": {}})
        rejected = sockets[0].receive_json()

    assert rejected == {
        "type": "action_rejected",
        "payload": {"reason": "room_not_ready"},
    }
