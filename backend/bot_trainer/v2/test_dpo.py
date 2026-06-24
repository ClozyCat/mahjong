from __future__ import annotations

import json
import tempfile
from pathlib import Path

import torch
import torch.nn.functional as F
import pytest

from train_dpo import (
    listwise_dpo_loss,
    non_discard_kl,
    expected_risk,
    dpo_metrics,
)
from counterfactual_dataset import (
    CounterfactualDiscardDataset,
    collate_counterfactual,
    encode_counterfactual_row,
)
from model import ModelConfig, build_model


def make_cf_row(**overrides) -> dict:
    row = {
        "schema_version": 1,
        "match_id": "m1",
        "decision_index": 0,
        "seat_index": 0,
        "policy_id": "learner",
        "tile_planes": [0.0] * 340,
        "scalar_features": [0.0] * 13,
        "discard_sequence": [0.0] * 1280,
        "discard_mask": [False] * 34,
        "legal_discards": [0, 3, 5],
        "teacher_scores": [0.1, 0.5, -0.2],
        "risk_scores": [0.2, 0.7, 0.1],
        "teacher_best_index": 3,
        "phase_bucket": "mid",
        "risk_bucket": "low",
    }
    for tile in row["legal_discards"]:
        row["discard_mask"][tile] = True
    row.update(overrides)
    return row


def write_jsonl(rows: list[dict], path: Path) -> None:
    with open(path, "w", encoding="utf-8") as f:
        for row in rows:
            f.write(json.dumps(row) + "\n")


class TestListwiseDpoLoss:
    def test_identical_student_and_ref_with_zero_scores_has_entropy_level_loss(self):
        logits = torch.tensor([[1.0, 2.0, 3.0, 0.0]], dtype=torch.float32)
        mask = torch.tensor([[True, True, True, False]])
        teacher_scores = torch.zeros(1, 4)

        loss = listwise_dpo_loss(logits, logits, teacher_scores, mask, beta=1.0, temperature=1.0)

        legal_logp = F.log_softmax(logits[:, :3], dim=-1)
        entropy = -(legal_logp.exp() * legal_logp).sum().item()
        assert loss.item() == pytest.approx(entropy, abs=0.01)

    def test_higher_teacher_score_pushes_student_toward_that_action(self):
        ref_logits = torch.tensor([[0.0, 0.0, 0.0]], dtype=torch.float32)
        mask = torch.tensor([[True, True, True]])
        teacher_scores = torch.tensor([[5.0, 0.0, 0.0]], dtype=torch.float32)

        student_aligned = torch.tensor([[3.0, 0.0, 0.0]], dtype=torch.float32)
        student_misaligned = torch.tensor([[0.0, 3.0, 0.0]], dtype=torch.float32)

        loss_aligned = listwise_dpo_loss(student_aligned, ref_logits, teacher_scores, mask, beta=1.0, temperature=1.0)
        loss_misaligned = listwise_dpo_loss(student_misaligned, ref_logits, teacher_scores, mask, beta=1.0, temperature=1.0)

        assert loss_aligned.item() < loss_misaligned.item()

    def test_masked_actions_do_not_affect_loss(self):
        ref_logits = torch.tensor([[0.0, 0.0, 0.0, 0.0]], dtype=torch.float32)
        mask = torch.tensor([[True, True, False, False]])
        teacher_scores = torch.tensor([[1.0, 0.0, 99.0, 99.0]], dtype=torch.float32)

        loss = listwise_dpo_loss(ref_logits, ref_logits, teacher_scores, mask, beta=1.0, temperature=1.0)

        assert torch.isfinite(loss)
        assert loss.item() == pytest.approx(0.6931, abs=0.01)

    def test_large_beta_makes_target_equal_to_ref(self):
        logits = torch.tensor([[1.0, 2.0]], dtype=torch.float32)
        mask = torch.tensor([[True, True]])
        teacher_scores = torch.tensor([[10.0, -10.0]], dtype=torch.float32)

        loss = listwise_dpo_loss(logits, logits, teacher_scores, mask, beta=10000.0, temperature=1.0)

        legal_logp = F.log_softmax(logits, dim=-1)
        entropy = -(legal_logp.exp() * legal_logp).sum().item()
        assert loss.item() == pytest.approx(entropy, abs=0.01)

    def test_gradient_flows_through_student_only(self):
        student = torch.tensor([[1.0, 2.0, 3.0]], dtype=torch.float32, requires_grad=True)
        ref = torch.tensor([[1.0, 2.0, 3.0]], dtype=torch.float32)
        mask = torch.tensor([[True, True, True]])
        teacher_scores = torch.tensor([[0.0, 0.0, 5.0]], dtype=torch.float32)

        loss = listwise_dpo_loss(student, ref, teacher_scores, mask, beta=1.0, temperature=1.0)
        loss.backward()

        assert student.grad is not None
        assert torch.isfinite(student.grad).all()

    def test_batch_averages_over_samples(self):
        ref_logits = torch.tensor([[0.0, 0.0], [0.0, 0.0]], dtype=torch.float32)
        student_logits = torch.tensor([[2.0, 0.0], [0.0, 2.0]], dtype=torch.float32)
        mask = torch.tensor([[True, True], [True, True]])
        teacher_scores = torch.tensor([[5.0, 0.0], [0.0, 5.0]], dtype=torch.float32)

        loss = listwise_dpo_loss(student_logits, ref_logits, teacher_scores, mask, beta=1.0, temperature=1.0)

        loss0 = listwise_dpo_loss(
            student_logits[:1], ref_logits[:1], teacher_scores[:1], mask[:1], beta=1.0, temperature=1.0
        )
        loss1 = listwise_dpo_loss(
            student_logits[1:], ref_logits[1:], teacher_scores[1:], mask[1:], beta=1.0, temperature=1.0
        )

        assert loss.item() == pytest.approx((loss0.item() + loss1.item()) / 2.0)


