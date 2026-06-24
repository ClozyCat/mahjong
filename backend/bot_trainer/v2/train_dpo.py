"""Direct Preference Optimization (DPO) trainer for discard preferences.

Replaces the AWR + value-pretrain + discard-ranker pipeline with a single-stage
preference learning step.

Loss formulation (listwise DPO)
-------------------------------
Given a discard position with legal actions *A* and expert preference scores
*r(a)* (``teacher_scores``), the KL-constrained optimal policy is:

    pi*(a|s) proportional to pi_ref(a|s) * exp(r(a) / beta)

In log-space with masked logits:

    target_logp(a) = log_softmax( ref_logits + r / beta )[a]

The training loss is cross-entropy between the student and this target:

    L = -sum_a target_prob(a) * log pi_student(a)

This is equivalent to minimising KL(target || student) where the target
blends the SFT reference with the expert's preference ranking.  When the
expert scores are identical to SFT logits (the degenerate "self-teaching"
case) the target collapses to SFT and the student does not move — a safe
no-op.  When the expert provides *better* signals (e.g. rollout search in
Plan D) the student is pushed toward the expert's preferred discards.

Non-discard heads (claim, self_kong, hu) are kept close to the SFT reference
via an explicit KL penalty, preventing drift through the shared encoder.
"""
from __future__ import annotations

import argparse
import json
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

import torch
import torch.nn.functional as F
from torch.utils.data import DataLoader
from tqdm import tqdm

from counterfactual_dataset import (
    CounterfactualDiscardDataset,
    collate_counterfactual,
)
from model import ModelConfig, build_model


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="DPO trainer for discard preferences")
    parser.add_argument("--counterfactual-discards", type=Path, required=True)
    parser.add_argument("--checkpoint", type=Path, required=True,
                        help="SFT checkpoint (used as starting point and DPO reference)")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--epochs", type=int, default=3)
    parser.add_argument("--batch-size", type=int, default=512)
    parser.add_argument("--lr", type=float, default=2e-5)
    parser.add_argument("--beta", type=float, default=0.5,
                        help="DPO beta: controls how strongly expert scores influence target "
                             "(lower = more conservative, higher = more aggressive)")
    parser.add_argument("--temperature", type=float, default=1.0,
                        help="Temperature applied to student logits before softmax")
    parser.add_argument("--kl-coef", type=float, default=0.05,
                        help="KL penalty coefficient on non-discard heads against SFT reference")
    parser.add_argument("--risk-penalty-weight", type=float, default=0.0,
                        help="Optional penalty on expected risk under student policy")
    parser.add_argument("--grad-clip-norm", type=float, default=1.0)
    parser.add_argument("--policy-id", default="learner")
    parser.add_argument("--expert-source", default="sft_logits",
                        help="Label stored in checkpoint metadata indicating the source of "
                             "teacher_scores (e.g. 'sft_logits', 'rollout_search_1ply')")
    parser.add_argument("--device", default="auto")
    parser.add_argument("--seed", type=int, default=42)
    return parser.parse_args()


def resolve_device(value: str) -> torch.device:
    if value == "auto":
        return torch.device("cuda" if torch.cuda.is_available() else "cpu")
    return torch.device(value)


def listwise_dpo_loss(
    student_logits: torch.Tensor,
    ref_logits: torch.Tensor,
    teacher_scores: torch.Tensor,
    legal_mask: torch.Tensor,
    beta: float,
    temperature: float,
) -> torch.Tensor:
    """Listwise DPO loss on discard preferences.

    target = softmax( ref_logits + teacher_scores / beta )  over legal actions
    loss   = -sum( target_prob * log_softmax( student_logits / T ) )
    """
    student_logits = student_logits / temperature
    mask = legal_mask.bool()

    student_masked = student_logits.masked_fill(~mask, float("-inf"))
    ref_masked = ref_logits.detach().masked_fill(~mask, float("-inf"))

    target_logits = ref_masked + teacher_scores / beta
    target_log_probs = F.log_softmax(target_logits, dim=-1)
    target_probs = target_log_probs.exp()

    student_log_probs = F.log_softmax(student_masked, dim=-1)

    cross_entropy = target_probs * student_log_probs
    cross_entropy = cross_entropy.masked_fill(~mask, 0.0)
    loss = -cross_entropy.sum(dim=-1).mean()
    return loss


