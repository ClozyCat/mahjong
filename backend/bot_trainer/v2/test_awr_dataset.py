import json
import tempfile
from pathlib import Path

import torch
import torch.nn.functional as F
import pytest

from awr_dataset import (
    ArenaTrajectoryDataset,
    encode_row,
    compute_discounted_returns_for_rows,
    compute_normalized_advantages,
    action_head_index,
    trajectory_diagnostics,
)


def make_sample_row(**overrides) -> dict:
    row = {
        "match_id": "test_match_1",
        "seat_index": 0,
        "decision_index": 0,
        "policy_id": "learner",
        "action_head": "discard",
        "action_index": 5,
        "action_semantic": "discard:w6",
        "log_prob": -1.5,
        "value": 0.0,
        "reward": 0.1,
        "step_reward": 0.05,
        "terminal_reward": 0.0,
        "shanten_before": 3,
        "shanten_after": 2,
        "risk_probs": [0.1] * 34,
        "opponent_tenpai_target": [0.0, 0.0, 0.0],
        "opponent_risk_target": [[0.0] * 34] * 3,
        "opponent_risk_mask": [[True] * 34] * 3,
        "done": False,
        "tile_planes": [0.0] * 340,
        "scalar_features": [0.0] * 12,
        "discard_sequence": [0.0] * 1280,
        "discard_mask": [True] * 34,
        "claim_mask": [True] * 7,
        "self_kong_mask": [True] * 3,
        "hu_mask": [True] * 2,
    }
    row.update(overrides)
    return row


def write_jsonl(rows: list[dict], path: Path) -> None:
    with open(path, "w", encoding="utf-8") as f:
        for row in rows:
            f.write(json.dumps(row) + "\n")


class TestArenaTrajectoryDataset:
    def test_load_and_length(self):
        rows = [make_sample_row(decision_index=i) for i in range(10)]
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "test.jsonl"
            write_jsonl(rows, path)
            ds = ArenaTrajectoryDataset(path)
            assert len(ds) == 10

    def test_filter_by_policy_id(self):
        rows = [
            make_sample_row(decision_index=0, policy_id="learner"),
            make_sample_row(decision_index=1, policy_id="opponent"),
            make_sample_row(decision_index=2, policy_id="learner"),
        ]
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "test.jsonl"
            write_jsonl(rows, path)
            ds = ArenaTrajectoryDataset(path, policy_id="learner")
            assert len(ds) == 2

    def test_item_shapes(self):
        rows = [make_sample_row()]
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "test.jsonl"
            write_jsonl(rows, path)
            ds = ArenaTrajectoryDataset(path)
            item = ds[0]
            assert item["tile_planes"].shape == (10, 34)
            assert item["scalar_features"].shape == (12,)
            assert item["discard_sequence"].shape == (32, 40)
            assert item["discard_mask"].shape == (34,)
            assert item["claim_mask"].shape == (7,)
            assert item["self_kong_mask"].shape == (3,)
            assert item["hu_mask"].shape == (2,)
            assert item["action_index"].ndim == 0
            assert "return" in item

    def test_compute_returns_single_seat(self):
        rows = [
            make_sample_row(decision_index=0, seat_index=0, reward=0.0, done=False),
            make_sample_row(decision_index=1, seat_index=0, reward=0.0, done=False),
            make_sample_row(decision_index=2, seat_index=0, reward=1.0, done=True),
        ]
        returns = compute_discounted_returns_for_rows(rows, gamma=0.99)
        assert abs(returns[0] - 1.0) < 0.02
        assert abs(returns[1] - 1.0) < 0.02
        assert abs(returns[2] - 1.0) < 0.01

    def test_action_head_index(self):
        assert action_head_index("discard") == 0
        assert action_head_index("claim") == 1
        assert action_head_index("self_kong") == 2
        assert action_head_index("hu") == 3

    def test_trajectory_diagnostics(self):
        rows = [
            make_sample_row(action_head="discard"),
            make_sample_row(action_head="claim"),
            make_sample_row(action_head="discard"),
            make_sample_row(action_head="discard", terminal_reward=1.5, done=True),
        ]
        diag = trajectory_diagnostics(rows)
        assert diag["row_count"] == 4
        assert diag["action_head_discard"] == 3
        assert diag["action_head_claim"] == 1


