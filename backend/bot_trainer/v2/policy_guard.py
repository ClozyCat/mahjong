from __future__ import annotations

import argparse
import json
from dataclasses import asdict, dataclass
from pathlib import Path

import torch
from torch.utils.data import DataLoader

from counterfactual_dataset import CounterfactualDiscardDataset
from model import ModelConfig, build_model
from train_discard_ranker import collate_counterfactual, masked_log_softmax


@dataclass(frozen=True)
class PolicyGuardMetrics:
    teacher_top1: float
    kl_from_baseline: float


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Offline policy guard against counterfactual teacher regressions")
    parser.add_argument("--baseline-checkpoint", type=Path, required=True)
    parser.add_argument("--candidate-checkpoint", type=Path, required=True)
    parser.add_argument("--counterfactual-discards", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--policy-id", default="learner")
    parser.add_argument("--batch-size", type=int, default=512)
    parser.add_argument("--max-batches", type=int, default=64)
    parser.add_argument("--min-top1-delta", type=float, default=-0.002)
    parser.add_argument("--max-kl", type=float, default=0.03)
    parser.add_argument("--device", default="auto")
    return parser.parse_args()


def evaluate_policy_guard(
    candidate: PolicyGuardMetrics,
    *,
    baseline: PolicyGuardMetrics,
    min_top1_delta: float,
    max_kl: float,
) -> dict:
    failures: list[str] = []
    top1_delta = candidate.teacher_top1 - baseline.teacher_top1
    if top1_delta < min_top1_delta:
        failures.append("teacher_top1_regression")
    if candidate.kl_from_baseline > max_kl:
        failures.append("kl_from_baseline")
    return {
        "accepted": not failures,
        "failures": failures,
        "candidate": asdict(candidate),
        "baseline": asdict(baseline),
        "top1_delta": top1_delta,
        "min_top1_delta": min_top1_delta,
        "max_kl": max_kl,
    }


def resolve_device(value: str) -> torch.device:
    if value == "auto":
        return torch.device("cuda" if torch.cuda.is_available() else "cpu")
    return torch.device(value)


def load_model(path: Path, device: torch.device) -> torch.nn.Module:
    checkpoint = torch.load(path, map_location="cpu")
    model = build_model(ModelConfig.from_dict(checkpoint.get("model_config", {})))
    model.load_state_dict(checkpoint["model_state"], strict=True)
    model.eval()
    return model.to(device)


def masked_categorical_kl_value(
    baseline_logits: torch.Tensor,
    candidate_logits: torch.Tensor,
    mask: torch.Tensor,
) -> torch.Tensor:
    baseline_log_probs = masked_log_softmax(baseline_logits, mask)
    candidate_log_probs = masked_log_softmax(candidate_logits, mask)
    baseline_probs = baseline_log_probs.exp().masked_fill(~mask.bool(), 0.0)
    return (baseline_probs * (baseline_log_probs - candidate_log_probs)).sum(dim=-1)


def compute_policy_metrics(
    *,
    baseline_model: torch.nn.Module,
    candidate_model: torch.nn.Module,
    loader: DataLoader,
    device: torch.device,
    max_batches: int,
) -> tuple[PolicyGuardMetrics, PolicyGuardMetrics]:
    baseline_top1 = 0.0
    candidate_top1 = 0.0
    candidate_kl = 0.0
    count = 0
    with torch.no_grad():
        for batch_index, batch in enumerate(loader):
            if max_batches > 0 and batch_index >= max_batches:
                break
            batch = {key: value.to(device) for key, value in batch.items()}
            baseline_logits = baseline_model(
                batch["tile_planes"].float(),
                batch["scalar_features"].float(),
                batch["discard_sequence"].float(),
            )["discard_logits"]
            candidate_logits = candidate_model(
                batch["tile_planes"].float(),
                batch["scalar_features"].float(),
                batch["discard_sequence"].float(),
            )["discard_logits"]
            legal_mask = batch["legal_mask"].bool()
            target = batch["teacher_best_index"].long()
            batch_size = int(target.shape[0])
            baseline_pred = baseline_logits.masked_fill(~legal_mask, -1.0e9).argmax(dim=-1)
            candidate_pred = candidate_logits.masked_fill(~legal_mask, -1.0e9).argmax(dim=-1)
            baseline_top1 += (baseline_pred == target).float().sum().item()
            candidate_top1 += (candidate_pred == target).float().sum().item()
            candidate_kl += masked_categorical_kl_value(baseline_logits, candidate_logits, legal_mask).sum().item()
            count += batch_size
    baseline = PolicyGuardMetrics(
        teacher_top1=baseline_top1 / max(1, count),
        kl_from_baseline=0.0,
    )
    candidate = PolicyGuardMetrics(
        teacher_top1=candidate_top1 / max(1, count),
        kl_from_baseline=candidate_kl / max(1, count),
    )
    return baseline, candidate


def main() -> None:
    args = parse_args()
    device = resolve_device(args.device)
    dataset = CounterfactualDiscardDataset(args.counterfactual_discards, policy_id=args.policy_id)
    if len(dataset) == 0:
        raise SystemExit(f"No counterfactual discard rows found: {args.counterfactual_discards}")
    loader = DataLoader(
        dataset,
        batch_size=args.batch_size,
        shuffle=False,
        collate_fn=collate_counterfactual,
    )
    baseline_model = load_model(args.baseline_checkpoint, device)
    candidate_model = load_model(args.candidate_checkpoint, device)
    baseline, candidate = compute_policy_metrics(
        baseline_model=baseline_model,
        candidate_model=candidate_model,
        loader=loader,
        device=device,
        max_batches=args.max_batches,
    )
    result = evaluate_policy_guard(
        candidate,
        baseline=baseline,
        min_top1_delta=args.min_top1_delta,
        max_kl=args.max_kl,
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2, ensure_ascii=False), encoding="utf-8")
    print(json.dumps(result, ensure_ascii=False))
    if not result["accepted"]:
        raise SystemExit(2)


if __name__ == "__main__":
    main()
