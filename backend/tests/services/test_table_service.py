from sqlalchemy import select

from app.db.models import RoomSnapshotRecord, TableRecord
import pytest

from app.db.repositories import create_player_session, get_table_record_by_code
from app.services.table_service import close_table, create_table


def test_create_table_persists_waiting_room(db_session):
    room = create_table(db_session)
    persisted = db_session.scalar(
        select(TableRecord).where(TableRecord.table_code == room.table_code)
    )

    assert room.table_code
    assert room.phase == "waiting"
    assert persisted is not None
    assert persisted.phase == "waiting"


def test_create_table_uses_requested_code_when_available(db_session):
    room = create_table(db_session, "room42")

    assert room.table_code == "ROOM42"


def test_create_table_rejects_duplicate_requested_code(db_session):
    room = create_table(db_session, "ROOM42")
    table_record = get_table_record_by_code(db_session, room.table_code)
    assert table_record is not None
    create_player_session(
        db_session,
        table_id=table_record.id,
        seat_index=0,
        nickname="Player A",
    )

    with pytest.raises(ValueError, match="table_code_exists"):
        create_table(db_session, "ROOM42")


def test_create_table_reuses_requested_code_when_existing_table_has_no_players(db_session):
    create_table(db_session, "ROOM42")

    room = create_table(db_session, "ROOM42")

    assert room.table_code == "ROOM42"


def test_create_table_persists_requested_test_mode_in_room_snapshot(db_session):
    room = create_table(db_session, "ROOM43", test_mode=True)
    persisted_snapshot = db_session.scalar(
        select(RoomSnapshotRecord).join(TableRecord, RoomSnapshotRecord.table_id == TableRecord.id).where(TableRecord.table_code == room.table_code)
    )

    assert persisted_snapshot is not None
    assert persisted_snapshot.payload["mode"] == "test"
    assert persisted_snapshot.payload["test_mode"] is True


def test_create_table_persists_requested_ai_mode_in_room_snapshot(db_session):
    room = create_table(db_session, "ROOM43A", mode="ai")
    persisted_snapshot = db_session.scalar(
        select(RoomSnapshotRecord).join(TableRecord, RoomSnapshotRecord.table_id == TableRecord.id).where(TableRecord.table_code == room.table_code)
    )

    assert persisted_snapshot is not None
    assert persisted_snapshot.payload["mode"] == "ai"
    assert persisted_snapshot.payload["test_mode"] is False


def test_create_table_persists_eight_fan_rule_toggle_in_room_snapshot(db_session):
    room = create_table(db_session, "ROOM44", enforce_minimum_eight_fan=False)
    persisted_snapshot = db_session.scalar(
        select(RoomSnapshotRecord).join(TableRecord, RoomSnapshotRecord.table_id == TableRecord.id).where(TableRecord.table_code == room.table_code)
    )

    assert persisted_snapshot is not None
    assert persisted_snapshot.payload["enforce_minimum_eight_fan"] is False


def test_close_table_deletes_persisted_record(db_session):
    room = create_table(db_session, "ROOM42")

    assert close_table(db_session, room.table_code) is True
    assert db_session.scalar(select(TableRecord).where(TableRecord.table_code == room.table_code)) is None