def non_discard_kl(
    student_outputs: dict[str, torch.Tensor],
    ref_outputs: dict[str, torch.Tensor],
) -> torch.Tensor:
    """Average KL(student || ref) over claim, self_kong, and hu heads."""
    kl_sum = torch.tensor(0.0, device=student_outputs["discard_logits"].device)
    count = 0
    for head_key in ("claim_logits", "self_kong_logits", "hu_logits"):
        s = F.log_softmax(student_outputs[head_key], dim=-1)
        r = F.softmax(ref_outputs[head_key], dim=-1)
        kl = (r * (r.clamp_min(1e-8).log() - s)).sum(dim=-1).mean()
        kl_sum = kl_sum + kl
        count += 1
    return kl_sum / max(count, 1)


def expected_risk(
    student_logits: torch.Tensor,
    risk_scores: torch.Tensor,
    legal_mask: torch.Tensor,
) -> torch.Tensor:
    """Expected risk under the student's discard policy."""
    mask = legal_mask.bool()
    masked_logits = student_logits.masked_fill(~mask, float("-inf"))
    probs = F.softmax(masked_logits, dim=-1).masked_fill(~mask, 0.0)
    return (probs * risk_scores).sum(dim=-1).mean()


def dpo_metrics(
    student_logits: torch.Tensor,
    teacher_scores: torch.Tensor,
    legal_mask: torch.Tensor,
    teacher_best_index: torch.Tensor,
) -> dict[str, float]:
    """Diagnostics: top-1 agreement with expert, margin, KL."""
    mask = legal_mask.bool()
    masked_logits = student_logits.masked_fill(~mask, float("-inf"))
    pred = masked_logits.argmax(dim=-1)
    top1 = (pred == teacher_best_index).float().mean().item()

    teacher_masked = teacher_scores.masked_fill(~mask, float("-inf"))
    teacher_logp = F.log_softmax(teacher_masked, dim=-1)
    student_logp = F.log_softmax(masked_logits, dim=-1)
    kl_element = teacher_logp.exp() * (teacher_logp - student_logp)
    kl_element = kl_element.masked_fill(~mask, 0.0)
    kl = kl_element.sum(dim=-1).mean().item()

    return {"teacher_top1": top1, "kl_vs_teacher": kl}


def move_batch(batch: dict[str, torch.Tensor], device: torch.device) -> dict[str, torch.Tensor]:
    return {key: value.to(device, non_blocking=True) for key, value in batch.items()}


def restore_non_discard_heads(
    model: torch.nn.Module,
    sft_state: dict[str, torch.Tensor],
) -> None:
    """Restore value_head and other non-discard heads from SFT before saving.

    The DPO gradient flows through the shared encoder and policy trunk,
    which means the value_head and other heads that share this pathway
    will drift even though they are not directly trained.  Restoring the
    value_head ensures the exported model has a clean value estimate for
    the arena's risk evaluation.
    """
    model_state = model.state_dict()
    for key, sft_value in sft_state.items():
        if key.startswith("value_head."):
            if key in model_state and model_state[key].shape == sft_value.shape:
                model_state[key] = sft_value.clone()
    model.load_state_dict(model_state, strict=True)


def save_dpo_checkpoint(
    path: Path,
    model: torch.nn.Module,
    model_config: ModelConfig,
    source_checkpoint: Path,
    metrics: dict[str, Any],
    epoch: int,
    expert_source: str,
) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    torch.save(
        {
            "model_state": {k: v.detach().cpu().clone() for k, v in model.state_dict().items()},
            "model_config": model_config.to_dict(),
            "training_source": "dpo",
            "expert_source": expert_source,
            "source_checkpoint": source_checkpoint.as_posix(),
            "metrics": metrics,
            "epoch": epoch,
            "created_at_utc": datetime.now(UTC).isoformat(),
        },
        path,
    )


