from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import torch
from torch.utils.data import Dataset

from awr_dataset import DISCARD_EVENT_FEATURE_COUNT, DISCARD_SEQUENCE_LENGTH

TILE_KIND_COUNT = 34


class CounterfactualDiscardDataset(Dataset):
    def __init__(self, path: Path, policy_id: str | None = None) -> None:
        self.rows = []
        for line in path.read_text(encoding="utf-8-sig").splitlines():
            if not line.strip():
                continue
            row = json.loads(line)
            if policy_id is not None and row.get("policy_id") != policy_id:
                continue
            self.rows.append(row)

    def __len__(self) -> int:
        return len(self.rows)

    def __getitem__(self, index: int) -> dict[str, torch.Tensor]:
        return encode_counterfactual_row(self.rows[index])


def encode_counterfactual_row(row: dict[str, Any]) -> dict[str, torch.Tensor]:
    legal_discards = [int(index) for index in row["legal_discards"]]
    raw_scores = [float(score) for score in row["teacher_scores"]]
    if len(legal_discards) != len(raw_scores):
        raise ValueError("legal_discards and teacher_scores must have the same length")

    legal_mask = torch.zeros(TILE_KIND_COUNT, dtype=torch.bool)
    teacher_scores = torch.full((TILE_KIND_COUNT,), float("-inf"), dtype=torch.float32)
    for tile_index, score in zip(legal_discards, raw_scores, strict=True):
        legal_mask[tile_index] = True
        teacher_scores[tile_index] = score

    return {
        "tile_planes": torch.tensor(row["tile_planes"], dtype=torch.float32).view(-1, 34),
        "scalar_features": torch.tensor(row["scalar_features"], dtype=torch.float32),
        "discard_sequence": torch.tensor(
            row["discard_sequence"],
            dtype=torch.float32,
        ).view(DISCARD_SEQUENCE_LENGTH, DISCARD_EVENT_FEATURE_COUNT),
        "discard_mask": torch.tensor(row["discard_mask"], dtype=torch.bool),
        "legal_mask": legal_mask,
        "teacher_scores": teacher_scores,
        "teacher_best_index": torch.tensor(int(row["teacher_best_index"]), dtype=torch.long),
    }
