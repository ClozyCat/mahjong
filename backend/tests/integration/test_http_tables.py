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
