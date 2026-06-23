import json
import tempfile
from pathlib import Path

import torch
import pytest

from train_awr import (
    AwrDiagnostics,
    advantage_weights,
    compute_ce_loss_for_action,
    masked_categorical_kl,
)
from candidate_gate import evaluate_candidate, evaluate_candidate_matrix
from awr_dataset import (
    ArenaTrajectoryDataset,
    encode_row,
    compute_discounted_returns_for_rows,
    compute_normalized_advantages,
    action_head_index,
    split_rows_by_match_id,
    trajectory_diagnostics,
)
from arena_summary import paired_subject_deltas
from train_value import BestValueCheckpoint


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
        "done": False,
        "tile_planes": [0.0] * 340,
        "scalar_features": [0.0] * 13,
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
            assert item["scalar_features"].shape == (13,)
            assert item["discard_sequence"].shape == (32, 40)
            assert item["discard_mask"].shape == (34,)
            assert item["claim_mask"].shape == (7,)
            assert item["self_kong_mask"].shape == (3,)
            assert item["hu_mask"].shape == (2,)
            assert item["action_index"].ndim == 0
            assert "return" in item

    def test_item_includes_precomputed_advantage_when_available(self):
        rows = [make_sample_row(advantage=1.25)]
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "test.jsonl"
            write_jsonl(rows, path)
            ds = ArenaTrajectoryDataset(path)
            item = ds[0]
            assert item["advantage"].item() == pytest.approx(1.25)

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

    def test_from_rows_builds_dataset_without_jsonl_file(self):
        rows = [make_sample_row(match_id="m1", decision_index=0, reward=1.0)]

        ds = ArenaTrajectoryDataset.from_rows(rows, gamma=0.99)

        assert len(ds) == 1
        assert ds[0]["return"].item() == pytest.approx(1.0)

    def test_split_rows_by_match_id_is_deterministic_and_keeps_matches_intact(self):
        rows = [
            make_sample_row(match_id="m1", decision_index=0),
            make_sample_row(match_id="m1", decision_index=1),
            make_sample_row(match_id="m2", decision_index=2),
            make_sample_row(match_id="m2", decision_index=3),
            make_sample_row(match_id="m3", decision_index=4),
            make_sample_row(match_id="m3", decision_index=5),
            make_sample_row(match_id="m4", decision_index=6),
            make_sample_row(match_id="m4", decision_index=7),
        ]

        train_rows, val_rows = split_rows_by_match_id(rows, val_fraction=0.25, seed=7)

        assert len(val_rows) == 2
        assert len(train_rows) == 6
        assert {row["match_id"] for row in val_rows} == {"m4"}
        assert not ({row["match_id"] for row in train_rows} & {row["match_id"] for row in val_rows})


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

    def test_per_player_normalization(self):
        rows = [
            {"match_id": "m1", "seat_index": 0, "policy_id": "p1", "reward": 0.1},
            {"match_id": "m1", "seat_index": 1, "policy_id": "p1", "reward": -0.1},
            {"match_id": "m1", "seat_index": 0, "policy_id": "p2", "reward": 0.9},
            {"match_id": "m1", "seat_index": 1, "policy_id": "p2", "reward": 1.1},
        ]
        returns = [0.2, -0.2, 1.0, 1.2]
        values = [0.1, 0.1, 0.1, 0.1]
        adv = compute_normalized_advantages(rows, returns, values, mode="per_player")
        assert abs(adv[0] - (-adv[1])) < 0.001
        assert abs(adv[2] - (-adv[3])) < 0.001

    def test_clips_outliers(self):
        rows = [{"match_id": "m1", "seat_index": 0, "reward": 100.0}]
        returns = [100.0]
        values = [0.0]
        adv = compute_normalized_advantages(rows, returns, values, mode="none")
        assert abs(adv[0]) <= 5.0

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

    def test_all_invalid_rows_do_not_produce_nan(self):
        teacher = torch.randn(4, 7)
        student = torch.randn(4, 7)
        mask = torch.zeros(4, 7, dtype=torch.bool)
        kl = masked_categorical_kl(teacher, student, mask)
        assert torch.isfinite(kl)
        assert kl.item() == pytest.approx(0.0)


