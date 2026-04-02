from __future__ import annotations

import os

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from sqlalchemy.engine import Engine
from sqlalchemy.orm import sessionmaker

from app.api.http import router as http_router
from app.api.ws import router as ws_router
from app.core.config import Settings, get_settings
from app.db.base import Base
from app.db.session import create_engine_for_url, create_session_factory
from app.services.game_service import GameService


DEV_CORS_ORIGINS = [
    "http://localhost:5173",
    "http://127.0.0.1:5173",
]


def get_dev_cors_origins() -> list[str]:
    origins = list(DEV_CORS_ORIGINS)
    extra_origins = os.getenv("MAHJONG_DEV_CORS_ORIGINS", "")
    for origin in (value.strip() for value in extra_origins.split(",")):
        if origin and origin not in origins:
            origins.append(origin)
    return origins


def create_app(
    *,
    settings: Settings | None = None,
    session_factory: sessionmaker | None = None,
    engine: Engine | None = None,
) -> FastAPI:
    app = FastAPI()
    app.add_middleware(
        CORSMiddleware,
        allow_origins=get_dev_cors_origins(),
        allow_credentials=True,
        allow_methods=["*"],
        allow_headers=["*"],
    )
    resolved_settings = settings or get_settings()
    resolved_engine = engine or create_engine_for_url(resolved_settings.database_url)
    resolved_session_factory = session_factory or sessionmaker(
        bind=resolved_engine,
        autoflush=False,
        expire_on_commit=False,
    )

    Base.metadata.create_all(resolved_engine)

    app.state.settings = resolved_settings
    app.state.engine = resolved_engine
    app.state.session_factory = resolved_session_factory
    app.state.game_service = GameService(
        resolved_session_factory,
        test_mode=resolved_settings.test_mode,
    )

    app.include_router(http_router)
    app.include_router(ws_router)
    return app


app = create_app()
