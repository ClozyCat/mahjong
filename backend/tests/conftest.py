from __future__ import annotations

from collections.abc import Iterator
from pathlib import Path
from uuid import uuid4

import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient
from sqlalchemy.orm import Session

from app.db.base import Base
from app.core.config import Settings
from app.db.session import create_engine_for_url, create_session_factory
from app.main import create_app


def _test_database_url(name: str) -> str:
    artifacts_dir = Path(__file__).resolve().parents[1] / ".pytest-artifacts"
    artifacts_dir.mkdir(exist_ok=True)
    database_path = artifacts_dir / f"{name}-{uuid4().hex}.db"
    return f"sqlite+pysqlite:///{database_path.as_posix()}"


@pytest.fixture
def db_session() -> Iterator[Session]:
    database_url = _test_database_url("session")
    engine = create_engine_for_url(database_url)
    session_factory = create_session_factory(database_url)
    Base.metadata.create_all(engine)
    session = session_factory()
    try:
        yield session
    finally:
        session.close()
        engine.dispose()


@pytest.fixture
def test_app() -> FastAPI:
    database_url = _test_database_url("app")
    engine = create_engine_for_url(database_url)
    session_factory = create_session_factory(database_url)
    Base.metadata.create_all(engine)

    return create_app(session_factory=session_factory, engine=engine)


@pytest.fixture
def test_mode_app() -> FastAPI:
    database_url = _test_database_url("test-mode-app")
    engine = create_engine_for_url(database_url)
    session_factory = create_session_factory(database_url)
    Base.metadata.create_all(engine)

    settings = Settings(database_url=database_url, test_mode=True)
    return create_app(
        settings=settings,
        session_factory=session_factory,
        engine=engine,
    )


@pytest.fixture
def ws_client_factory(test_app):
    client = TestClient(test_app)

    def _factory(path: str):
        return client.websocket_connect(path)

    try:
        yield _factory
    finally:
        client.close()