class TestAwrWeights:
    def test_value_source_uses_precomputed_normalized_advantage(self):
        returns = torch.tensor([100.0, 100.0])
        values = torch.tensor([0.0, 0.0])
        precomputed = torch.tensor([-1.0, 2.0])

        weights, advantage = advantage_weights(
            returns,
            values,
            precomputed,
            adv_norm="per_match",
            adv_source="value",
            temperature=1.0,
            weight_clip=20.0,
            policy_filter="positive",
        )

        assert advantage.tolist() == pytest.approx([-1.0, 2.0])
        assert weights[0].item() == pytest.approx(0.0)
        assert weights[1].item() > 1.0

    def test_return_source_ignores_precomputed_advantage(self):
        returns = torch.tensor([-2.0, 2.0])
        values = torch.tensor([0.0, 0.0])
        precomputed = torch.tensor([2.0, -2.0])

        weights, advantage = advantage_weights(
            returns,
            values,
            precomputed,
            adv_norm="per_match",
            adv_source="return",
            temperature=1.0,
            weight_clip=20.0,
            policy_filter="positive",
        )

        assert advantage.tolist() == pytest.approx([-2.0, 2.0])
        assert weights[0].item() == pytest.approx(0.0)
        assert weights[1].item() > 1.0

    def test_precomputed_advantage_drives_weights(self):
        returns = torch.tensor([0.0, 0.0])
        values = torch.tensor([0.0, 0.0])
        precomputed = torch.tensor([-1.0, 2.0])

        weights, advantage = advantage_weights(
            returns,
            values,
            precomputed,
            adv_norm="per_match",
            temperature=1.0,
            weight_clip=20.0,
            policy_filter="positive",
        )

        assert advantage.tolist() == pytest.approx([-1.0, 2.0])
        assert weights[0].item() == pytest.approx(0.0)
        assert weights[1].item() > 1.0

    def test_zero_action_weights_have_zero_loss_and_finite_gradient(self):
        logits = torch.randn(2, 4, requires_grad=True)
        mask = torch.ones(2, 4, dtype=torch.bool)
        actions = torch.tensor([0, 1])
        weights = torch.zeros(2)

        loss = compute_ce_loss_for_action(logits, mask, actions, weights)
        loss.backward()

        assert loss.item() == pytest.approx(0.0)
        assert torch.isfinite(logits.grad).all()

    def test_inactive_mask_rows_with_zero_weight_do_not_produce_nan(self):
        logits = torch.randn(2, 4, requires_grad=True)
        mask = torch.zeros(2, 4, dtype=torch.bool)
        actions = torch.tensor([0, 1])
        weights = torch.zeros(2)

        loss = compute_ce_loss_for_action(logits, mask, actions, weights)
        loss.backward()

        assert loss.item() == pytest.approx(0.0)
        assert torch.isfinite(logits.grad).all()


class TestAwrDiagnostics:
    def test_accumulates_advantage_and_weight_metrics(self):
        diagnostics = AwrDiagnostics()

        diagnostics.update(
            advantage=torch.tensor([-1.0, 0.0, 2.0]),
            weights=torch.tensor([0.0, 1.0, 4.0]),
        )
        metrics = diagnostics.summary()

        assert metrics["adv_mean"] == pytest.approx(1.0 / 3.0)
        assert metrics["adv_pos_rate"] == pytest.approx(1.0 / 3.0)
        assert metrics["weight_mean"] == pytest.approx(5.0 / 3.0)
        assert metrics["weight_max"] == pytest.approx(4.0)
        assert metrics["active_weight_rate"] == pytest.approx(2.0 / 3.0)
        assert metrics["adv_std"] > 0.0


