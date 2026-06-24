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
    restore_export_value_head,
)
from model import ModelConfig, build_model
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
from candidate_bank import CandidateRecord, CandidateBank, select_best_candidate
from bucket_report import bucket_key, summarize_buckets
from counterfactual_dataset import CounterfactualDiscardDataset
from train_discard_ranker import compute_ranker_loss, ranker_metrics


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

    def test_value_source_uses_current_value_when_precomputed_advantage_disabled(self):
        returns = torch.tensor([2.0, 2.0])
        values = torch.tensor([3.0, 0.0])
        precomputed = torch.tensor([2.0, -2.0])

        weights, advantage = advantage_weights(
            returns,
            values,
            precomputed,
            adv_norm="batch",
            adv_source="value",
            temperature=1.0,
            weight_clip=20.0,
            policy_filter="positive",
        )

        assert advantage.tolist() == pytest.approx([-1.0, 1.0])
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


class TestAwrExportValueHead:
    def test_restores_sft_value_head_without_touching_policy_weights(self):
        config = ModelConfig()
        awr_model = build_model(config)
        sft_model = build_model(config)

        awr_state = awr_model.state_dict()
        sft_state = sft_model.state_dict()
        for name in awr_state:
            awr_state[name] = torch.full_like(awr_state[name], 1.0)
            sft_state[name] = torch.full_like(sft_state[name], 2.0)

        restored = restore_export_value_head(
            checkpoint_state=awr_state,
            risk_checkpoint_state=sft_state,
        )

        assert torch.equal(restored["value_head.net.0.weight"], sft_state["value_head.net.0.weight"])
        assert torch.equal(restored["discard_head.net.0.weight"], awr_state["discard_head.net.0.weight"])


class TestCandidateGatePairedSubjects:
    def test_selection_gate_accepts_weighted_positive_candidate_with_paired_warning(self):
        summary = make_gate_summary(
            baseline=make_policy_summary(avg_score_delta=0.0, win_rate=3.0, deal_in_rate=2.0),
            candidate=make_policy_summary(avg_score_delta=8.0, win_rate=3.1, deal_in_rate=2.0),
            paired={
                "baseline_neural__vs__awr_candidate_neural": {
                    "baseline_policy": "baseline_neural",
                    "candidate_policy": "awr_candidate_neural",
                    "paired_match_count": 80,
                    "avg_score_delta": 8.0,
                    "confidence95_low": -4.0,
                    "confidence95_high": 20.0,
                }
            },
        )

        selection = evaluate_candidate_matrix([summary], pool_path=None, gate_mode="selection")
        promotion = evaluate_candidate_matrix([summary], pool_path=None, gate_mode="promotion")

        assert selection["accepted"] is True
        assert selection["selection_failures"] == []
        assert promotion["accepted"] is False
        assert "paired_confidence" in promotion["all_failures"]

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


class TestCandidateBank:
    def test_selects_best_candidate_by_weighted_score_then_safety(self):
        bank = CandidateBank()
        weak = CandidateRecord(
            iter=0,
            checkpoint="iter_0/awr_best.pt",
            onnx="iter_0/awr.onnx",
            gate_result={
                "weighted_metrics": {
                    "avg_score_delta": 2.0,
                    "win_rate": 0.1,
                    "deal_in_rate": 0.0,
                }
            },
            selected=True,
            promoted=False,
        )
        strong = CandidateRecord(
            iter=1,
            checkpoint="iter_1/awr_best.pt",
            onnx="iter_1/awr.onnx",
            gate_result={
                "weighted_metrics": {
                    "avg_score_delta": 5.0,
                    "win_rate": 0.0,
                    "deal_in_rate": 0.1,
                }
            },
            selected=True,
            promoted=False,
        )

        bank.add(weak)
        bank.add(strong)

        assert select_best_candidate(bank).iter == 1


