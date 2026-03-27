from __future__ import annotations

import secrets

from sqlalchemy.orm import Session

from app.db.models import ReconnectTokenRecord
from app.db.models import utcnow
from app.db.repositories import get_reconnect_token as get_reconnect_token_record
from app.db.repositories import issue_reconnect_token as create_reconnect_token_record


def issue_reconnect_token(
    session: Session,
    *,
    table_id: int,
    seat_index: int,
    player_session_id: int,
) -> ReconnectTokenRecord:
    return create_reconnect_token_record(
        session,
        table_id=table_id,
        seat_index=seat_index,
        player_session_id=player_session_id,
        token=secrets.token_urlsafe(24),
    )


def consume_reconnect_token(
    session: Session,
    *,
    token: str,
) -> ReconnectTokenRecord | None:
    record = get_reconnect_token_record(session, token)
    if record is None or record.consumed_at is not None:
        return None

    record.consumed_at = utcnow()
    session.commit()
    session.refresh(record)
    return record
