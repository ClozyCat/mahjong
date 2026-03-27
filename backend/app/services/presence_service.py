from __future__ import annotations

from sqlalchemy.orm import Session

from app.db.models import PlayerSessionRecord
from app.db.repositories import set_player_session_connected


def mark_connected(
    session: Session,
    *,
    player_session_id: int,
) -> PlayerSessionRecord | None:
    return set_player_session_connected(
        session,
        player_session_id=player_session_id,
        connected=True,
    )


def mark_disconnected(
    session: Session,
    *,
    player_session_id: int,
) -> PlayerSessionRecord | None:
    return set_player_session_connected(
        session,
        player_session_id=player_session_id,
        connected=False,
    )
