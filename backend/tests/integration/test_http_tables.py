from fastapi.testclient import TestClient

from app.main import create_app


def test_health_and_create_table_return_waiting_room(test_app) -> None:
    client = TestClient(test_app)

    health_response = client.get("/api/health")

    assert health_response.status_code == 200
    assert health_response.json() == {"status": "ok"}

    create_response = client.post("/api/tables")

    assert create_response.status_code == 201
    payload = create_response.json()
    assert payload["table_code"]
    assert payload["phase"] == "waiting"
    assert payload["seats"] == []


def test_create_table_uses_requested_code_when_provided(test_app) -> None:
    client = TestClient(test_app)

    create_response = client.post("/api/tables", json={"table_code": "ROOM42"})

    assert create_response.status_code == 201
    assert create_response.json()["table_code"] == "ROOM42"


def test_create_table_returns_conflict_for_existing_requested_code(test_app) -> None:
    client = TestClient(test_app)

    first = client.post("/api/tables", json={"table_code": "ROOM42"})
    with client.websocket_connect("/ws/ROOM42") as websocket:
        websocket.send_json({"type": "join_table", "payload": {"nickname": "P0"}})
        assert websocket.receive_json()["type"] == "room_snapshot"
    second = client.post("/api/tables", json={"table_code": "ROOM42"})

    assert first.status_code == 201
    assert second.status_code == 409
    assert second.json() == {"detail": "table_code_exists"}


def test_create_table_reuses_requested_code_when_existing_table_has_no_players(test_app) -> None:
    client = TestClient(test_app)

    first = client.post("/api/tables", json={"table_code": "ROOM42"})
    second = client.post("/api/tables", json={"table_code": "ROOM42"})

    assert first.status_code == 201
    assert second.status_code == 201
    assert second.json()["table_code"] == "ROOM42"


def test_create_table_accepts_test_mode_override(test_app) -> None:
    client = TestClient(test_app)

    create_response = client.post("/api/tables", json={"table_code": "ROOM99", "test_mode": True})

    assert create_response.status_code == 201
    table_code = create_response.json()["table_code"]

    with client.websocket_connect(f"/ws/{table_code}") as websocket:
        websocket.send_json({"type": "join_table", "payload": {"nickname": "Solo"}})
        snapshot = websocket.receive_json()

    assert snapshot["type"] == "room_snapshot"
    assert snapshot["payload"]["phase"] == "playing"


def test_create_table_can_disable_test_mode_even_when_app_default_is_true(test_mode_app) -> None:
    client = TestClient(test_mode_app)

    create_response = client.post("/api/tables", json={"table_code": "ROOM98", "test_mode": False})

    assert create_response.status_code == 201
    table_code = create_response.json()["table_code"]

    with client.websocket_connect(f"/ws/{table_code}") as websocket:
        websocket.send_json({"type": "join_table", "payload": {"nickname": "Solo"}})
        snapshot = websocket.receive_json()

    assert snapshot["type"] == "room_snapshot"
    assert snapshot["payload"]["phase"] == "waiting"


def test_default_app_factory_boots_with_a_working_session_factory() -> None:
    client = TestClient(create_app())

    create_response = client.post("/api/tables")

    assert create_response.status_code == 201


def test_preflight_for_create_table_allows_local_vite_origin(test_app) -> None:
    client = TestClient(test_app)

    response = client.options(
        "/api/tables",
        headers={
            "Origin": "http://localhost:5173",
            "Access-Control-Request-Method": "POST",
        },
    )

    assert response.status_code == 200
    assert response.headers["access-control-allow-origin"] == "http://localhost:5173"
