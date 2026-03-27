from __future__ import annotations

from collections.abc import Iterator

from fastapi import APIRouter, Depends, Request, status
from sqlalchemy.orm import Session, sessionmaker

from app.api.schemas import HealthResponse, WaitingRoomResponse
from app.db.session import get_session
from app.services.table_service import create_table, to_waiting_room

router = APIRouter(prefix="/api")


def get_db_session(request: Request) -> Iterator[Session]:
    session_factory: sessionmaker[Session] = request.app.state.session_factory
    yield from get_session(session_factory)


@router.get("/health", response_model=HealthResponse)
def healthcheck() -> HealthResponse:
    return HealthResponse(status="ok")


@router.post(
    "/tables",
    response_model=WaitingRoomResponse,
    status_code=status.HTTP_201_CREATED,
)
def create_table_endpoint(
    session: Session = Depends(get_db_session),
) -> WaitingRoomResponse:
    table = create_table(session)
    return WaitingRoomResponse.model_validate(to_waiting_room(table))
