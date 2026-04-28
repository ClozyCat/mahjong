from __future__ import annotations

import json
from pathlib import Path

from dataset import encode_row


def test_encode_row_without_torch_dependency(tmp_path: Path) -> None:
    metadata_path, train_path = write_fixture(tmp_path)
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    row = json.loads(train_path.read_text(encoding="utf-8").splitlines()[0])

    encoded = encode_row(row, metadata)

    assert encoded["tile_planes"].shape == (10, 34)
    assert encoded["scalar_features"].shape == (10,)
    assert encoded["discard_mask"].shape == (34,)
    assert encoded["discard_target"].item() == 0


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
