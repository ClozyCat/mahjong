from __future__ import annotations

import asyncio

from fastapi import APIRouter, WebSocket, WebSocketDisconnect

from app.services.game_service import GameService

router = APIRouter()


async def _send_rejection_if_needed(websocket: WebSocket, response: dict) -> None:
    if response.get("type") == "action_rejected":
        await websocket.send_json(response)


@router.websocket("/ws/{table_code}")
async def websocket_endpoint(websocket: WebSocket, table_code: str) -> None:
    await websocket.accept()
    game_service: GameService = websocket.app.state.game_service

    try:
        while True:
            await game_service.process_due_timeout(table_code)
            timeout_seconds = await game_service.next_timeout_seconds(table_code)
            try:
                if timeout_seconds is None:
                    message = await websocket.receive_json()
                else:
                    message = await asyncio.wait_for(
                        websocket.receive_json(),
                        timeout=timeout_seconds,
                    )
            except asyncio.TimeoutError:
                await game_service.process_due_timeout(table_code)
                continue

            message_type = message.get("type")
            payload = message.get("payload", {})

            if message_type == "join_table":
                response = await game_service.join_table(
                    table_code=table_code,
                    nickname=payload.get("nickname", "Player"),
                    websocket=websocket,
                )
                await _send_rejection_if_needed(websocket, response)
                continue

            if message_type == "reconnect":
                response = await game_service.reconnect(
                    table_code=table_code,
                    token=payload.get("reconnect_token", ""),
                    websocket=websocket,
                )
                await _send_rejection_if_needed(websocket, response)
                continue

            if message_type == "ready":
                response = await game_service.mark_ready(
                    table_code=table_code,
                    websocket=websocket,
                    ready=bool(payload.get("ready", True)),
                )
                await _send_rejection_if_needed(websocket, response)
                continue

            if message_type == "reserve_ai_seat":
                response = await game_service.reserve_ai_seat(
                    table_code=table_code,
                    websocket=websocket,
                    seat_index=int(payload.get("seat_index", -1)),
                )
                await _send_rejection_if_needed(websocket, response)
                continue

            if message_type == "configure_ai_seat":
                response = await game_service.configure_ai_seat(
                    table_code=table_code,
                    websocket=websocket,
                    seat_index=int(payload.get("seat_index", -1)),
                    api_key=str(payload.get("api_key", "")),
                    base_url=str(payload.get("base_url", "")),
                    model=str(payload.get("model", "")),
                )
                await _send_rejection_if_needed(websocket, response)
                continue

            if message_type == "cancel_ai_seat":
                response = await game_service.cancel_ai_seat(
                    table_code=table_code,
                    websocket=websocket,
                    seat_index=int(payload.get("seat_index", -1)),
                )
                await _send_rejection_if_needed(websocket, response)
                continue

            if message_type == "use_default_bot":
                response = await game_service.use_default_bot(
                    table_code=table_code,
                    websocket=websocket,
                    seat_index=int(payload.get("seat_index", -1)),
                )
                await _send_rejection_if_needed(websocket, response)
                continue

            if message_type == "start_match":
                response = await game_service.start_match(
                    table_code=table_code,
                    websocket=websocket,
                )
                await _send_rejection_if_needed(websocket, response)
                continue

            if message_type == "start_next_round":
                response = await game_service.start_next_round(
                    table_code=table_code,
                    websocket=websocket,
                )
                await _send_rejection_if_needed(websocket, response)
                continue

            if message_type == "restart_match":
                response = await game_service.restart_match(
                    table_code=table_code,
                    websocket=websocket,
                )
                await _send_rejection_if_needed(websocket, response)
                continue

            if message_type == "leave_table":
                response = await game_service.leave_table(
                    table_code=table_code,
                    websocket=websocket,
                )
                if response.get("type") == "action_rejected":
                    await websocket.send_json(response)
                    continue
                await websocket.send_json(response)
                await websocket.close()
                return

            if message_type == "action_request":
                await game_service.handle_action_request(table_code, websocket, payload)
                continue

            if message_type == "heartbeat":
                await game_service.handle_heartbeat(websocket, payload)
                continue

            if message_type == "quick_chat":
                await game_service.handle_quick_chat(table_code, websocket, payload)
                continue

            await websocket.send_json(
                {"type": "action_rejected", "payload": {"reason": "unsupported_message"}}
            )
    except WebSocketDisconnect:
        await game_service.disconnect(table_code, websocket)
