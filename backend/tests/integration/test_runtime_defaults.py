from pathlib import Path
from uuid import uuid4

from alembic import command
from alembic.config import Config
from fastapi.testclient import TestClient
from sqlalchemy import inspect

from app.core.config import Settings
from app.db.session import create_engine_for_url
from app.main import create_app


def _migration_database_url(name: str) -> str:
    artifacts_dir = Path(__file__).resolve().parents[1] / ".pytest-artifacts"
    artifacts_dir.mkdir(exist_ok=True)
    database_path = artifacts_dir / f"{name}-{uuid4().hex}.db"
    return f"sqlite+pysqlite:///{database_path.as_posix()}"


def test_default_settings_disable_test_mode() -> None:
    settings = Settings()

    assert settings.test_mode is False


def test_default_app_allows_second_real_player_to_join_same_room() -> None:
    client = TestClient(create_app())
    table_code = client.post("/api/tables").json()["table_code"]

    with client.websocket_connect(f"/ws/{table_code}") as ws_0:
        ws_0.send_json({"type": "join_table", "payload": {"nickname": "P0"}})
        first_join = ws_0.receive_json()

        with client.websocket_connect(f"/ws/{table_code}") as ws_1:
            ws_1.send_json({"type": "join_table", "payload": {"nickname": "P1"}})
            second_join = ws_1.receive_json()

    assert first_join["type"] == "room_snapshot"
    assert first_join["payload"]["local_seat"] == 0
    assert second_join["type"] == "room_snapshot"
    assert second_join["payload"]["local_seat"] == 1


def test_alembic_schema_supports_reconnect_token_player_session_id() -> None:
    database_url = _migration_database_url("alembic-schema")
    alembic_config = Config("alembic.ini")
    alembic_config.set_main_option("sqlalchemy.url", database_url)

    command.upgrade(alembic_config, "head")

    engine = create_engine_for_url(database_url)
    inspector = inspect(engine)
    reconnect_token_columns = {
        column["name"] for column in inspector.get_columns("reconnect_tokens")
    }

    assert "player_session_id" in reconnect_token_columns
