from __future__ import annotations

import asyncio
import json
from dataclasses import asdict, dataclass
from typing import Any
from urllib import error, request

from app.services.bot_strategy import BotDecision


@dataclass(frozen=True)
class AISeatConfig:
    api_key: str
    base_url: str
    model: str


class OpenAICompatibleAIError(RuntimeError):
    pass


class OpenAICompatibleAIClient:
    async def validate_config(self, config: AISeatConfig) -> None:
        content = await self._chat_completion(
            config,
            messages=[
                {
                    "role": "system",
                    "content": "You are a connection probe. Reply with READY when the request succeeds.",
                },
                {
                    "role": "user",
                    "content": "Reply exactly READY.",
                },
            ],
            temperature=0.0,
            max_tokens=16,
        )
        if "READY" not in content.upper():
            raise OpenAICompatibleAIError("未能通过 AI 接口连通性验证。")

    async def choose_action(
        self,
        *,
        config: AISeatConfig,
        seat_index: int,
        room_mode: str,
        room_snapshot: dict[str, Any],
    ) -> BotDecision:
        content = await self._chat_completion(
            config,
            messages=[
                {
                    "role": "system",
                    "content": (
                        "You are an AI mahjong player for Chinese Official Mahjong. "
                        "Return only compact JSON with keys action_type and tile_ids. "
                        "action_type must be one of the currently available options. "
                        "tile_ids must be an array of exact tile ids from the provided state."
                    ),
                },
                {
                    "role": "user",
                    "content": json.dumps(
                        {
                            "seat_index": seat_index,
                            "room_mode": room_mode,
                            "instruction": (
                                "Choose a legal action for the current pending action. "
                                "If the action does not need tiles, return an empty tile_ids list."
                            ),
                            "room_snapshot": room_snapshot,
                        },
                        ensure_ascii=False,
                    ),
                },
            ],
            temperature=0.1,
            max_tokens=280,
        )
        return self._parse_decision(content)

    async def _chat_completion(
        self,
        config: AISeatConfig,
        *,
        messages: list[dict[str, Any]],
        temperature: float,
        max_tokens: int,
    ) -> str:
        return await asyncio.to_thread(
            self._sync_chat_completion,
            config,
            messages,
            temperature,
            max_tokens,
        )

    def _sync_chat_completion(
        self,
        config: AISeatConfig,
        messages: list[dict[str, Any]],
        temperature: float,
        max_tokens: int,
    ) -> str:
        payload = json.dumps(
            {
                "model": config.model,
                "messages": messages,
                "temperature": temperature,
                "max_tokens": max_tokens,
            }
        ).encode("utf-8")
        endpoint = self._chat_completions_endpoint(config.base_url)
        headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {config.api_key}",
        }
        http_request = request.Request(
            endpoint,
            data=payload,
            headers=headers,
            method="POST",
        )

        try:
            with request.urlopen(http_request, timeout=15) as response:
                body = response.read().decode("utf-8")
        except error.HTTPError as exc:
            detail = exc.read().decode("utf-8", errors="ignore").strip()
            raise OpenAICompatibleAIError(detail or f"AI 接口返回 HTTP {exc.code}") from exc
        except error.URLError as exc:
            raise OpenAICompatibleAIError(f"AI 接口请求失败: {exc.reason}") from exc
        except OSError as exc:
            raise OpenAICompatibleAIError(f"AI 接口不可用: {exc}") from exc

        try:
            payload_json = json.loads(body)
        except json.JSONDecodeError as exc:
            raise OpenAICompatibleAIError("AI 接口返回了无法解析的 JSON。") from exc

        content = self._extract_content(payload_json)
        if not content:
            raise OpenAICompatibleAIError("AI 接口未返回可用内容。")
        return content

    def _chat_completions_endpoint(self, base_url: str) -> str:
        trimmed = base_url.rstrip("/")
        if trimmed.endswith("/chat/completions"):
            return trimmed
        return f"{trimmed}/chat/completions"

    def _extract_content(self, payload: dict[str, Any]) -> str:
        choices = payload.get("choices")
        if not isinstance(choices, list) or not choices:
            return ""
        message = choices[0].get("message", {})
        content = message.get("content")
        if isinstance(content, str):
            return content.strip()
        if isinstance(content, list):
            fragments = []
            for item in content:
                if isinstance(item, dict) and item.get("type") == "text":
                    text = item.get("text")
                    if isinstance(text, str):
                        fragments.append(text)
            return "\n".join(fragments).strip()
        return ""

    def _parse_decision(self, raw_content: str) -> BotDecision:
        normalized = raw_content.strip()
        if normalized.startswith("```"):
            normalized = normalized.strip("`")
            if normalized.startswith("json"):
                normalized = normalized[4:].strip()

        payload: dict[str, Any] | None = None
        try:
            decoded = json.loads(normalized)
            if isinstance(decoded, dict):
                payload = decoded
        except json.JSONDecodeError:
            start = normalized.find("{")
            end = normalized.rfind("}")
            if start != -1 and end != -1 and start < end:
                decoded = json.loads(normalized[start : end + 1])
                if isinstance(decoded, dict):
                    payload = decoded

        if payload is None:
            raise OpenAICompatibleAIError("AI 回复不是合法的 JSON 动作。")

        action_type = payload.get("action_type", payload.get("action"))
        tile_ids = payload.get("tile_ids", [])
        if not isinstance(action_type, str):
            raise OpenAICompatibleAIError("AI 回复缺少 action_type。")
        if not isinstance(tile_ids, list) or not all(isinstance(item, str) for item in tile_ids):
            raise OpenAICompatibleAIError("AI 回复中的 tile_ids 格式不正确。")
        return BotDecision(action_type=action_type.strip(), tile_ids=tile_ids)


def serialize_ai_config(config: AISeatConfig | None) -> dict[str, str] | None:
    return asdict(config) if config is not None else None


def deserialize_ai_config(payload: dict[str, Any] | None) -> AISeatConfig | None:
    if payload is None:
        return None
    api_key = payload.get("api_key")
    base_url = payload.get("base_url")
    model = payload.get("model")
    if not all(isinstance(item, str) and item.strip() for item in (api_key, base_url, model)):
        return None
    return AISeatConfig(
        api_key=api_key.strip(),
        base_url=base_url.strip(),
        model=model.strip(),
    )
