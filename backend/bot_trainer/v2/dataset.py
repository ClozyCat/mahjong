from __future__ import annotations
import json
from pathlib import Path
from typing import Any, Sequence

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
SCALAR_FEATURE_COUNT = 12
DISCARD_SEQUENCE_LENGTH = 32
DISCARD_EVENT_FEATURE_COUNT = 40
IGNORE_INDEX = -100
DISK_CACHE_VERSION = 7
STANDARD_WIND_ORDER = ("east", "south", "west", "north")
ROUND_WIND_TO_INDEX = {"east": 0.0, "south": 1.0, "west": 2.0, "north": 3.0}

class MissingTorchError(RuntimeError):
    pass

class MahjongDecisionDataset(Dataset):
    def __init__(
        self,
        jsonl_path: Path,
        metadata_path: Path,
        cache_dir: Path | None = None,
        rebuild_cache: bool = False,
    ) -> None:
        if torch is None:
            raise MissingTorchError("PyTorch is required: pip install torch")
        self.jsonl_path = jsonl_path
        self.metadata_path = metadata_path
        self.metadata = load_metadata(metadata_path)
        self.lookups = build_encoder_lookups(self.metadata)
        self.cache_dir = resolve_cache_dir(jsonl_path, cache_dir)
        self._arrays: dict[str, np.ndarray] = {}

        if not jsonl_path.exists():
            print(f"Warning: Dataset file not found at {jsonl_path}")
            self.num_samples = 0
            return

        self._load_or_build_disk_cache(rebuild_cache)

    def _load_or_build_disk_cache(self, rebuild_cache: bool) -> None:
        manifest = read_json(self.cache_dir / "manifest.json")
        if rebuild_cache or not cache_is_current(manifest, self):
            self._build_disk_cache()

        try:
            self._open_disk_cache(announce=True)
        except (OSError, ValueError, KeyError) as exc:
            print(f"Disk cache is invalid ({exc}); rebuilding {self.cache_dir.name}...")
            self._build_disk_cache()
            self._open_disk_cache(announce=True)

    def _build_disk_cache(self) -> None:
        self.cache_dir.mkdir(parents=True, exist_ok=True)
        manifest_path = self.cache_dir / "manifest.json"
        if manifest_path.exists():
            manifest_path.unlink()
        self.num_samples = count_jsonl_rows(self.jsonl_path)
        print(
            f"Building disk-backed tensor cache for {self.num_samples} samples: "
            f"{self.cache_dir}"
        )

        specs = tensor_array_specs(self.metadata)
        arrays = {
            name: np.lib.format.open_memmap(
                array_path(self.cache_dir, name),
                mode="w+",
                dtype=dtype,
                shape=(self.num_samples, *shape),
            )
            for name, (shape, dtype) in specs.items()
        }

        row_index = 0
        with self.jsonl_path.open("r", encoding="utf-8") as handle:
            with tqdm(total=self.num_samples, desc=f"Caching {self.jsonl_path.name}") as pbar:
                for line in handle:
                    line = line.strip()
                    if not line:
                        continue
                    encoded = encode_row(json.loads(line), self.metadata, self.lookups)
                    for name, mmap_array in arrays.items():
                        mmap_array[row_index] = encoded[name]
                    row_index += 1
                    pbar.update(1)

        if row_index != self.num_samples:
            raise ValueError(f"expected {self.num_samples} rows, cached {row_index}")

        for mmap_array in arrays.values():
            mmap_array.flush()

        manifest = expected_cache_manifest(self, self.num_samples)
        manifest_path.write_text(
            json.dumps(manifest, indent=2, ensure_ascii=False),
            encoding="utf-8",
        )

    def _open_disk_cache(self, announce: bool) -> None:
        manifest = read_json(self.cache_dir / "manifest.json")
        if manifest is None:
            raise ValueError("missing manifest")

        self.num_samples = int(manifest["num_samples"])
        specs = tensor_array_specs(self.metadata)
        arrays: dict[str, np.ndarray] = {}
        for name, (shape, dtype) in specs.items():
            mmap_array = np.load(array_path(self.cache_dir, name), mmap_mode="r")
            expected_shape = (self.num_samples, *shape)
            if mmap_array.shape != expected_shape:
                raise ValueError(f"{name} shape {mmap_array.shape} != {expected_shape}")
            if mmap_array.dtype != dtype:
                raise ValueError(f"{name} dtype {mmap_array.dtype} != {dtype}")
            arrays[name] = mmap_array
        self._arrays = arrays
        if announce:
            print(f"Using disk-backed tensor cache: {self.cache_dir}")

    def __getstate__(self) -> dict[str, Any]:
        state = self.__dict__.copy()
        # DataLoader worker spawn 会 pickle Dataset；不要把 mmap 内容序列化进子进程。
        state["_arrays"] = {}
        return state

    def __setstate__(self, state: dict[str, Any]) -> None:
        self.__dict__.update(state)
        if getattr(self, "num_samples", 0) > 0:
            self._open_disk_cache(announce=False)

    def __len__(self) -> int:
        return getattr(self, "num_samples", 0)

    def __getitem__(self, index: int) -> int:
        # [核心优化] 不再返回字典，而是仅仅返回索引！
        return index

    def get_batch(self, indices: Sequence[int]) -> dict[str, torch.Tensor]:
        # 每个 batch 只从磁盘映射缓存读取当前索引，避免整份数据常驻内存。
        index_array = np.asarray(list(indices), dtype=np.int64)
        batch = {
            name: torch.from_numpy(np.asarray(mmap_array[index_array]))
            for name, mmap_array in self._arrays.items()
        }
        return batch


