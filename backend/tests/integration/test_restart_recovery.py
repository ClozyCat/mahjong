from contextlib import ExitStack

from fastapi.testclient import TestClient


def _join_player(ws, nickname: str) -> dict:
    ws.send_json({"type": "join_table", "payload": {"nickname": nickname}})
    return ws.receive_json()


def _receive_until_snapshot(ws) -> dict:
    while True:
        message = ws.receive_json()
        if message["type"] == "room_snapshot":
            return message


def _ready_all_and_start(*sockets) -> tuple[dict, dict | None]:
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
    snapshots = [_receive_until_snapshot(ws) for ws in sockets]
    for snapshot in snapshots:
        assert snapshot["type"] == "room_snapshot"
        assert snapshot["payload"]["phase"] == "playing"

    active_snapshot = next(
        snapshot
        for snapshot in snapshots
        if snapshot["payload"]["private_state"]["pending_action"] is not None
    )
    seat_index = active_snapshot["payload"]["private_state"]["pending_action"]["seat_index"]
    start_prompt = sockets[seat_index].receive_json()
    assert start_prompt["type"] == "action_prompt"

    local_snapshot = next(
        snapshot
        for snapshot in snapshots
        if snapshot["payload"]["local_seat"] == 0
    )
    return local_snapshot, start_prompt


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
        if action_prompt is not None:
            assert action_prompt["type"] == "action_prompt"
        room = test_app.state.game_service._rooms.pop(table_code, None)
        if room is not None and room.timeout_task is not None:
            room.timeout_task.cancel()
        if room is not None and room.bot_action_task is not None:
            room.bot_action_task.cancel()

        with client.websocket_connect(f"/ws/{table_code}") as ws_reconnect:
            ws_reconnect.send_json(
                {"type": "reconnect", "payload": {"reconnect_token": reconnect_token}}
            )
            reconnect_snapshot = ws_reconnect.receive_json()

    assert reconnect_snapshot["type"] == "room_snapshot"
    assert reconnect_snapshot["payload"]["phase"] == "playing"
    assert reconnect_snapshot["payload"]["private_state"] is not None
    assert reconnect_snapshot["payload"]["private_state"]["players"][0]["concealed_tiles"]
