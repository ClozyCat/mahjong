from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import torch
from torch.utils.data import Dataset

DISCARD_SEQUENCE_LENGTH = 32
DISCARD_EVENT_FEATURE_COUNT = 40


class ArenaTrajectoryDataset(Dataset):
    """Loads arena trajectory JSONL, computes MC returns, optionally filters by policy_id."""

    def __init__(
        self,
        path: Path,
        gamma: float = 0.995,
        policy_id: str | None = None,
    ) -> None:
        rows = [
            json.loads(line)
            for line in path.read_text(encoding="utf-8-sig").splitlines()
            if line.strip()
        ]
        if policy_id is not None:
            rows = [row for row in rows if row.get("policy_id") == policy_id]
        self.rows = rows
        self.returns = compute_discounted_returns_for_rows(self.rows, gamma=gamma)

    def __len__(self) -> int:
        return len(self.rows)

    def __getitem__(self, index: int) -> dict[str, torch.Tensor]:
        row = dict(self.rows[index])
        row["return"] = self.returns[index]
        return encode_row(row, self.returns[index])


def encode_row(row: dict[str, Any], discounted_return: float) -> dict[str, torch.Tensor]:
    result = {
        "tile_planes": torch.tensor(row["tile_planes"], dtype=torch.float32).view(-1, 34),
        "scalar_features": torch.tensor(row["scalar_features"], dtype=torch.float32),
        "discard_sequence": torch.tensor(
            row["discard_sequence"],
            dtype=torch.float32,
        ).view(DISCARD_SEQUENCE_LENGTH, DISCARD_EVENT_FEATURE_COUNT),
        "discard_mask": torch.tensor(row["discard_mask"], dtype=torch.bool),
        "claim_mask": torch.tensor(row["claim_mask"], dtype=torch.bool),
        "self_kong_mask": torch.tensor(row["self_kong_mask"], dtype=torch.bool),
        "hu_mask": torch.tensor(row["hu_mask"], dtype=torch.bool),
        "action_index": torch.tensor(row["action_index"], dtype=torch.long),
        "reward": torch.tensor(row["reward"], dtype=torch.float32),
        "return": torch.tensor(discounted_return, dtype=torch.float32),
        "advantage": torch.tensor(
            row.get("advantage", discounted_return - float(row.get("value", 0.0))),
            dtype=torch.float32,
        ),
        "step_reward": torch.tensor(row.get("step_reward", 0.0), dtype=torch.float32),
        "terminal_reward": torch.tensor(row.get("terminal_reward", 0.0), dtype=torch.float32),
        "risk_probs": torch.tensor(row["risk_probs"], dtype=torch.float32),
        "opponent_tenpai_target": torch.tensor(
            row["opponent_tenpai_target"], dtype=torch.float32
        ),
        "opponent_risk_target": torch.tensor(
            row["opponent_risk_target"], dtype=torch.float32
        ),
        "opponent_risk_mask": torch.tensor(
            row["opponent_risk_mask"], dtype=torch.bool
        ),
        "done": torch.tensor(row["done"], dtype=torch.bool),
        "log_prob": torch.tensor(row["log_prob"], dtype=torch.float32),
        "action_head": torch.tensor(action_head_index(row["action_head"]), dtype=torch.long),
    }
    return result


def action_head_index(action_head: str) -> int:
    mapping = {"discard": 0, "claim": 1, "self_kong": 2, "hu": 3}
    return mapping[action_head]


def compute_discounted_returns_for_rows(
    rows: list[dict[str, Any]],
    gamma: float,
) -> list[float]:
    returns = [0.0 for _ in rows]
    groups: dict[tuple[str, int], list[int]] = {}
    for index, row in enumerate(rows):
        key = (str(row["match_id"]), int(row["seat_index"]))
        groups.setdefault(key, []).append(index)

    for indices in groups.values():
        running = 0.0
        for index in reversed(indices):
            running = float(rows[index]["reward"]) + gamma * running
            returns[index] = round(running, 6)
    return returns


def compute_normalized_advantages(
    rows: list[dict[str, Any]],
    returns: list[float],
    values: list[float],
    mode: str = "per_match",
) -> list[float]:
    """
    Compute normalized advantages from returns and values.

    mode:
      "none"       — raw advantage = return - value, clipped to [-5, 5]
      "per_match"  — z-score normalize within each match_id group
      "per_seat"   — z-score normalize within each (match_id, seat_index) group
      "batch"      — z-score normalize across entire dataset
    """
    n = len(rows)
    raw = [min(max(returns[i] - values[i], -5.0), 5.0) for i in range(n)]

    if mode == "none":
        return raw

    if mode == "batch":
        mean = sum(raw) / n
        var = sum((x - mean) ** 2 for x in raw) / n
        std = (var + 1e-8) ** 0.5
        return [(x - mean) / std for x in raw]

    groups: dict[tuple[str, ...], list[int]] = {}
    for i, row in enumerate(rows):
        if mode == "per_match":
            key = (str(row["match_id"]),)
        elif mode == "per_seat":
            key = (str(row["match_id"]), str(row["seat_index"]))
        else:
            key = ("_global",)
        groups.setdefault(key, []).append(i)

    result = [0.0] * n
    for indices in groups.values():
        group_raw = [raw[i] for i in indices]
        g_n = len(group_raw)
        if g_n < 2:
            for i in indices:
                result[i] = raw[i]
            continue
        mean = sum(group_raw) / g_n
        var = sum((x - mean) ** 2 for x in group_raw) / g_n
        std = (var + 1e-8) ** 0.5
        for i in indices:
            result[i] = min(max((raw[i] - mean) / std, -5.0), 5.0)
    return result


def trajectory_diagnostics(rows: list[dict[str, Any]]) -> dict[str, float | int]:
    diagnostics: dict[str, float | int] = {
        "row_count": len(rows),
        "terminal_reward_mean": terminal_mean_value(rows),
        "step_reward_mean": mean_value(rows, "step_reward"),
    }
    for action_head in ("discard", "claim", "self_kong", "hu"):
        diagnostics[f"action_head_{action_head}"] = sum(
            1 for row in rows if row.get("action_head") == action_head
        )
    return diagnostics


def mean_value(rows: list[dict[str, Any]], key: str) -> float:
    if not rows:
        return 0.0
    return sum(float(row.get(key, 0.0) or 0.0) for row in rows) / len(rows)


def terminal_mean_value(rows: list[dict[str, Any]]) -> float:
    if not rows:
        return 0.0
    return sum(terminal_reward(row) for row in rows) / len(rows)


def terminal_reward(row: dict[str, Any]) -> float:
    if not bool(row.get("done")):
        return 0.0
    return float(row.get("terminal_reward", 0.0) or 0.0)
