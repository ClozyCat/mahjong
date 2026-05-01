from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import torch
from torch.utils.data import Dataset


class ArenaTrajectoryDataset(Dataset):
    def __init__(
        self,
        path: Path,
        gamma: float = 0.99,
        gae_lambda: float = 0.95,
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
        self.advantages, self.returns = compute_gae_for_rows(
            self.rows,
            gamma=gamma,
            gae_lambda=gae_lambda,
        )

    def __len__(self) -> int:
        return len(self.rows)

    def __getitem__(self, index: int) -> dict[str, torch.Tensor]:
        row = dict(self.rows[index])
        row["advantage"] = self.advantages[index]
        return encode_row(row, self.returns[index])


def encode_row(row: dict[str, Any], discounted_return: float | None = None) -> dict[str, torch.Tensor]:
    return {
        "tile_planes": torch.tensor(row["tile_planes"], dtype=torch.float32).view(-1, 34),
        "scalar_features": torch.tensor(row["scalar_features"], dtype=torch.float32),
        "discard_sequence": torch.tensor(
            row.get("discard_sequence", [[0.0] * 38 for _ in range(64)]),
            dtype=torch.float32,
        ).view(64, 38),
        "discard_mask": torch.tensor(row["discard_mask"], dtype=torch.bool),
        "claim_mask": torch.tensor(row["claim_mask"], dtype=torch.bool),
        "self_kong_mask": torch.tensor(row["self_kong_mask"], dtype=torch.bool),
        "hu_mask": torch.tensor(row["hu_mask"], dtype=torch.bool),
        "action_index": torch.tensor(row["action_index"], dtype=torch.long),
        "reward": torch.tensor(row["reward"], dtype=torch.float32),
        "return": torch.tensor(
            row["reward"] if discounted_return is None else discounted_return,
            dtype=torch.float32,
        ),
        "advantage": torch.tensor(row.get("advantage", 0.0), dtype=torch.float32),
        "step_reward": torch.tensor(row.get("step_reward", 0.0), dtype=torch.float32),
        "terminal_reward": torch.tensor(row.get("terminal_reward", 0.0), dtype=torch.float32),
        "shanten_before": optional_int_tensor(row.get("shanten_before")),
        "shanten_after": optional_int_tensor(row.get("shanten_after")),
        "fan_potential_before": optional_int_tensor(row.get("fan_potential_before")),
        "fan_potential_after": optional_int_tensor(row.get("fan_potential_after")),
        "done": torch.tensor(row["done"], dtype=torch.bool),
        "old_log_prob": torch.tensor(row["log_prob"], dtype=torch.float32),
        "old_value": torch.tensor(row["value"], dtype=torch.float32),
        "action_head": torch.tensor(action_head_index(row["action_head"]), dtype=torch.long),
        "has_global_state": torch.tensor(
            row.get("global_tile_planes") is not None
            and row.get("global_scalar_features") is not None,
            dtype=torch.bool,
        ),
    }


def action_head_index(action_head: str) -> int:
    mapping = {"discard": 0, "claim": 1, "self_kong": 2, "hu": 3}
    return mapping[action_head]


def optional_int_tensor(value: Any) -> torch.Tensor:
    return torch.tensor(-1 if value is None else int(value), dtype=torch.long)


def compute_returns(rewards: list[float], dones: list[bool], gamma: float) -> list[float]:
    returns = [0.0 for _ in rewards]
    running = 0.0
    for index in range(len(rewards) - 1, -1, -1):
        if dones[index]:
            running = 0.0
        running = rewards[index] + gamma * running
        returns[index] = round(running, 6)
    return returns


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


def compute_gae_for_rows(
    rows: list[dict[str, Any]],
    gamma: float,
    gae_lambda: float,
) -> tuple[list[float], list[float]]:
    advantages = [0.0 for _ in rows]
    returns = [0.0 for _ in rows]
    groups: dict[tuple[str, int], list[int]] = {}
    for index, row in enumerate(rows):
        key = (str(row["match_id"]), int(row["seat_index"]))
        groups.setdefault(key, []).append(index)

    for indices in groups.values():
        running_advantage = 0.0
        next_value = 0.0
        for index in reversed(indices):
            reward = float(rows[index]["reward"])
            value = float(rows[index].get("value", 0.0))
            delta = reward + gamma * next_value - value
            running_advantage = delta + gamma * gae_lambda * running_advantage
            advantages[index] = round(running_advantage, 6)
            returns[index] = round(value + running_advantage, 6)
            next_value = value
    return advantages, returns
