from __future__ import annotations

from datetime import datetime

from pydantic import BaseModel
from pydantic import ConfigDict
from pydantic import Field
from pydantic import StringConstraints
from typing import Annotated


class HealthResponse(BaseModel):
    status: str


class TableSeatResponse(BaseModel):
    model_config = ConfigDict(from_attributes=True)

    seat_index: int
    nickname: str | None = None
    connected: bool
    ready: bool = False


class WaitingRoomResponse(BaseModel):
    model_config = ConfigDict(from_attributes=True)

    table_code: str
    phase: str
    created_at: datetime
    seats: list[TableSeatResponse]


class CreateTableRequest(BaseModel):
    table_code: Annotated[str | None, StringConstraints(pattern=r"^[A-Z0-9]{1,12}$")] = None
    test_mode: bool | None = None
    enforce_minimum_eight_fan: bool | None = None


class JoinTableRequest(BaseModel):
    nickname: str


class ReconnectRequest(BaseModel):
    reconnect_token: str


class ActionRequestPayload(BaseModel):
    action_type: str
    tile_ids: list[str] = Field(default_factory=list)


class ActionPromptPayload(BaseModel):
    seat_index: int
    options: list[str]
    deadline_at: str


class ActionRejectedPayload(BaseModel):
    reason: str


class PlayerPresencePayload(BaseModel):
    table_code: str
    seat_index: int
    connected: bool


class RoomSnapshotPayload(BaseModel):
    table_code: str
    phase: str
    seats: list[TableSeatResponse]
    local_seat: int
    reconnect_token: str
    match_state: dict | None = None
    private_state: dict | None = None
    continue_action: dict | None = None


class RoundEventPayload(BaseModel):
    event_type: str
    event: dict


class HeartbeatPayload(BaseModel):
    sent_at: str | None = None
