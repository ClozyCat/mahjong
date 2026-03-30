from __future__ import annotations

from collections.abc import Iterator

from fastapi import APIRouter, Depends, HTTPException, Request, status
from sqlalchemy.orm import Session, sessionmaker

from app.api.schemas import CreateTableRequest, HealthResponse, WaitingRoomResponse
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
    request: Request,
    payload: CreateTableRequest | None = None,
    session: Session = Depends(get_db_session),
) -> WaitingRoomResponse:
    try:
        table = create_table(
            session,
            payload.table_code if payload else None,
            test_mode=(
                payload.test_mode
                if payload is not None and payload.test_mode is not None
                else request.app.state.settings.test_mode
            ),
            enforce_minimum_eight_fan=(
                payload.enforce_minimum_eight_fan
                if payload is not None and payload.enforce_minimum_eight_fan is not None
                else True
            ),
        )
    except ValueError as exc:
        if str(exc) == "table_code_exists":
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail="table_code_exists",
            ) from exc
        raise
    return WaitingRoomResponse.model_validate(to_waiting_room(table))
