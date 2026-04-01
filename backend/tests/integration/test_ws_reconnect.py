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
    snapshots = [ws.receive_json() for ws in sockets]
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
    return active_snapshot, start_prompt


def test_reconnect_returns_full_active_round_snapshot_and_rotates_reconnect_token(
    test_app,
) -> None:
    client = TestClient(test_app)
    table_code = client.post("/api/tables").json()["table_code"]

    with ExitStack() as stack:
        ws_0 = stack.enter_context(client.websocket_connect(f"/ws/{table_code}"))
        ws_1 = stack.enter_context(client.websocket_connect(f"/ws/{table_code}"))
        ws_2 = stack.enter_context(client.websocket_connect(f"/ws/{table_code}"))
        ws_3 = stack.enter_context(client.websocket_connect(f"/ws/{table_code}"))

        first_join = _join_player(ws_0, "P0")
        second_join = _join_player(ws_1, "P1")
        assert second_join["payload"]["local_seat"] == 1
        assert ws_0.receive_json()["type"] == "player_presence"
        assert ws_0.receive_json()["type"] == "room_snapshot"

        third_join = _join_player(ws_2, "P2")
        assert third_join["payload"]["local_seat"] == 2
        for peer in (ws_0, ws_1):
            assert peer.receive_json()["type"] == "player_presence"
            assert peer.receive_json()["type"] == "room_snapshot"

        fourth_join = _join_player(ws_3, "P3")
        assert fourth_join["payload"]["phase"] == "waiting"
        assert fourth_join["payload"]["private_state"] is None
        for peer in (ws_1, ws_2):
            assert peer.receive_json()["type"] == "player_presence"
            assert peer.receive_json()["type"] == "room_snapshot"
        assert ws_0.receive_json()["type"] == "player_presence"
        assert ws_0.receive_json()["type"] == "room_snapshot"
        _ready_all_and_start(ws_0, ws_1, ws_2, ws_3)

        original_token = first_join["payload"]["reconnect_token"]

        ws_reconnect = stack.enter_context(client.websocket_connect(f"/ws/{table_code}"))
        ws_reconnect.send_json(
            {"type": "reconnect", "payload": {"reconnect_token": original_token}}
        )
        reconnect_snapshot = ws_reconnect.receive_json()

        peer_presence = ws_1.receive_json()
        peer_snapshot = ws_1.receive_json()

    assert reconnect_snapshot["type"] == "room_snapshot"
    assert reconnect_snapshot["payload"]["table_code"] == table_code
    assert reconnect_snapshot["payload"]["local_seat"] == 0
    assert reconnect_snapshot["payload"]["phase"] == "playing"
    assert len(reconnect_snapshot["payload"]["seats"]) == 4
    assert reconnect_snapshot["payload"]["reconnect_token"]
    assert reconnect_snapshot["payload"]["reconnect_token"] != original_token
    assert reconnect_snapshot["payload"]["private_state"] is not None
    assert (
        reconnect_snapshot["payload"]["private_state"]["current_actor"]
        == reconnect_snapshot["payload"]["private_state"]["dealer_seat"]
    )
    current_actor = reconnect_snapshot["payload"]["private_state"]["current_actor"]
    players = reconnect_snapshot["payload"]["private_state"]["players"]
    assert players[0]["concealed_tiles"]
    for seat_index, player in enumerate(players[1:], start=1):
        assert player["concealed_tiles"] is None
        expected_count = 14 if seat_index == current_actor else 13
        assert player["concealed_count"] == expected_count

    assert peer_presence == {
        "type": "player_presence",
        "payload": {"table_code": table_code, "seat_index": 0, "connected": True},
    }
    assert peer_snapshot["type"] == "room_snapshot"
    assert peer_snapshot["payload"]["phase"] == "playing"

    with client.websocket_connect(f"/ws/{table_code}") as ws_reuse:
        ws_reuse.send_json(
            {"type": "reconnect", "payload": {"reconnect_token": original_token}}
        )
        rejected = ws_reuse.receive_json()

    assert rejected == {
        "type": "action_rejected",
        "payload": {"reason": "invalid_reconnect_token"},
    }