def resolve_cache_dir(jsonl_path: Path, cache_dir: Path | None) -> Path:
    root = cache_dir if cache_dir is not None else jsonl_path.parent / ".tensor_cache"
    return root / jsonl_path.stem


def tensor_array_specs(metadata: dict[str, Any]) -> dict[str, tuple[tuple[int, ...], np.dtype]]:
    return {
        "tile_planes": ((TILE_PLANE_COUNT, TILE_KIND_COUNT), np.dtype(np.float32)),
        "scalar_features": ((SCALAR_FEATURE_COUNT,), np.dtype(np.float32)),
        "discard_sequence": (
            (DISCARD_SEQUENCE_LENGTH, DISCARD_EVENT_FEATURE_COUNT),
            np.dtype(np.float32),
        ),
        "discard_mask": ((TILE_KIND_COUNT,), np.dtype(np.bool_)),
        "claim_mask": ((len(metadata["claim_actions"]),), np.dtype(np.bool_)),
        "self_kong_mask": ((len(metadata["self_kong_actions"]),), np.dtype(np.bool_)),
        "hu_mask": ((2,), np.dtype(np.bool_)),
        "discard_target": ((), np.dtype(np.int64)),
        "claim_target": ((), np.dtype(np.int64)),
        "self_kong_target": ((), np.dtype(np.int64)),
        "hu_target": ((), np.dtype(np.int64)),
        "value_target": ((1,), np.dtype(np.float32)),
        "risk_target": ((TILE_KIND_COUNT,), np.dtype(np.float32)),
        "fan_target": ((1,), np.dtype(np.float32)),
        "decision_kind": ((), np.dtype(np.int64)),
    }


def array_path(cache_dir: Path, name: str) -> Path:
    return cache_dir / f"{name}.npy"


def read_json(path: Path) -> dict[str, Any] | None:
    if not path.exists():
        return None
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None


def file_signature(path: Path) -> dict[str, int]:
    stat = path.stat()
    return {"size": stat.st_size, "mtime_ns": stat.st_mtime_ns}


def expected_cache_manifest(dataset: MahjongDecisionDataset, num_samples: int | None = None) -> dict[str, Any]:
    manifest: dict[str, Any] = {
        "version": DISK_CACHE_VERSION,
        "jsonl": file_signature(dataset.jsonl_path),
        "metadata": file_signature(dataset.metadata_path),
        "schema_version": dataset.metadata.get("schema_version"),
        "tile_kind_count": TILE_KIND_COUNT,
        "tile_plane_count": TILE_PLANE_COUNT,
        "scalar_feature_count": SCALAR_FEATURE_COUNT,
        "discard_sequence_length": DISCARD_SEQUENCE_LENGTH,
        "discard_event_feature_count": DISCARD_EVENT_FEATURE_COUNT,
        "claim_action_count": len(dataset.metadata["claim_actions"]),
        "self_kong_action_count": len(dataset.metadata["self_kong_actions"]),
    }
    if num_samples is not None:
        manifest["num_samples"] = num_samples
    return manifest


