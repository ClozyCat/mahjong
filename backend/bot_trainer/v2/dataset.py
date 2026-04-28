from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import numpy as np
from tqdm import tqdm

try:
    import torch
    from torch.utils.data import Dataset
except ModuleNotFoundError:
    torch = None
    Dataset = object

TILE_KIND_COUNT = 34
TILE_PLANE_COUNT = 10
SCALAR_FEATURE_COUNT = 10
IGNORE_INDEX = -100

class MissingTorchError(RuntimeError):
    pass

class MahjongDecisionDataset(Dataset):
    def __init__(self, jsonl_path: Path, metadata_path: Path) -> None:
        if torch is None:
            raise MissingTorchError("PyTorch is required: pip install torch")
        self.jsonl_path = jsonl_path
        self.metadata = load_metadata(metadata_path)

        if not jsonl_path.exists():
            print(f"Warning: Dataset file not found at {jsonl_path}")
            self.num_samples = 0
            return

        # 1. 极速扫描获取总行数，用于精确预分配内存
        print(f"Scanning {jsonl_path.name} to allocate continuous memory...")
        with jsonl_path.open("rb") as f:
            self.num_samples = sum(1 for _ in f)

        print(f"Pre-allocating giant tensors for {self.num_samples} samples...")
        
        # 2. 预分配连续内存 (消除 Python Dict/List 的海量内存开销)
        self.tile_planes = torch.zeros((self.num_samples, TILE_PLANE_COUNT, TILE_KIND_COUNT), dtype=torch.float32)
        self.scalar_features = torch.zeros((self.num_samples, SCALAR_FEATURE_COUNT), dtype=torch.float32)
        self.discard_mask = torch.zeros((self.num_samples, TILE_KIND_COUNT), dtype=torch.bool)
        self.claim_mask = torch.zeros((self.num_samples, len(self.metadata["claim_actions"])), dtype=torch.bool)
        self.self_kong_mask = torch.zeros((self.num_samples, len(self.metadata["self_kong_actions"])), dtype=torch.bool)
        self.hu_mask = torch.zeros((self.num_samples, 2), dtype=torch.bool)
        self.discard_target = torch.zeros((self.num_samples,), dtype=torch.int64)
        self.claim_target = torch.zeros((self.num_samples,), dtype=torch.int64)
        self.self_kong_target = torch.zeros((self.num_samples,), dtype=torch.int64)
        self.hu_target = torch.zeros((self.num_samples,), dtype=torch.int64)
        self.value_target = torch.zeros((self.num_samples, 1), dtype=torch.float32)
        self.risk_target = torch.zeros((self.num_samples, TILE_KIND_COUNT), dtype=torch.float32)
        self.decision_kind = torch.zeros((self.num_samples,), dtype=torch.int64)

        # 3. 填充数据
        print("Filling tensors...")
        with jsonl_path.open("r", encoding="utf-8") as f:
            for i, line in enumerate(tqdm(f, desc=f"Loading {jsonl_path.name}", total=self.num_samples)):
                line = line.strip()
                if not line:
                    continue
                encoded = encode_row(json.loads(line), self.metadata)
                
                # 直接填入大张量，不需要再创建任何中转对象
                self.tile_planes[i] = torch.from_numpy(encoded["tile_planes"])
                self.scalar_features[i] = torch.from_numpy(encoded["scalar_features"])
                self.discard_mask[i] = torch.from_numpy(encoded["discard_mask"])
                self.claim_mask[i] = torch.from_numpy(encoded["claim_mask"])
                self.self_kong_mask[i] = torch.from_numpy(encoded["self_kong_mask"])
                self.hu_mask[i] = torch.from_numpy(encoded["hu_mask"])
                self.discard_target[i] = int(encoded["discard_target"])
                self.claim_target[i] = int(encoded["claim_target"])
                self.self_kong_target[i] = int(encoded["self_kong_target"])
                self.hu_target[i] = int(encoded["hu_target"])
                self.value_target[i] = torch.from_numpy(encoded["value_target"])
                self.risk_target[i] = torch.from_numpy(encoded["risk_target"])
                self.decision_kind[i] = int(encoded["decision_kind"])

    def __len__(self) -> int:
        return getattr(self, "num_samples", 0)

    def __getitem__(self, index: int) -> int:
        # [核心优化] 不再返回字典，而是仅仅返回索引！
        return index

    def get_batch(self, indices: list[int]) -> dict[str, torch.Tensor]:
        # [核心优化] C++ 级别的极速内存切片，完全绕过低效的 collate_fn 拼装
        return {
            "tile_planes": self.tile_planes[indices],
            "scalar_features": self.scalar_features[indices],
            "discard_mask": self.discard_mask[indices],
            "claim_mask": self.claim_mask[indices],
            "self_kong_mask": self.self_kong_mask[indices],
            "hu_mask": self.hu_mask[indices],
            "discard_target": self.discard_target[indices],
            "claim_target": self.claim_target[indices],
            "self_kong_target": self.self_kong_target[indices],
            "hu_target": self.hu_target[indices],
            "value_target": self.value_target[indices],
            "risk_target": self.risk_target[indices],
            "decision_kind": self.decision_kind[indices],
        }