class TestNormalizedAdvantages:
    def test_none_mode_returns_raw_advantage(self):
        rows = [
            {"match_id": "m1", "seat_index": 0, "reward": 0.1},
            {"match_id": "m1", "seat_index": 0, "reward": 0.5},
        ]
        returns = [0.2, 0.6]
        values = [0.3, 0.4]
        adv = compute_normalized_advantages(rows, returns, values, mode="none")
        assert abs(adv[0] - (-0.1)) < 0.001
        assert abs(adv[1] - 0.2) < 0.001

    def test_per_match_normalization(self):
        rows = [
            {"match_id": "m1", "seat_index": 0, "reward": 0.1},
            {"match_id": "m1", "seat_index": 0, "reward": 0.5},
            {"match_id": "m1", "seat_index": 0, "reward": -0.3},
        ]
        returns = [0.2, 0.6, -0.2]
        values = [0.3, 0.3, 0.3]
        adv = compute_normalized_advantages(rows, returns, values, mode="per_match")
        mean = sum(adv) / len(adv)
        assert abs(mean) < 0.001, f"mean should be ~0, got {mean}"
        std = (sum((a - mean) ** 2 for a in adv) / len(adv)) ** 0.5
        assert abs(std - 1.0) < 0.001, f"std should be ~1, got {std}"

    def test_per_seat_normalization(self):
        rows = [
            {"match_id": "m1", "seat_index": 0, "reward": 0.1},
            {"match_id": "m1", "seat_index": 0, "reward": -0.1},
            {"match_id": "m1", "seat_index": 1, "reward": 0.9},
            {"match_id": "m1", "seat_index": 1, "reward": 1.1},
        ]
        returns = [0.2, -0.2, 1.0, 1.2]
        values = [0.1, 0.1, 0.1, 0.1]
        adv = compute_normalized_advantages(rows, returns, values, mode="per_seat")
        assert abs(adv[0] - (-adv[1])) < 0.001
        assert abs(adv[2] - (-adv[3])) < 0.001

    def test_clips_outliers(self):
        rows = [{"match_id": "m1", "seat_index": 0, "reward": 100.0}]
        returns = [100.0]
        values = [0.0]
        adv = compute_normalized_advantages(rows, returns, values, mode="none")
        assert abs(adv[0]) <= 5.0


def masked_categorical_kl(teacher_logits, student_logits, mask):
    teacher = teacher_logits.clone()
    student = student_logits.clone()
    teacher[~mask] = float("-inf")
    student[~mask] = float("-inf")
    teacher_probs = F.softmax(teacher, dim=-1)
    student_log_probs = F.log_softmax(student, dim=-1)
    element_kl = teacher_probs * (torch.log(teacher_probs + 1e-8) - student_log_probs)
    element_kl = torch.where(mask, element_kl, torch.zeros_like(element_kl))
    kl_per_sample = element_kl.sum(-1)
    return kl_per_sample.mean()


class TestKLDivergence:
    def test_identical_logits_kl_zero(self):
        logits = torch.randn(4, 34)
        mask = torch.ones(4, 34, dtype=torch.bool)
        kl = masked_categorical_kl(logits, logits, mask)
        assert kl.item() < 0.01

    def test_maximally_different_kl_positive(self):
        teacher = torch.zeros(4, 34)
        teacher[:, 0] = 10.0
        student = torch.zeros(4, 34)
        student[:, -1] = 10.0
        mask = torch.ones(4, 34, dtype=torch.bool)
        kl = masked_categorical_kl(teacher, student, mask)
        assert kl.item() > 1.0

    def test_masked_positions_ignored(self):
        teacher = torch.randn(4, 7)
        student = torch.randn(4, 7)
        mask = torch.zeros(4, 7, dtype=torch.bool)
        mask[:, 0] = True
        teacher[:, 1:] = 0
        student[:, 1:] = 999
        kl = masked_categorical_kl(teacher, student, mask)
        assert kl.item() < 0.01
