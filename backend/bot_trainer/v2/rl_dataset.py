from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import torch
from torch.utils.data import Dataset


class ArenaTrajectoryDataset(Dataset):
    def __init__(self, path: Path) -> None:
        self.rows = [
            json.loads(line)
            for line in path.read_text(encoding="utf-8").splitlines()
            if line.strip()
        ]

    def __len__(self) -> int:
        return len(self.rows)

    def __getitem__(self, index: int) -> dict[str, torch.Tensor]:
        return encode_row(self.rows[index])


def encode_row(row: dict[str, Any]) -> dict[str, torch.Tensor]:
    return {
        "tile_planes": torch.tensor(row["tile_planes"], dtype=torch.float32).view(-1, 34),
        "scalar_features": torch.tensor(row["scalar_features"], dtype=torch.float32),
        "discard_mask": torch.tensor(row["discard_mask"], dtype=torch.bool),
        "claim_mask": torch.tensor(row["claim_mask"], dtype=torch.bool),
        "self_kong_mask": torch.tensor(row["self_kong_mask"], dtype=torch.bool),
        "hu_mask": torch.tensor(row["hu_mask"], dtype=torch.bool),
        "action_index": torch.tensor(row["action_index"], dtype=torch.long),
        "reward": torch.tensor(row["reward"], dtype=torch.float32),
        "done": torch.tensor(row["done"], dtype=torch.bool),
        "old_log_prob": torch.tensor(row["log_prob"], dtype=torch.float32),
        "old_value": torch.tensor(row["value"], dtype=torch.float32),
        "action_head": torch.tensor(action_head_index(row["action_head"]), dtype=torch.long),
    }


def action_head_index(action_head: str) -> int:
    mapping = {"discard": 0, "claim": 1, "self_kong": 2, "hu": 3}
    return mapping[action_head]


def compute_returns(rewards: list[float], dones: list[bool], gamma: float) -> list[float]:
    returns = [0.0 for _ in rewards]
    running = 0.0
    for index in range(len(rewards) - 1, -1, -1):
        if dones[index]:
            running = 0.0
        running = rewards[index] + gamma * running
        returns[index] = round(running, 6)
    return returns
