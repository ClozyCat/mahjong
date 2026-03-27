from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime

from sqlalchemy import delete, select
from sqlalchemy.orm import Session

from app.db.models import (
    PlayerSessionRecord,
    ReconnectTokenRecord,
    RoundEventRecord,
    RoundSnapshotRecord,
    RoomSnapshotRecord,
    SettlementRecord,
    TableRecord,
    TableSeatRecord,
    utcnow,
)


@dataclass(frozen=True)
class TableDTO:
    table_code: str
    phase: str
    created_at: datetime


def create_table_record(
    session: Session,
    *,
    table_code: str,
    phase: str = "waiting",
) -> TableRecord:
    record = TableRecord(table_code=table_code, phase=phase)
    session.add(record)
    session.commit()
    session.refresh(record)
    return record


def table_code_exists(session: Session, table_code: str) -> bool:
    existing = session.scalar(
        select(TableRecord.id).where(TableRecord.table_code == table_code)
    )
    return existing is not None


def table_has_players(session: Session, *, table_id: int) -> bool:
    existing = session.scalar(
        select(PlayerSessionRecord.id).where(PlayerSessionRecord.table_id == table_id)
    )
    return existing is not None


def to_table_dto(record: TableRecord) -> TableDTO:
    return TableDTO(
        table_code=record.table_code,
        phase=record.phase,
        created_at=record.created_at,
    )


def get_table_record_by_code(session: Session, table_code: str) -> TableRecord | None:
    return session.scalar(select(TableRecord).where(TableRecord.table_code == table_code))


def delete_table_record(session: Session, table_code: str) -> bool:
    record = get_table_record_by_code(session, table_code)
    if record is None:
        return False
    session.execute(
        delete(RoomSnapshotRecord).where(RoomSnapshotRecord.table_id == record.id)
    )
    session.execute(
        delete(RoundSnapshotRecord).where(RoundSnapshotRecord.table_id == record.id)
    )
    session.execute(
        delete(SettlementRecord).where(SettlementRecord.table_id == record.id)
    )
    session.execute(
        delete(RoundEventRecord).where(RoundEventRecord.table_id == record.id)
    )
    session.execute(
        delete(ReconnectTokenRecord).where(ReconnectTokenRecord.table_id == record.id)
    )
    session.execute(
        delete(TableSeatRecord).where(TableSeatRecord.table_id == record.id)
    )
    session.execute(
        delete(PlayerSessionRecord).where(PlayerSessionRecord.table_id == record.id)
    )
    session.delete(record)
    session.commit()
    return True


def create_player_session(
    session: Session,
    *,
    table_id: int,
    seat_index: int,
    nickname: str,
    connected: bool = True,
) -> PlayerSessionRecord:
    record = PlayerSessionRecord(
        table_id=table_id,
        seat_index=seat_index,
        nickname=nickname,
        connected=connected,
    )
    session.add(record)
    session.commit()
    session.refresh(record)
    return record


def create_table_seat(
    session: Session,
    *,
    table_id: int,
    seat_index: int,
    player_session_id: int,
) -> TableSeatRecord:
    record = session.scalar(
        select(TableSeatRecord).where(
            TableSeatRecord.table_id == table_id,
            TableSeatRecord.seat_index == seat_index,
        )
    )
    if record is None:
        record = TableSeatRecord(
            table_id=table_id,
            seat_index=seat_index,
            player_session_id=player_session_id,
        )
        session.add(record)
    else:
        record.player_session_id = player_session_id
    session.commit()
    session.refresh(record)
    return record


def issue_reconnect_token(
    session: Session,
    *,
    table_id: int,
    seat_index: int,
    player_session_id: int,
    token: str,
) -> ReconnectTokenRecord:
    record = ReconnectTokenRecord(
        table_id=table_id,
        seat_index=seat_index,
        player_session_id=player_session_id,
        token=token,
    )
    session.add(record)
    session.commit()
    session.refresh(record)
    return record


def get_reconnect_token(
    session: Session, token: str
) -> ReconnectTokenRecord | None:
    return session.scalar(
        select(ReconnectTokenRecord).where(ReconnectTokenRecord.token == token)
    )


def get_table_seat(
    session: Session, *, table_id: int, seat_index: int
) -> TableSeatRecord | None:
    return session.scalar(
        select(TableSeatRecord).where(
            TableSeatRecord.table_id == table_id,
            TableSeatRecord.seat_index == seat_index,
        )
    )


def get_player_session(
    session: Session, player_session_id: int
) -> PlayerSessionRecord | None:
    return session.get(PlayerSessionRecord, player_session_id)


def set_player_session_connected(
    session: Session, *, player_session_id: int, connected: bool
) -> PlayerSessionRecord | None:
    record = get_player_session(session, player_session_id)
    if record is None:
        return None
    record.connected = connected
    session.commit()
    session.refresh(record)
    return record


def get_room_snapshot(
    session: Session,
    *,
    table_id: int,
) -> RoomSnapshotRecord | None:
    return session.scalar(
        select(RoomSnapshotRecord).where(RoomSnapshotRecord.table_id == table_id)
    )


def save_room_snapshot(
    session: Session,
    *,
    table_id: int,
    room_version: int,
    payload: dict,
) -> RoomSnapshotRecord:
    record = get_room_snapshot(session, table_id=table_id)
    if record is None:
        record = RoomSnapshotRecord(
            table_id=table_id,
            room_version=room_version,
            payload=payload,
        )
        session.add(record)
    else:
        record.room_version = room_version
        record.payload = payload
        record.created_at = utcnow()

    session.commit()
    session.refresh(record)
    return record