def main() -> None:
    args = parse_args()
    torch.manual_seed(args.seed)
    device = resolve_device(args.device)

    dataset = CounterfactualDiscardDataset(args.counterfactual_discards, policy_id=args.policy_id)
    if len(dataset) == 0:
        raise SystemExit(f"No counterfactual discard rows found: {args.counterfactual_discards}")
    loader = DataLoader(
        dataset,
        batch_size=args.batch_size,
        shuffle=True,
        collate_fn=collate_counterfactual,
    )

    checkpoint = torch.load(args.checkpoint, map_location="cpu")
    model_config = ModelConfig.from_dict(checkpoint.get("model_config", {}))
    sft_state = checkpoint["model_state"]

    model = build_model(model_config).to(device)
    model.load_state_dict(sft_state, strict=True)

    ref_model = build_model(model_config).to(device)
    ref_model.load_state_dict(sft_state, strict=True)
    ref_model.eval()
    for p in ref_model.parameters():
        p.requires_grad = False

    optimizer = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=0.01)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    best_top1 = -1.0
    best_metrics: dict[str, Any] = {}

    for epoch in range(1, args.epochs + 1):
        model.train()
        total_dpo_loss = 0.0
        total_kl_loss = 0.0
        total_risk_loss = 0.0
        total_top1 = 0.0
        total_kl_teacher = 0.0
        count = 0

        pbar = tqdm(loader, desc=f"DPO epoch {epoch}/{args.epochs}", dynamic_ncols=True)
        for batch in pbar:
            batch = move_batch(batch, device)

            outputs = model(
                batch["tile_planes"].float(),
                batch["scalar_features"].float(),
                batch["discard_sequence"].float(),
            )
            with torch.no_grad():
                ref_outputs = ref_model(
                    batch["tile_planes"].float(),
                    batch["scalar_features"].float(),
                    batch["discard_sequence"].float(),
                )

            dpo_loss = listwise_dpo_loss(
                outputs["discard_logits"],
                ref_outputs["discard_logits"],
                batch["teacher_scores"],
                batch["legal_mask"],
                beta=args.beta,
                temperature=args.temperature,
            )
            kl_loss = non_discard_kl(outputs, ref_outputs)

            risk_loss = torch.tensor(0.0, device=device)
            if args.risk_penalty_weight > 0:
                risk_loss = expected_risk(
                    outputs["discard_logits"],
                    batch["risk_scores"],
                    batch["legal_mask"],
                )

            loss = dpo_loss + args.kl_coef * kl_loss + args.risk_penalty_weight * risk_loss

            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            if args.grad_clip_norm > 0:
                torch.nn.utils.clip_grad_norm_(model.parameters(), args.grad_clip_norm)
            optimizer.step()

            metrics = dpo_metrics(
                outputs["discard_logits"],
                batch["teacher_scores"],
                batch["legal_mask"],
                batch["teacher_best_index"],
            )
            batch_size = int(batch["teacher_best_index"].shape[0])
            total_dpo_loss += dpo_loss.item() * batch_size
            total_kl_loss += kl_loss.item() * batch_size
            total_risk_loss += risk_loss.item() * batch_size
            total_top1 += metrics["teacher_top1"] * batch_size
            total_kl_teacher += metrics["kl_vs_teacher"] * batch_size
            count += batch_size

            pbar.set_postfix({
                "dpo": f"{total_dpo_loss / max(1, count):.4f}",
                "top1": f"{total_top1 / max(1, count):.3f}",
                "kl_t": f"{total_kl_teacher / max(1, count):.4f}",
            })

        epoch_metrics = {
            "epoch": epoch,
            "dpo_loss": total_dpo_loss / max(1, count),
            "kl_loss": total_kl_loss / max(1, count),
            "risk_loss": total_risk_loss / max(1, count),
            "teacher_top1": total_top1 / max(1, count),
            "kl_vs_teacher": total_kl_teacher / max(1, count),
            "lr": args.lr,
            "beta": args.beta,
            "temperature": args.temperature,
            "kl_coef": args.kl_coef,
            "expert_source": args.expert_source,
            "samples": count,
        }
        print(json.dumps(epoch_metrics, ensure_ascii=False))

        if epoch_metrics["teacher_top1"] > best_top1:
            best_top1 = epoch_metrics["teacher_top1"]
            best_metrics = dict(epoch_metrics)
            restore_non_discard_heads(model, sft_state)
            save_dpo_checkpoint(
                args.output,
                model,
                model_config,
                args.checkpoint,
                best_metrics,
                epoch,
                args.expert_source,
            )

    print(f"Saved DPO checkpoint to {args.output} (best teacher_top1={best_top1:.4f})")


if __name__ == "__main__":
    main()
