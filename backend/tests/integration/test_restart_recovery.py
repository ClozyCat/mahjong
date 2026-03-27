from contextlib import ExitStack

from fastapi.testclient import TestClient


def _join_player(ws, nickname: str) -> dict:
    ws.send_json({"type": "join_table", "payload": {"nickname": nickname}})
    return ws.receive_json()


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
    start_snapshot = sockets[0].receive_json()
    start_prompt = sockets[0].receive_json()
    for peer in sockets[1:]:
        peer_snapshot = peer.receive_json()
        assert peer_snapshot["type"] == "room_snapshot"
        assert peer_snapshot["payload"]["phase"] == "playing"
    return start_snapshot, start_prompt


def test_reconnect_restores_live_match_after_room_cache_reset(test_app) -> None:
    client = TestClient(test_app)
    table_code = client.post("/api/tables").json()["table_code"]

    with ExitStack() as stack:
        ws_0 = stack.enter_context(client.websocket_connect(f"/ws/{table_code}"))
        ws_1 = stack.enter_context(client.websocket_connect(f"/ws/{table_code}"))
        ws_2 = stack.enter_context(client.websocket_connect(f"/ws/{table_code}"))
        ws_3 = stack.enter_context(client.websocket_connect(f"/ws/{table_code}"))

        first_join = _join_player(ws_0, "P0")
        reconnect_token = first_join["payload"]["reconnect_token"]

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
        assert active_snapshot["payload"]["phase"] == "playing"
        assert action_prompt["type"] == "action_prompt"

    test_app.state.game_service._rooms.clear()

    with client.websocket_connect(f"/ws/{table_code}") as ws_reconnect:
        ws_reconnect.send_json(
            {"type": "reconnect", "payload": {"reconnect_token": reconnect_token}}
        )
        reconnect_snapshot = ws_reconnect.receive_json()

    assert reconnect_snapshot["type"] == "room_snapshot"
    assert reconnect_snapshot["payload"]["phase"] == "playing"
    assert reconnect_snapshot["payload"]["private_state"] is not None
    assert reconnect_snapshot["payload"]["private_state"]["current_actor"] == 0
    assert reconnect_snapshot["payload"]["private_state"]["players"][0]["concealed_tiles"]
