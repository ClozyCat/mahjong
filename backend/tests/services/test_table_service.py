from sqlalchemy import select

from app.db.models import TableRecord
from app.services.table_service import create_table


def test_create_table_persists_waiting_room(db_session):
    room = create_table(db_session)
    persisted = db_session.scalar(
        select(TableRecord).where(TableRecord.table_code == room.table_code)
    )

    assert room.table_code
    assert room.phase == "waiting"
    assert persisted is not None
    assert persisted.phase == "waiting"