class TestNonDiscardKl:
    def test_identical_outputs_has_zero_kl(self):
        outputs = {
            "discard_logits": torch.zeros(2, 34),
            "claim_logits": torch.tensor([[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]]),
            "self_kong_logits": torch.tensor([[1.0, 0.0, 0.0]]),
            "hu_logits": torch.tensor([[1.0, 0.0]]),
        }
        kl = non_discard_kl(outputs, outputs)
        assert kl.item() < 0.01

    def test_different_outputs_has_positive_kl(self):
        ref = {
            "discard_logits": torch.zeros(2, 34),
            "claim_logits": torch.tensor([[5.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]]),
            "self_kong_logits": torch.tensor([[5.0, 0.0, 0.0]]),
            "hu_logits": torch.tensor([[5.0, 0.0]]),
        }
        student = {
            "discard_logits": torch.zeros(2, 34),
            "claim_logits": torch.tensor([[0.0, 5.0, 0.0, 0.0, 0.0, 0.0, 0.0]]),
            "self_kong_logits": torch.tensor([[0.0, 5.0, 0.0]]),
            "hu_logits": torch.tensor([[0.0, 5.0]]),
        }
        kl = non_discard_kl(student, ref)
        assert kl.item() > 0.5


class TestExpectedRisk:
    def test_student_concentrating_on_low_risk_has_lower_expected_risk(self):
        logits = torch.tensor([[5.0, 0.0]], dtype=torch.float32)
        risk = torch.tensor([[0.9, 0.1]], dtype=torch.float32)
        mask = torch.tensor([[True, True]])

        risky = expected_risk(logits, risk, mask)

        safe_logits = torch.tensor([[0.0, 5.0]], dtype=torch.float32)
        safe = expected_risk(safe_logits, risk, mask)

        assert safe.item() < risky.item()

    def test_masked_actions_excluded_from_risk(self):
        logits = torch.tensor([[5.0, 0.0]], dtype=torch.float32)
        risk = torch.tensor([[0.9, 0.1]], dtype=torch.float32)
        mask = torch.tensor([[True, False]])

        result = expected_risk(logits, risk, mask)
        assert 0.8 < result.item() < 1.0


class TestDpoMetrics:
    def test_top1_when_student_matches_teacher_best(self):
        logits = torch.tensor([[0.0, 0.0, 5.0, 0.0]], dtype=torch.float32)
        teacher_scores = torch.tensor([[0.0, 0.0, 5.0, 0.0]], dtype=torch.float32)
        mask = torch.tensor([[True, True, True, False]])
        best = torch.tensor([2])

        metrics = dpo_metrics(logits, teacher_scores, mask, best)

        assert metrics["teacher_top1"] == pytest.approx(1.0)

    def test_top1_when_student_disagrees(self):
        logits = torch.tensor([[5.0, 0.0, 0.0, 0.0]], dtype=torch.float32)
        teacher_scores = torch.tensor([[0.0, 0.0, 5.0, 0.0]], dtype=torch.float32)
        mask = torch.tensor([[True, True, True, False]])
        best = torch.tensor([2])

        metrics = dpo_metrics(logits, teacher_scores, mask, best)

        assert metrics["teacher_top1"] == pytest.approx(0.0)
        assert metrics["kl_vs_teacher"] > 0.0


class TestCounterfactualDataset:
    def test_loads_rows_and_encodes_shapes(self):
        row = make_cf_row()
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "cf.jsonl"
            write_jsonl([row], path)
            ds = CounterfactualDiscardDataset(path)

        assert len(ds) == 1
        item = ds[0]
        assert item["tile_planes"].shape == (10, 34)
        assert item["legal_mask"].sum().item() == 3
        assert item["teacher_scores"][3].item() == pytest.approx(0.5)
        assert item["teacher_best_index"].item() == 3

    def test_filters_by_policy_id(self):
        learner = make_cf_row(policy_id="learner")
        opponent = make_cf_row(policy_id="opponent")
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "cf.jsonl"
            write_jsonl([opponent, learner], path)
            ds = CounterfactualDiscardDataset(path, policy_id="learner")

        assert len(ds) == 1

    def test_collate_stacks_batch(self):
        rows = [make_cf_row(decision_index=i) for i in range(4)]
        items = [encode_counterfactual_row(row) for row in rows]
        batch = collate_counterfactual(items)

        assert batch["tile_planes"].shape == (4, 10, 34)
        assert batch["legal_mask"].shape == (4, 34)
        assert batch["teacher_best_index"].shape == (4,)


class TestDpoCheckpointCompatibility:
    def test_dpo_checkpoint_loads_with_export_onnx_pattern(self):
        config = ModelConfig()
        model = build_model(config)
        from train_dpo import save_dpo_checkpoint

        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "dpo.pt"
            save_dpo_checkpoint(
                path,
                model,
                config,
                Path(tmp) / "sft.pt",
                {"teacher_top1": 0.95, "dpo_loss": 0.1},
                epoch=1,
                expert_source="sft_logits",
            )
            checkpoint = torch.load(path, map_location="cpu")

        assert checkpoint["training_source"] == "dpo"
        assert checkpoint["expert_source"] == "sft_logits"
        assert "model_state" in checkpoint
        assert "model_config" in checkpoint
