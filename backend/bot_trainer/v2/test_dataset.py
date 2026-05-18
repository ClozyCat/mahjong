from __future__ import annotations

import json
from pathlib import Path

from dataset import (
    DISCARD_EVENT_FEATURE_COUNT,
    DISCARD_SEQUENCE_LENGTH,
    IGNORE_INDEX,
    SCALAR_FEATURE_COUNT,
    encode_row,
)


def test_encode_row_without_torch_dependency(tmp_path: Path) -> None:
    metadata_path, train_path = write_fixture(tmp_path)
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    row = json.loads(train_path.read_text(encoding="utf-8").splitlines()[0])

    encoded = encode_row(row, metadata)

    assert encoded["tile_planes"].shape == (10, 34)
    assert encoded["scalar_features"].shape == (SCALAR_FEATURE_COUNT,)
    assert encoded["discard_sequence"].shape == (
        DISCARD_SEQUENCE_LENGTH,
        DISCARD_EVENT_FEATURE_COUNT,
    )
    assert encoded["discard_mask"].shape == (34,)
    assert encoded["discard_target"].item() == 0


def test_discard_sequence_encodes_order_source_and_latest_marker(tmp_path: Path) -> None:
    metadata_path, train_path = write_fixture(tmp_path)
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    row = json.loads(train_path.read_text(encoding="utf-8").splitlines()[0])
    row["context"]["seat_index"] = 0
    row["context"]["discard_history"] = [
        {"seat_index": 1, "tile_key": "w3"},
        {"seat_index": 2, "tile_key": "t5"},
    ]

    encoded = encode_row(row, metadata)
    sequence = encoded["discard_sequence"]
    previous = sequence[-2]
    latest = sequence[-1]

    assert previous[2].item() == 1.0
    assert previous[34:38].tolist() == [0.0, 1.0, 0.0, 0.0]
    assert previous[39].item() == 0.0
    assert latest[13].item() == 1.0
    assert latest[34:38].tolist() == [0.0, 0.0, 1.0, 0.0]
    assert latest[39].item() == 1.0


def test_scalar_features_use_standard_seat_wind_for_runtime_context(tmp_path: Path) -> None:
    metadata_path, train_path = write_fixture(tmp_path)
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    row = json.loads(train_path.read_text(encoding="utf-8").splitlines()[0])
    row["context"]["seat_index"] = 1
    row["context"]["dealer_seat"] = 0
    row["context"]["round_wind"] = "south"

    encoded = encode_row(row, metadata)

    assert abs(encoded["scalar_features"][10].item() - (1.0 / 3.0)) < 1e-6
    assert encoded["scalar_features"][11].item() == 1.0


def test_scalar_features_use_standard_north_wind_index(tmp_path: Path) -> None:
    metadata_path, train_path = write_fixture(tmp_path)
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    row = json.loads(train_path.read_text(encoding="utf-8").splitlines()[0])
    row["context"]["seat_index"] = 3
    row["context"]["dealer_seat"] = 0
    row["context"]["seat_wind"] = "north"
    row["context"]["round_wind"] = "north"

    encoded = encode_row(row, metadata)

    assert encoded["scalar_features"][10].item() == 1.0
    assert encoded["scalar_features"][11].item() == 1.0


def test_dataset_reads_batches_from_disk_cache(tmp_path: Path) -> None:
    import pytest

    torch = pytest.importorskip("torch")
    from dataset import MahjongDecisionDataset
    from train import build_loader

    metadata_path, train_path = write_fixture(tmp_path)
    dataset = MahjongDecisionDataset(train_path, metadata_path, cache_dir=tmp_path / "cache")

    batch = dataset.get_batch([0])
    loader_batch = next(iter(build_loader(dataset, 1, False, 0, torch.device("cpu"))))

    assert len(dataset) == 1
    assert (tmp_path / "cache" / "train" / "tile_planes.npy").exists()
    assert batch["tile_planes"].shape == (1, 10, 34)
    assert batch["scalar_features"].shape == (1, SCALAR_FEATURE_COUNT)
    assert batch["discard_sequence"].shape == (
        1,
        DISCARD_SEQUENCE_LENGTH,
        DISCARD_EVENT_FEATURE_COUNT,
    )
    assert batch["discard_target"].tolist() == [0]
    assert batch["discard_mask"].dtype == torch.bool
    assert loader_batch["discard_target"].tolist() == [0]


def test_chow_claim_target_uses_discard_position(tmp_path: Path) -> None:
    metadata_path, train_path = write_fixture(tmp_path)
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    base_row = json.loads(train_path.read_text(encoding="utf-8").splitlines()[0])
    expected_by_discard = {"w3": 4, "w4": 5, "w5": 6}

    for last_discard, expected_target in expected_by_discard.items():
        row = claim_row(base_row, last_discard, "w4")
        encoded = encode_row(row, metadata)
        assert encoded["claim_target"].item() == expected_target
        assert encoded["claim_mask"][expected_target].item()


def test_self_kong_pass_trains_self_kong_head_only(tmp_path: Path) -> None:
    metadata_path, train_path = write_fixture(tmp_path)
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    row = json.loads(train_path.read_text(encoding="utf-8").splitlines()[0])
    row["decision_kind"] = "active_turn"
    row["context"]["self_kong_candidates"] = [
        {
            "kind": "concealed_kong",
            "tile_ids": ["w1#0", "w1#1", "w1#2", "w1#3"],
            "tile_key": "w1",
            "meld_index": None,
        }
    ]
    row["legal_actions"] = ["pass", "self_kong:concealed_kong:w1"]
    row["label"] = {"type": "pass"}

    encoded = encode_row(row, metadata)

    assert encoded["claim_target"].item() == IGNORE_INDEX
    assert encoded["self_kong_target"].item() == 0
    assert encoded["self_kong_mask"][0].item()
    assert encoded["self_kong_mask"][1].item()


