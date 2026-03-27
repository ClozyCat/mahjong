from __future__ import annotations

from datetime import datetime, timezone

from sqlalchemy import JSON, Boolean, DateTime, ForeignKey, Integer, String, Text, UniqueConstraint
from sqlalchemy.orm import Mapped, mapped_column

from app.db.base import Base


def utcnow() -> datetime:
    return datetime.now(timezone.utc)


class TableRecord(Base):
    __tablename__ = "tables"

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    table_code: Mapped[str] = mapped_column(String(12), unique=True, index=True)
    phase: Mapped[str] = mapped_column(String(32), default="waiting")
    current_round_id: Mapped[str | None] = mapped_column(String(64), nullable=True)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)


class TableSeatRecord(Base):
    __tablename__ = "table_seats"
    __table_args__ = (UniqueConstraint("table_id", "seat_index"),)

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    table_id: Mapped[int] = mapped_column(ForeignKey("tables.id", ondelete="CASCADE"), index=True)
    seat_index: Mapped[int] = mapped_column(Integer)
    player_session_id: Mapped[int | None] = mapped_column(
        ForeignKey("player_sessions.id", ondelete="SET NULL"),
        nullable=True,
    )
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)


class PlayerSessionRecord(Base):
    __tablename__ = "player_sessions"

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    table_id: Mapped[int] = mapped_column(ForeignKey("tables.id", ondelete="CASCADE"), index=True)
    seat_index: Mapped[int | None] = mapped_column(Integer, nullable=True)
    nickname: Mapped[str] = mapped_column(String(64))
    connected: Mapped[bool] = mapped_column(Boolean, default=True)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)


class ReconnectTokenRecord(Base):
    __tablename__ = "reconnect_tokens"

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    table_id: Mapped[int] = mapped_column(ForeignKey("tables.id", ondelete="CASCADE"), index=True)
    seat_index: Mapped[int] = mapped_column(Integer)
    player_session_id: Mapped[int] = mapped_column(
        ForeignKey("player_sessions.id", ondelete="CASCADE"),
        index=True,
    )
    token: Mapped[str] = mapped_column(String(128), unique=True, index=True)
    issued_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)
    consumed_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), nullable=True)


class RoomSnapshotRecord(Base):
    __tablename__ = "room_snapshots"

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    table_id: Mapped[int] = mapped_column(ForeignKey("tables.id", ondelete="CASCADE"), unique=True)
    room_version: Mapped[int] = mapped_column(Integer, default=0)
    payload: Mapped[dict] = mapped_column(JSON)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)


class RoundSnapshotRecord(Base):
    __tablename__ = "round_snapshots"
    __table_args__ = (UniqueConstraint("table_id", "round_id"),)

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    table_id: Mapped[int] = mapped_column(ForeignKey("tables.id", ondelete="CASCADE"), index=True)
    round_id: Mapped[str] = mapped_column(String(64))
    round_version: Mapped[int] = mapped_column(Integer, default=0)
    payload: Mapped[dict] = mapped_column(JSON)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)


class SettlementRecord(Base):
    __tablename__ = "settlements"

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    table_id: Mapped[int] = mapped_column(ForeignKey("tables.id", ondelete="CASCADE"), index=True)
    round_id: Mapped[str] = mapped_column(String(64), index=True)
    win_type: Mapped[str] = mapped_column(String(32))
    winner_seat: Mapped[int | None] = mapped_column(Integer, nullable=True)
    discarder_seat: Mapped[int | None] = mapped_column(Integer, nullable=True)
    fan_total_by_seat: Mapped[dict] = mapped_column(JSON)
    score_delta_by_seat: Mapped[dict] = mapped_column(JSON)
    implemented_fan_keys: Mapped[list] = mapped_column(JSON)
    flower_count_by_seat: Mapped[dict] = mapped_column(JSON)
    final_public_hand_shapes: Mapped[dict] = mapped_column(JSON)
    finished_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)


class RoundEventRecord(Base):
    __tablename__ = "round_events"

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    table_id: Mapped[int] = mapped_column(ForeignKey("tables.id", ondelete="CASCADE"), index=True)
    round_id: Mapped[str | None] = mapped_column(String(64), nullable=True, index=True)
    event_type: Mapped[str] = mapped_column(String(64))
    payload: Mapped[dict | None] = mapped_column(JSON, nullable=True)
    event_text: Mapped[str | None] = mapped_column(Text, nullable=True)
    created_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=utcnow)