def load_metadata(metadata_path: Path) -> dict[str, Any]:
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    if metadata.get("schema_version") != 2:
        raise ValueError(f"unsupported metadata schema: {metadata.get('schema_version')}")
    if len(metadata["tile_keys"]) != TILE_KIND_COUNT:
        raise ValueError("metadata tile_keys must contain 34 entries")
    return metadata


def load_jsonl(jsonl_path: Path) -> list[dict[str, Any]]:
    if not jsonl_path.exists():
        return []
    return [
        json.loads(line)
        for line in jsonl_path.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]


def build_jsonl_offsets(jsonl_path: Path) -> list[int]:
    offsets: list[int] = []
    if not jsonl_path.exists():
        return offsets
    with jsonl_path.open("rb") as handle:
        while True:
            offset = handle.tell()
            line = handle.readline()
            if not line:
                break
            if line.strip():
                offsets.append(offset)
    return offsets


def encode_row(row: dict[str, Any], metadata: dict[str, Any]) -> dict[str, np.ndarray]:
    context = row["context"]
    tile_to_index = {tile_key: index for index, tile_key in enumerate(metadata["tile_keys"])}
    claim_to_index = {action: index for index, action in enumerate(metadata["claim_actions"])}
    self_kong_to_index = {
        action: index for index, action in enumerate(metadata["self_kong_actions"])
    }

    return {
        "tile_planes": encode_tile_planes(context, tile_to_index),
        "scalar_features": encode_scalar_features(context),
        "discard_mask": encode_discard_mask(row, tile_to_index),
        "claim_mask": encode_claim_mask(row, claim_to_index),
        "self_kong_mask": encode_self_kong_mask(row, self_kong_to_index),
        "hu_mask": encode_hu_mask(row),
        "discard_target": np.asarray(discard_target(row, tile_to_index), dtype=np.int64),
        "claim_target": np.asarray(claim_target(row, claim_to_index), dtype=np.int64),
        "self_kong_target": np.asarray(self_kong_target(row, self_kong_to_index), dtype=np.int64),
        "hu_target": np.asarray(hu_target(row), dtype=np.int64),
        "value_target": np.asarray([float(row["outcome"]["score_delta"]) / 100.0], dtype=np.float32),
        "risk_target": risk_target(row, tile_to_index),
        "decision_kind": np.asarray(decision_kind_index(row["decision_kind"]), dtype=np.int64),
    }


def encode_tile_planes(context: dict[str, Any], tile_to_index: dict[str, int]) -> np.ndarray:
    planes = np.zeros((TILE_PLANE_COUNT, TILE_KIND_COUNT), dtype=np.float32)

    add_count_plane(
        planes,
        0,
        [
            tile["tile_key"]
            for tile in context["player"]["concealed_tiles"]
            if not tile.get("is_flower", False)
        ],
        tile_to_index,
    )
    add_count_plane(planes, 1, flatten(context["player"].get("meld_tile_key_groups", [])), tile_to_index)
    add_count_plane(planes, 2, context.get("visible_tile_keys", []), tile_to_index)

    seat_index = int(context["seat_index"])
    seat_count = max(1, int(context.get("seat_count", 4)))
    discards_by_seat = context.get("opponent_discards_by_seat", [])
    melds_by_seat = context.get("opponent_melds_by_seat", [])
    for offset in range(1, 4):
        seat = (seat_index + offset) % seat_count
        if seat < len(discards_by_seat):
            add_count_plane(planes, 2 + offset * 2 - 1, discards_by_seat[seat], tile_to_index)
        if seat < len(melds_by_seat):
            add_count_plane(planes, 2 + offset * 2, flatten(melds_by_seat[seat]), tile_to_index)

    last_discard = context.get("last_discard_tile_key")
    if last_discard in tile_to_index:
        planes[9, tile_to_index[last_discard]] = 1.0
    return planes