def test_auxiliary_loss_weights_can_disable_value_and_risk() -> None:
    import torch
    from train import compute_losses

    outputs = {
        "discard_logits": torch.tensor([[3.0, 0.0] + [-100.0] * 32]),
        "claim_logits": torch.zeros((1, 7)),
        "self_kong_logits": torch.zeros((1, 3)),
        "hu_logits": torch.zeros((1, 2)),
        "value": torch.tensor([[999.0]]),
        "risk_logits": torch.full((1, 34), 999.0),
        "fan_logits": torch.zeros((1, 1)),
    }
    batch = {
        "discard_mask": torch.tensor([[True, True] + [False] * 32]),
        "discard_target": torch.tensor([0]),
        "claim_mask": torch.zeros((1, 7), dtype=torch.bool),
        "claim_target": torch.tensor([-100]),
        "self_kong_mask": torch.zeros((1, 3), dtype=torch.bool),
        "self_kong_target": torch.tensor([-100]),
        "hu_mask": torch.tensor([[True, False]]),
        "hu_target": torch.tensor([-100]),
        "value_target": torch.tensor([[0.0]]),
        "risk_target": torch.zeros((1, 34)),
        "fan_target": torch.zeros((1, 1)),
    }

    losses = compute_losses(outputs, batch, value_weight=0.0, risk_weight=0.0, hu_weight=1.0)

    # value_loss和fan_loss被裁剪到max=100.0以防止数值爆炸
    assert losses["value_loss"].item() == 100.0
    assert losses["risk_loss"].item() > 100.0
    assert losses["loss"].item() < 0.1


def test_auxiliary_loss_weights_warm_up_to_targets() -> None:
    from train import loss_weights_for_epoch

    first = loss_weights_for_epoch(
        epoch=1,
        warmup_epochs=4,
        claim_weight=1.0,
        self_kong_weight=1.0,
        hu_weight=1.0,
        value_start=0.25,
        value_target=0.75,
        risk_start=0.25,
        risk_target=1.0,
        fan_start=0.25,
        fan_target=0.5,
    )
    final = loss_weights_for_epoch(
        epoch=4,
        warmup_epochs=4,
        claim_weight=1.0,
        self_kong_weight=1.0,
        hu_weight=1.0,
        value_start=0.25,
        value_target=0.75,
        risk_start=0.25,
        risk_target=1.0,
        fan_start=0.25,
        fan_target=0.5,
    )

    assert first["value_weight"] == 0.25
    assert first["risk_weight"] == 0.25
    assert first["fan_weight"] == 0.25
    assert final["value_weight"] == 0.75
    assert final["risk_weight"] == 1.0
    assert final["fan_weight"] == 0.5


def claim_row(base_row: dict, last_discard: str, middle_tile_key: str) -> dict:
    row = json.loads(json.dumps(base_row))
    row["decision_kind"] = "claim_window"
    row["context"]["last_discard_tile_key"] = last_discard
    row["legal_actions"] = ["pass", f"claim:chow:{middle_tile_key}"]
    row["label"] = {"type": "claim_chow", "middle_tile_key": middle_tile_key}
    return row


def write_fixture(tmp_path: Path) -> tuple[Path, Path]:
    metadata = {
        "schema_version": 2,
        "tile_keys": [
            "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9",
            "t1", "t2", "t3", "t4", "t5", "t6", "t7", "t8", "t9",
            "b1", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9",
            "east", "south", "west", "north", "red", "green", "white",
        ],
        "claim_actions": ["pass", "hu", "pung", "kong", "chow_left", "chow_mid", "chow_right"],
        "self_kong_actions": ["pass", "concealed_kong", "add_kong"],
    }
    row = {
        "schema_version": 2,
        "match_id": "fixture",
        "decision_index": 0,
        "seat_index": 0,
        "decision_kind": "active_turn",
        "context": {
            "seat_index": 0,
            "seat_count": 4,
            "dealer_seat": 0,
            "round_wind": "east",
            "cumulative_scores": [0, 0, 0, 0],
            "wall_tiles_remaining": 70,
            "visible_tile_keys": [],
            "opponent_discards_by_seat": [[], [], [], []],
            "opponent_melds_by_seat": [[], [], [], []],
            "player": {
                "concealed_tiles": [
                    {"tile_id": "w1#0", "tile_key": "w1", "is_flower": False},
                    {"tile_id": "t1#0", "tile_key": "t1", "is_flower": False},
                ],
                "concealed_tile_counts": [0] * 34,
                "meld_tile_key_groups": [],
                "flower_count": 0,
            },
            "restricted_discard_tile_key": None,
            "drawn_tile_id": "t1#0",
            "self_kong_candidates": [],
            "claim_options": [],
            "last_discard_tile_key": None,
            "add_kong_risk_tiles": [],
        },
        "legal_actions": ["discard:w1", "discard:t1"],
        "label": {"type": "discard", "tile_key": "w1"},
        "outcome": {"score_delta": 8, "won": True, "dealt_in": False, "round_drawn": False},
    }
    metadata_path = tmp_path / "metadata.json"
    train_path = tmp_path / "train.jsonl"
    metadata_path.write_text(json.dumps(metadata), encoding="utf-8")
    train_path.write_text(json.dumps(row) + "\n", encoding="utf-8")
    return metadata_path, train_path
