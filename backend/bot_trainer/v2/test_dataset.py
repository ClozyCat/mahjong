from __future__ import annotations

import json
from pathlib import Path

from dataset import IGNORE_INDEX, encode_row


def test_encode_row_without_torch_dependency(tmp_path: Path) -> None:
    metadata_path, train_path = write_fixture(tmp_path)
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    row = json.loads(train_path.read_text(encoding="utf-8").splitlines()[0])

    encoded = encode_row(row, metadata)

    assert encoded["tile_planes"].shape == (10, 34)
    assert encoded["scalar_features"].shape == (10,)
    assert encoded["discard_mask"].shape == (34,)
    assert encoded["discard_target"].item() == 0


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
