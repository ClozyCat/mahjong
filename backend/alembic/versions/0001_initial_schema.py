"""initial schema

Revision ID: 0001_initial_schema
Revises:
Create Date: 2026-03-25
"""

from __future__ import annotations

from alembic import op
import sqlalchemy as sa


revision = "0001_initial_schema"
down_revision = None
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.create_table(
        "tables",
        sa.Column("id", sa.Integer(), primary_key=True),
        sa.Column("table_code", sa.String(length=12), nullable=False),
        sa.Column("phase", sa.String(length=32), nullable=False),
        sa.Column("current_round_id", sa.String(length=64), nullable=True),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
    )
    op.create_index("ix_tables_table_code", "tables", ["table_code"], unique=True)

    op.create_table(
        "player_sessions",
        sa.Column("id", sa.Integer(), primary_key=True),
        sa.Column("table_id", sa.Integer(), sa.ForeignKey("tables.id", ondelete="CASCADE"), nullable=False),
        sa.Column("seat_index", sa.Integer(), nullable=True),
        sa.Column("nickname", sa.String(length=64), nullable=False),
        sa.Column("connected", sa.Boolean(), nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
    )

    op.create_table(
        "table_seats",
        sa.Column("id", sa.Integer(), primary_key=True),
        sa.Column("table_id", sa.Integer(), sa.ForeignKey("tables.id", ondelete="CASCADE"), nullable=False),
        sa.Column("seat_index", sa.Integer(), nullable=False),
        sa.Column("player_session_id", sa.Integer(), sa.ForeignKey("player_sessions.id", ondelete="SET NULL"), nullable=True),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
        sa.UniqueConstraint("table_id", "seat_index", name="uq_table_seats_table_id_seat_index"),
    )

    op.create_table(
        "reconnect_tokens",
        sa.Column("id", sa.Integer(), primary_key=True),
        sa.Column("table_id", sa.Integer(), sa.ForeignKey("tables.id", ondelete="CASCADE"), nullable=False),
        sa.Column("seat_index", sa.Integer(), nullable=False),
        sa.Column("player_session_id", sa.Integer(), sa.ForeignKey("player_sessions.id", ondelete="CASCADE"), nullable=False),
        sa.Column("token", sa.String(length=128), nullable=False),
        sa.Column("issued_at", sa.DateTime(timezone=True), nullable=False),
        sa.Column("consumed_at", sa.DateTime(timezone=True), nullable=True),
    )
    op.create_index(
        "ix_reconnect_tokens_player_session_id",
        "reconnect_tokens",
        ["player_session_id"],
        unique=False,
    )
    op.create_index("ix_reconnect_tokens_token", "reconnect_tokens", ["token"], unique=True)

    op.create_table(
        "room_snapshots",
        sa.Column("id", sa.Integer(), primary_key=True),
        sa.Column("table_id", sa.Integer(), sa.ForeignKey("tables.id", ondelete="CASCADE"), nullable=False),
        sa.Column("room_version", sa.Integer(), nullable=False),
        sa.Column("payload", sa.JSON(), nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
        sa.UniqueConstraint("table_id", name="uq_room_snapshots_table_id"),
    )

    op.create_table(
        "round_snapshots",
        sa.Column("id", sa.Integer(), primary_key=True),
        sa.Column("table_id", sa.Integer(), sa.ForeignKey("tables.id", ondelete="CASCADE"), nullable=False),
        sa.Column("round_id", sa.String(length=64), nullable=False),
        sa.Column("round_version", sa.Integer(), nullable=False),
        sa.Column("payload", sa.JSON(), nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
        sa.UniqueConstraint("table_id", "round_id", name="uq_round_snapshots_table_id_round_id"),
    )

    op.create_table(
        "settlements",
        sa.Column("id", sa.Integer(), primary_key=True),
        sa.Column("table_id", sa.Integer(), sa.ForeignKey("tables.id", ondelete="CASCADE"), nullable=False),
        sa.Column("round_id", sa.String(length=64), nullable=False),
        sa.Column("win_type", sa.String(length=32), nullable=False),
        sa.Column("winner_seat", sa.Integer(), nullable=True),
        sa.Column("discarder_seat", sa.Integer(), nullable=True),
        sa.Column("fan_total_by_seat", sa.JSON(), nullable=False),
        sa.Column("score_delta_by_seat", sa.JSON(), nullable=False),
        sa.Column("implemented_fan_keys", sa.JSON(), nullable=False),
        sa.Column("flower_count_by_seat", sa.JSON(), nullable=False),
        sa.Column("final_public_hand_shapes", sa.JSON(), nullable=False),
        sa.Column("finished_at", sa.DateTime(timezone=True), nullable=False),
    )

    op.create_table(
        "round_events",
        sa.Column("id", sa.Integer(), primary_key=True),
        sa.Column("table_id", sa.Integer(), sa.ForeignKey("tables.id", ondelete="CASCADE"), nullable=False),
        sa.Column("round_id", sa.String(length=64), nullable=True),
        sa.Column("event_type", sa.String(length=64), nullable=False),
        sa.Column("payload", sa.JSON(), nullable=True),
        sa.Column("event_text", sa.Text(), nullable=True),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
    )


def downgrade() -> None:
    op.drop_table("round_events")
    op.drop_table("settlements")
    op.drop_table("round_snapshots")
    op.drop_table("room_snapshots")
    op.drop_index("ix_reconnect_tokens_player_session_id", table_name="reconnect_tokens")
    op.drop_index("ix_reconnect_tokens_token", table_name="reconnect_tokens")
    op.drop_table("reconnect_tokens")
    op.drop_table("table_seats")
    op.drop_table("player_sessions")
    op.drop_index("ix_tables_table_code", table_name="tables")
    op.drop_table("tables")
