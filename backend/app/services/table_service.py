from __future__ import annotations

import secrets
import string
from dataclasses import dataclass
from datetime import datetime

from sqlalchemy.orm import Session

from app.db.repositories import (
    TableDTO,
    create_table_record,
    delete_table_record,
    get_table_record_by_code,
    save_room_snapshot,
    table_has_players,
    table_code_exists,
    to_table_dto,
)

TABLE_CODE_ALPHABET = string.ascii_uppercase + string.digits
TABLE_CODE_LENGTH = 6
TABLE_CODE_ATTEMPTS = 16


@dataclass(frozen=True)
class WaitingRoomSeatDTO:
    seat_index: int
    nickname: str | None
    connected: bool
    ready: bool = False


@dataclass(frozen=True)
class WaitingRoomDTO:
    table_code: str
    phase: str
    created_at: datetime
    seats: list[WaitingRoomSeatDTO]


def _generate_table_code() -> str:
    return "".join(secrets.choice(TABLE_CODE_ALPHABET) for _ in range(TABLE_CODE_LENGTH))


def normalize_table_code(table_code: str) -> str:
    return table_code.strip().upper()


def _initial_room_payload(*, table_code: str, test_mode: bool) -> dict:
    return {
        "table_code": table_code,
        "phase": "waiting",
        "test_mode": test_mode,
        "seats": [],
        "match_state": None,
        "round_state": None,
        "pending_timeout": None,
    }


def _create_waiting_table(
    session: Session,
    *,
    table_code: str,
    test_mode: bool,
) -> TableDTO:
    record = create_table_record(session, table_code=table_code, phase="waiting")
    save_room_snapshot(
        session,
        table_id=record.id,
        room_version=0,
        payload=_initial_room_payload(table_code=table_code, test_mode=test_mode),
    )
    return to_table_dto(record)


def create_table(
    session: Session,
    table_code: str | None = None,
    *,
    test_mode: bool = False,
) -> TableDTO:
    if table_code:
        normalized_table_code = normalize_table_code(table_code)
        existing_record = get_table_record_by_code(session, normalized_table_code)
        if existing_record is not None:
            if table_has_players(session, table_id=existing_record.id):
                raise ValueError("table_code_exists")
            delete_table_record(session, normalized_table_code)
        return _create_waiting_table(
            session,
            table_code=normalized_table_code,
            test_mode=test_mode,
        )

    for _ in range(TABLE_CODE_ATTEMPTS):
        table_code = _generate_table_code()
        if table_code_exists(session, table_code):
            continue
        return _create_waiting_table(
            session,
            table_code=table_code,
            test_mode=test_mode,
        )
    raise RuntimeError("Unable to generate a unique table code")


def to_waiting_room(table: TableDTO) -> WaitingRoomDTO:
    return WaitingRoomDTO(
        table_code=table.table_code,
        phase=table.phase,
        created_at=table.created_at,
        seats=[],
    )


def get_table_by_code(session: Session, table_code: str) -> TableDTO | None:
    record = get_table_record_by_code(session, table_code)
    if record is None:
        return None
    return to_table_dto(record)


def close_table(session: Session, table_code: str) -> bool:
    return delete_table_record(session, normalize_table_code(table_code))