def encode_scalar_features(context: dict[str, Any]) -> np.ndarray:
    features = np.zeros((SCALAR_FEATURE_COUNT,), dtype=np.float32)
    seat_index = int(context["seat_index"])
    features[0] = seat_index / 3.0
    features[1] = int(context.get("dealer_seat", 0)) / 3.0
    features[2] = max(0, int(context.get("wall_tiles_remaining", 0))) / 84.0
    features[3] = len(context["player"].get("meld_tile_key_groups", [])) / 4.0
    features[4] = int(context["player"].get("flower_count", 0)) / 8.0
    features[5] = 1.0 if context.get("restricted_discard_tile_key") is not None else 0.0
    features[6] = 1.0 if context.get("drawn_tile_id") is not None else 0.0
    features[7] = len(context.get("self_kong_candidates", [])) / 4.0
    features[8] = len(context.get("claim_options", [])) / 4.0
    scores = context.get("cumulative_scores", [])
    features[9] = float(scores[seat_index]) / 100.0 if seat_index < len(scores) else 0.0
    return features


def encode_discard_mask(row: dict[str, Any], tile_to_index: dict[str, int]) -> np.ndarray:
    mask = np.zeros((TILE_KIND_COUNT,), dtype=np.bool_)
    for action in row.get("legal_actions", []):
        if action.startswith("discard:"):
            tile_key = action.split(":", 1)[1]
            if tile_key in tile_to_index:
                mask[tile_to_index[tile_key]] = True
    return mask


def encode_claim_mask(row: dict[str, Any], claim_to_index: dict[str, int]) -> np.ndarray:
    mask = np.zeros((len(claim_to_index),), dtype=np.bool_)
    for action in row.get("legal_actions", []):
        if action == "pass":
            mask[claim_to_index["pass"]] = True
        elif action.startswith("claim:"):
            claim_name = claim_name_from_action_id(action)
            if claim_name in claim_to_index:
                mask[claim_to_index[claim_name]] = True
    return mask


def encode_self_kong_mask(row: dict[str, Any], self_kong_to_index: dict[str, int]) -> np.ndarray:
    mask = np.zeros((len(self_kong_to_index),), dtype=np.bool_)
    for action in row.get("legal_actions", []):
        if action == "pass":
            mask[self_kong_to_index["pass"]] = True
        elif action.startswith("self_kong:"):
            kind = action.split(":", 2)[1]
            if kind in self_kong_to_index:
                mask[self_kong_to_index[kind]] = True
    return mask


def encode_hu_mask(row: dict[str, Any]) -> np.ndarray:
    legal_actions = row.get("legal_actions", [])
    return np.asarray([True, any(action == "claim:hu" or action == "hu" for action in legal_actions)])


def discard_target(row: dict[str, Any], tile_to_index: dict[str, int]) -> int:
    label = row["label"]
    if label["type"] != "discard":
        return IGNORE_INDEX
    return tile_to_index[label["tile_key"]]


def claim_target(row: dict[str, Any], claim_to_index: dict[str, int]) -> int:
    label_type = row["label"]["type"]
    if label_type == "pass":
        return claim_to_index["pass"]
    if label_type == "hu":
        return claim_to_index["hu"]
    if label_type == "claim_pung":
        return claim_to_index["pung"]
    if label_type == "claim_kong":
        return claim_to_index["kong"]
    if label_type == "claim_chow":
        return claim_to_index["chow_mid"]
    return IGNORE_INDEX


def self_kong_target(row: dict[str, Any], self_kong_to_index: dict[str, int]) -> int:
    label = row["label"]
    if label["type"] != "self_kong":
        return IGNORE_INDEX
    return self_kong_to_index[label["kind"]]


def hu_target(row: dict[str, Any]) -> int:
    label_type = row["label"]["type"]
    if label_type == "hu":
        return 1
    if row["decision_kind"] in {"claim_window", "rob_kong"}:
        return 0
    return IGNORE_INDEX


def risk_target(row: dict[str, Any], tile_to_index: dict[str, int]) -> np.ndarray:
    target = np.zeros((TILE_KIND_COUNT,), dtype=np.float32)
    if row["outcome"].get("dealt_in", False):
        label = row["label"]
        if label.get("type") == "discard" and label.get("tile_key") in tile_to_index:
            target[tile_to_index[label["tile_key"]]] = 1.0
    return target


def decision_kind_index(decision_kind: str) -> int:
    return {"active_turn": 0, "claim_window": 1, "rob_kong": 2}.get(decision_kind, -1)


def claim_name_from_action_id(action: str) -> str:
    parts = action.split(":")
    if len(parts) < 2:
        return action
    return "chow_mid" if parts[1] == "chow" else parts[1]


def add_count_plane(
    planes: np.ndarray,
    plane: int,
    tile_keys: list[str],
    tile_to_index: dict[str, int],
) -> None:
    for tile_key in tile_keys:
        index = tile_to_index.get(tile_key)
        if index is not None:
            planes[plane, index] = min(4.0, planes[plane, index] + 1.0)


def flatten(groups: list[list[str]]) -> list[str]:
    return [tile_key for group in groups for tile_key in group]