def cache_is_current(manifest: dict[str, Any] | None, dataset: MahjongDecisionDataset) -> bool:
    if manifest is None or not isinstance(manifest.get("num_samples"), int):
        return False

    expected = expected_cache_manifest(dataset)
    for key, expected_value in expected.items():
        if manifest.get(key) != expected_value:
            return False

    for name in tensor_array_specs(dataset.metadata):
        if not array_path(dataset.cache_dir, name).exists():
            return False
    return True


def count_jsonl_rows(jsonl_path: Path) -> int:
    with jsonl_path.open("rb") as handle:
        return sum(1 for line in handle if line.strip())


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


def build_encoder_lookups(metadata: dict[str, Any]) -> dict[str, dict[str, int]]:
    return {
        "tile_to_index": {tile_key: index for index, tile_key in enumerate(metadata["tile_keys"])},
        "claim_to_index": {
            action: index for index, action in enumerate(metadata["claim_actions"])
        },
        "self_kong_to_index": {
            action: index for index, action in enumerate(metadata["self_kong_actions"])
        },
    }


def encode_row(
    row: dict[str, Any],
    metadata: dict[str, Any],
    lookups: dict[str, dict[str, int]] | None = None,
) -> dict[str, np.ndarray]:
    context = row["context"]
    lookups = lookups or build_encoder_lookups(metadata)
    tile_to_index = lookups["tile_to_index"]
    claim_to_index = lookups["claim_to_index"]
    self_kong_to_index = lookups["self_kong_to_index"]

    return {
        "tile_planes": encode_tile_planes(context, tile_to_index),
        "scalar_features": encode_scalar_features(context),
        "discard_sequence": encode_discard_sequence(context, tile_to_index),
        "discard_mask": encode_discard_mask(row, tile_to_index),
        "claim_mask": encode_claim_mask(row, claim_to_index, tile_to_index),
        "self_kong_mask": encode_self_kong_mask(row, self_kong_to_index),
        "hu_mask": encode_hu_mask(row),
        "discard_target": np.asarray(discard_target(row, tile_to_index), dtype=np.int64),
        "claim_target": np.asarray(claim_target(row, claim_to_index, tile_to_index), dtype=np.int64),
        "self_kong_target": np.asarray(self_kong_target(row, self_kong_to_index), dtype=np.int64),
        "hu_target": np.asarray(hu_target(row), dtype=np.int64),
        "value_target": np.asarray([float(row["outcome"]["score_delta"]) / 1000.0], dtype=np.float32),
        "risk_target": risk_target(row, tile_to_index),
        "fan_target": np.asarray([float(row["outcome"].get("fan_count", 0)) / 10.0], dtype=np.float32),
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


def encode_discard_sequence(context: dict[str, Any], tile_to_index: dict[str, int]) -> np.ndarray:
    sequence = np.zeros(
        (DISCARD_SEQUENCE_LENGTH, DISCARD_EVENT_FEATURE_COUNT),
        dtype=np.float32,
    )
    history = context.get("discard_history", [])
    seat_index = int(context.get("seat_index", 0))
    seat_count = max(1, int(context.get("seat_count", 4)))
    retained = history[-DISCARD_SEQUENCE_LENGTH:]
    start = DISCARD_SEQUENCE_LENGTH - len(retained)
    for offset, event in enumerate(retained):
        slot = start + offset
        tile_index_value = tile_to_index.get(str(event.get("tile_key", "")))
        if tile_index_value is not None:
            sequence[slot, tile_index_value] = 1.0
        event_seat = int(event.get("seat_index", seat_index))
        relative_seat = (event_seat + seat_count - seat_index) % seat_count
        if 0 <= relative_seat < 4:
            sequence[slot, TILE_KIND_COUNT + relative_seat] = 1.0
        sequence[slot, 38] = float(slot + 1) / float(DISCARD_SEQUENCE_LENGTH)
        sequence[slot, 39] = 1.0 if offset == len(retained) - 1 else 0.0
    return sequence


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
    
    round_wind = context.get("round_wind", "east")
    round_wind_val = ROUND_WIND_TO_INDEX.get(round_wind, 0.0)
    features[10] = round_wind_val / 3.0
    seat_wind = context.get("seat_wind") or seat_wind_key(
        seat_index,
        int(context.get("dealer_seat", 0)),
    )
    features[11] = 1.0 if seat_wind == round_wind else 0.0
    
    return features


def seat_wind_key(seat_index: int, dealer_seat: int) -> str:
    return STANDARD_WIND_ORDER[(seat_index + 4 - dealer_seat) % 4]


def encode_discard_mask(row: dict[str, Any], tile_to_index: dict[str, int]) -> np.ndarray:
    mask = np.zeros((TILE_KIND_COUNT,), dtype=np.bool_)
    for action in row.get("legal_actions", []):
        if action.startswith("discard:"):
            tile_key = action.split(":", 1)[1]
            if tile_key in tile_to_index:
                mask[tile_to_index[tile_key]] = True
    return mask


def encode_claim_mask(
    row: dict[str, Any],
    claim_to_index: dict[str, int],
    tile_to_index: dict[str, int],
) -> np.ndarray:
    mask = np.zeros((len(claim_to_index),), dtype=np.bool_)
    last_discard = row.get("context", {}).get("last_discard_tile_key")
    for action in row.get("legal_actions", []):
        if action == "pass":
            mask[claim_to_index["pass"]] = True
        elif action.startswith("claim:"):
            claim_name = claim_name_from_action_id(action, last_discard, tile_to_index)
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


def claim_target(
    row: dict[str, Any],
    claim_to_index: dict[str, int],
    tile_to_index: dict[str, int],
) -> int:
    label_type = row["label"]["type"]
    if label_type == "pass":
        if row["decision_kind"] in {"claim_window", "rob_kong"}:
            return claim_to_index["pass"]
        return IGNORE_INDEX
    if label_type == "hu":
        return claim_to_index["hu"]
    if label_type == "claim_pung":
        return claim_to_index["pung"]
    if label_type == "claim_kong":
        return claim_to_index["kong"]
    if label_type == "claim_chow":
        claim_name = chow_claim_name(
            row.get("context", {}).get("last_discard_tile_key"),
            row["label"].get("middle_tile_key"),
            tile_to_index,
        )
        return claim_to_index[claim_name]
    return IGNORE_INDEX


def self_kong_target(row: dict[str, Any], self_kong_to_index: dict[str, int]) -> int:
    label = row["label"]
    if label["type"] == "pass" and any(
        action.startswith("self_kong:") for action in row.get("legal_actions", [])
    ):
        return self_kong_to_index["pass"]
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


def claim_name_from_action_id(
    action: str,
    last_discard: str | None,
    tile_to_index: dict[str, int],
) -> str:
    parts = action.split(":")
    if len(parts) < 2:
        return action
    if parts[1] != "chow":
        return parts[1]
    middle_tile_key = parts[2] if len(parts) > 2 else None
    return chow_claim_name(last_discard, middle_tile_key, tile_to_index)


def chow_claim_name(
    last_discard: str | None,
    middle_tile_key: str | None,
    tile_to_index: dict[str, int],
) -> str:
    if last_discard not in tile_to_index or middle_tile_key not in tile_to_index:
        return "chow_mid"
    discard_index = tile_to_index[last_discard]
    middle_index = tile_to_index[middle_tile_key]
    if discard_index >= 27 or middle_index >= 27 or discard_index // 9 != middle_index // 9:
        return "chow_mid"
    if discard_index == middle_index - 1:
        return "chow_left"
    if discard_index == middle_index + 1:
        return "chow_right"
    return "chow_mid"


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
