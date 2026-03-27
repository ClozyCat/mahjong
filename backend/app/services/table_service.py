from __future__ import annotations

import secrets
import string
from dataclasses import dataclass
from datetime import datetime

from sqlalchemy.orm import Session

from app.db.repositories import (
    TableDTO,
    create_table_record,
    get_table_record_by_code,
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


def create_table(session: Session) -> TableDTO:
    for _ in range(TABLE_CODE_ATTEMPTS):
        table_code = _generate_table_code()
        if table_code_exists(session, table_code):
            continue
        record = create_table_record(session, table_code=table_code, phase="waiting")
        return to_table_dto(record)
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