class TestBucketReport:
    def test_summarizes_trajectory_rows_by_phase_and_action_head(self):
        rows = [
            make_sample_row(
                policy_id="learner",
                action_head="discard",
                phase_bucket="late",
                risk_bucket="high",
                reward=-1.0,
            ),
            make_sample_row(
                policy_id="learner",
                action_head="discard",
                phase_bucket="late",
                risk_bucket="high",
                reward=1.0,
            ),
            make_sample_row(
                policy_id="learner",
                action_head="claim",
                phase_bucket="early",
                risk_bucket="low",
                reward=0.5,
            ),
        ]

        summary = summarize_buckets(rows)

        assert bucket_key(rows[0]) == "late/high/discard"
        assert summary["learner"]["late/high/discard"]["count"] == 2
        assert summary["learner"]["late/high/discard"]["avg_reward"] == pytest.approx(0.0)
        assert summary["learner"]["early/low/claim"]["count"] == 1


class TestCounterfactualDiscardDataset:
    def test_loads_counterfactual_discard_rows(self):
        row = {
            "schema_version": 1,
            "match_id": "m1",
            "decision_index": 1,
            "seat_index": 0,
            "tile_planes": [0.0] * 340,
            "scalar_features": [0.0] * 13,
            "discard_sequence": [0.0] * 1280,
            "discard_mask": [True] * 34,
            "legal_discards": [0, 3, 5],
            "teacher_scores": [0.1, 0.5, -0.2],
            "teacher_best_index": 3,
            "phase_bucket": "mid",
            "risk_bucket": "low",
        }
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "cf.jsonl"
            write_jsonl([row], path)

            ds = CounterfactualDiscardDataset(path)
            item = ds[0]

        assert len(ds) == 1
        assert item["tile_planes"].shape == (10, 34)
        assert item["legal_mask"].sum().item() == 3
        assert item["teacher_scores"][3].item() == pytest.approx(0.5)
        assert item["teacher_best_index"].item() == 3

    def test_filters_by_policy_id(self):
        learner = {
            "schema_version": 1,
            "policy_id": "learner",
            "tile_planes": [0.0] * 340,
            "scalar_features": [0.0] * 13,
            "discard_sequence": [0.0] * 1280,
            "discard_mask": [True] * 34,
            "legal_discards": [0],
            "teacher_scores": [1.0],
            "teacher_best_index": 0,
        }
        opponent = {**learner, "policy_id": "opponent"}
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "cf.jsonl"
            write_jsonl([opponent, learner], path)

            ds = CounterfactualDiscardDataset(path, policy_id="learner")

        assert len(ds) == 1


class TestDiscardRanker:
    def test_loss_masks_illegal_teacher_scores(self):
        outputs = {
            "discard_logits": torch.tensor([[3.0, 4.0, 0.0]], dtype=torch.float32),
        }
        changed_illegal_outputs = {
            "discard_logits": torch.tensor([[3.0, -99.0, 0.0]], dtype=torch.float32),
        }
        batch = {
            "legal_mask": torch.tensor([[True, False, True]]),
            "teacher_scores": torch.tensor([[3.0, -1000.0, 0.0]], dtype=torch.float32),
            "teacher_best_index": torch.tensor([0]),
        }

        loss = compute_ranker_loss(outputs, batch, temperature=1.0, top1_weight=0.0)
        changed_illegal_loss = compute_ranker_loss(
            changed_illegal_outputs,
            batch,
            temperature=1.0,
            top1_weight=0.0,
        )

        assert torch.isfinite(loss)
        assert loss.item() < 0.1
        assert changed_illegal_loss.item() == pytest.approx(loss.item())

    def test_metrics_compare_masked_top1(self):
        outputs = {
            "discard_logits": torch.tensor(
                [[0.0, 9.0, 2.0], [5.0, 0.0, 2.0]],
                dtype=torch.float32,
            ),
        }
        batch = {
            "legal_mask": torch.tensor([[True, False, True], [False, True, True]]),
            "teacher_scores": torch.tensor(
                [[0.0, -1000.0, 2.0], [-1000.0, 0.0, 2.0]],
                dtype=torch.float32,
            ),
            "teacher_best_index": torch.tensor([2, 2]),
        }

        metrics = ranker_metrics(outputs, batch)

        assert metrics["top1"] == pytest.approx(1.0)
        assert metrics["target_margin"] == pytest.approx(2.0)