class TestCandidateGatePairedSubjects:
    def test_uses_reverse_paired_key_with_candidate_oriented_delta(self):
        summary = {
            "policies": {
                "baseline_neural": make_policy_summary(avg_score_delta=0.0),
                "awr_candidate_neural": make_policy_summary(avg_score_delta=10.0),
            },
            "paired_subjects": {
                "awr_candidate_neural__vs__baseline_neural": {
                    "baseline_policy": "awr_candidate_neural",
                    "candidate_policy": "baseline_neural",
                    "avg_score_delta": -10.0,
                    "confidence95_low": -15.0,
                    "confidence95_high": -5.0,
                    "positive_delta_rate": 0.2,
                    "min_score_delta": -20.0,
                    "max_score_delta": -1.0,
                }
            },
        }

        result = evaluate_candidate(
            summary,
            baseline_policy="baseline_neural",
            candidate_policy="awr_candidate_neural",
        )

        paired = result["promotion_report"]["paired"]
        assert paired["baseline_policy"] == "baseline_neural"
        assert paired["candidate_policy"] == "awr_candidate_neural"
        assert paired["avg_score_delta"] == pytest.approx(10.0)
        assert paired["confidence95_low"] == pytest.approx(5.0)
        assert paired["confidence95_high"] == pytest.approx(15.0)
        assert "paired_subjects_missing" not in result["promotion_report"]["warnings"]

    def test_matrix_rejects_high_latency_even_when_weighted_metrics_pass(self):
        summary = make_gate_summary(
            baseline=make_policy_summary(avg_score_delta=0.0, win_rate=0.5, deal_in_rate=0.2),
            candidate=make_policy_summary(
                avg_score_delta=10.0,
                win_rate=0.6,
                deal_in_rate=0.1,
                avg_latency_ms_per_decision=250.0,
            ),
        )

        result = evaluate_candidate_matrix([summary], pool_path=None)

        assert result["accepted"] is False
        assert "latency" in result["all_failures"]

    def test_matrix_rejects_paired_ci_crossing_zero_when_paired_data_exists(self):
        summary = make_gate_summary(
            baseline=make_policy_summary(avg_score_delta=0.0, win_rate=0.5, deal_in_rate=0.2),
            candidate=make_policy_summary(avg_score_delta=10.0, win_rate=0.6, deal_in_rate=0.1),
            paired={
                "baseline_neural__vs__awr_candidate_neural": {
                    "baseline_policy": "baseline_neural",
                    "candidate_policy": "awr_candidate_neural",
                    "paired_match_count": 20,
                    "avg_score_delta": 10.0,
                    "confidence95_low": -1.0,
                    "confidence95_high": 21.0,
                }
            },
        )

        result = evaluate_candidate_matrix([summary], pool_path=None)

        assert result["accepted"] is False
        assert "paired_confidence" in result["all_failures"]

    def test_arena_summary_emits_both_paired_key_directions(self):
        paired = paired_subject_deltas({
            (1, 0): {
                "baseline_neural": 10.0,
                "awr_candidate_neural": 15.0,
            },
            (2, 0): {
                "baseline_neural": 20.0,
                "awr_candidate_neural": 18.0,
            },
        })

        assert "baseline_neural__vs__awr_candidate_neural" in paired
        assert "awr_candidate_neural__vs__baseline_neural" in paired
        assert paired["baseline_neural__vs__awr_candidate_neural"]["avg_score_delta"] == pytest.approx(1.5)
        assert paired["awr_candidate_neural__vs__baseline_neural"]["avg_score_delta"] == pytest.approx(-1.5)


def make_policy_summary(**overrides) -> dict:
    summary = {
        "avg_score_delta": 0.0,
        "win_rate": 0.5,
        "deal_in_rate": 0.1,
        "avg_first_tenpai_turn": 12.0,
        "final_tenpai_rate": 0.5,
        "avg_claims": 10.0,
        "avg_latency_ms_per_decision": 1.0,
    }
    summary.update(overrides)
    return summary


def make_gate_summary(
    *,
    baseline: dict,
    candidate: dict,
    paired: dict | None = None,
) -> dict:
    return {
        "policies": {
            "baseline_neural": baseline,
            "awr_candidate_neural": candidate,
        },
        "paired_subjects": paired or {},
    }


class TestBestValueCheckpoint:
    def test_uses_validation_ev_for_selection_when_available(self):
        tracker = BestValueCheckpoint(patience=2)

        assert tracker.update(epoch=1, train_ev=0.1, val_ev=0.0) is True
        assert tracker.update(epoch=2, train_ev=0.2, val_ev=-0.1) is False

        assert tracker.best_epoch == 1
        assert tracker.best_score == pytest.approx(0.0)
        assert tracker.should_stop is False

    def test_stops_after_patience_without_improvement(self):
        tracker = BestValueCheckpoint(patience=2)

        tracker.update(epoch=1, train_ev=0.1, val_ev=0.1)
        tracker.update(epoch=2, train_ev=0.2, val_ev=0.0)
        tracker.update(epoch=3, train_ev=0.3, val_ev=-0.1)

        assert tracker.should_stop is True
