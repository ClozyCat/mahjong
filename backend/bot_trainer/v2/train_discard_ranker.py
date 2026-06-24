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

from counterfactual_dataset import CounterfactualDiscardDataset
from model import ModelConfig, build_model
from train import resolve_device


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--checkpoint", type=Path, required=True)
    parser.add_argument("--counterfactual-discards", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--epochs", type=int, default=2)
    parser.add_argument("--batch-size", type=int, default=256)
    parser.add_argument("--lr", type=float, default=1e-5)
    parser.add_argument("--temperature", type=float, default=1.5)
    parser.add_argument("--top1-weight", type=float, default=0.1)
    parser.add_argument("--grad-clip-norm", type=float, default=1.0)
    parser.add_argument("--policy-id", default="learner")
    parser.add_argument("--device", default="auto")
    return parser.parse_args()


def collate_counterfactual(batch: list[dict[str, torch.Tensor]]) -> dict[str, torch.Tensor]:
    return {key: torch.stack([item[key] for item in batch]) for key in batch[0]}


def masked_log_softmax(logits: torch.Tensor, legal_mask: torch.Tensor) -> torch.Tensor:
    masked = logits.float().masked_fill(~legal_mask.bool(), -1.0e9)
    return F.log_softmax(masked, dim=-1)


def compute_ranker_loss(
    outputs: dict[str, torch.Tensor],
    batch: dict[str, torch.Tensor],
    temperature: float,
    top1_weight: float,
) -> torch.Tensor:
    legal_mask = batch["legal_mask"].bool()
    student_log_probs = masked_log_softmax(outputs["discard_logits"] / temperature, legal_mask)
    teacher_scores = batch["teacher_scores"].float().masked_fill(~legal_mask, -1.0e9)
    teacher_probs = F.softmax(teacher_scores / temperature, dim=-1)
    kl_loss = F.kl_div(student_log_probs, teacher_probs, reduction="batchmean") * (temperature**2)
    if top1_weight <= 0:
        return kl_loss
    top1_loss = F.nll_loss(student_log_probs, batch["teacher_best_index"].long())
    return kl_loss + top1_weight * top1_loss


def ranker_metrics(
    outputs: dict[str, torch.Tensor],
    batch: dict[str, torch.Tensor],
) -> dict[str, float]:
    legal_mask = batch["legal_mask"].bool()
    masked_logits = outputs["discard_logits"].float().masked_fill(~legal_mask, -1.0e9)
    pred = masked_logits.argmax(dim=-1)
    target = batch["teacher_best_index"].long()
    top1 = (pred == target).float().mean().item()
    target_one_hot = torch.nn.functional.one_hot(target, masked_logits.shape[-1]).bool()
    competitor_mask = legal_mask & ~target_one_hot
    competitor_logits = masked_logits.masked_fill(~competitor_mask, -1.0e9)
    competitor_best = competitor_logits.max(dim=-1).values
    target_logits = masked_logits.gather(1, target.unsqueeze(1)).squeeze(1)
    has_competitor = competitor_mask.any(dim=-1)
    if has_competitor.any():
        margin = (target_logits[has_competitor] - competitor_best[has_competitor]).mean().item()
    else:
        margin = 0.0
    return {"top1": top1, "target_margin": margin}


def move_batch(batch: dict[str, torch.Tensor], device: torch.device) -> dict[str, torch.Tensor]:
    return {key: value.to(device, non_blocking=True) for key, value in batch.items()}


def run_epoch(
    model: torch.nn.Module,
    loader: DataLoader,
    optimizer: torch.optim.Optimizer | None,
    device: torch.device,
    temperature: float,
    top1_weight: float,
    grad_clip_norm: float,
    desc: str,
) -> dict[str, float]:
    is_training = optimizer is not None
    model.train(is_training)
    loss_sum = 0.0
    top1_sum = 0.0
    margin_sum = 0.0
    count = 0
    pbar = tqdm(loader, desc=desc, dynamic_ncols=True)
    for batch in pbar:
        batch = move_batch(batch, device)
        with torch.set_grad_enabled(is_training):
            outputs = model(
                batch["tile_planes"].float(),
                batch["scalar_features"].float(),
                batch["discard_sequence"].float(),
            )
            loss = compute_ranker_loss(outputs, batch, temperature, top1_weight)
            if is_training:
                optimizer.zero_grad(set_to_none=True)
                loss.backward()
                if grad_clip_norm > 0:
                    torch.nn.utils.clip_grad_norm_(model.parameters(), grad_clip_norm)
                optimizer.step()
        metrics = ranker_metrics(outputs, batch)
        batch_size = int(batch["teacher_best_index"].shape[0])
        loss_sum += loss.item() * batch_size
        top1_sum += metrics["top1"] * batch_size
        margin_sum += metrics["target_margin"] * batch_size
        count += batch_size
        pbar.set_postfix({"loss": f"{loss_sum / max(1, count):.4f}", "top1": f"{top1_sum / max(1, count):.3f}"})
    return {
        "loss": loss_sum / max(1, count),
        "top1": top1_sum / max(1, count),
        "target_margin": margin_sum / max(1, count),
    }


def load_model(checkpoint_path: Path, device: torch.device) -> tuple[torch.nn.Module, ModelConfig, dict[str, Any]]:
    checkpoint = torch.load(checkpoint_path, map_location="cpu")
    model_config = ModelConfig.from_dict(checkpoint.get("model_config", {}))
    model = build_model(model_config)
    model.load_state_dict(checkpoint["model_state"], strict=True)
    return model.to(device), model_config, checkpoint


def save_ranker_checkpoint(
    path: Path,
    model: torch.nn.Module,
    model_config: ModelConfig,
    source_checkpoint: Path,
    metrics: dict[str, float],
    epoch: int,
) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    torch.save(
        {
            "model_state": model.state_dict(),
            "metadata": {
                "source_checkpoint": source_checkpoint.as_posix(),
                "training_source": "counterfactual_discard_ranker",
            },
            "metrics": metrics,
            "epoch": epoch,
            "model_config": model_config.to_dict(),
            "training_source": "counterfactual_discard_ranker",
            "created_at_utc": datetime.now(UTC).isoformat(),
        },
        path,
    )


def main() -> None:
    args = parse_args()
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
    model, model_config, _checkpoint = load_model(args.checkpoint, device)
    optimizer = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=0.01)
    best_metrics: dict[str, float] | None = None
    for epoch in range(1, args.epochs + 1):
        metrics = run_epoch(
            model,
            loader,
            optimizer,
            device,
            args.temperature,
            args.top1_weight,
            args.grad_clip_norm,
            f"Discard ranker epoch {epoch}/{args.epochs}",
        )
        print(json.dumps({"epoch": epoch, **metrics}, ensure_ascii=False))
        if best_metrics is None or metrics["top1"] > best_metrics["top1"]:
            best_metrics = dict(metrics)
            save_ranker_checkpoint(args.output, model, model_config, args.checkpoint, metrics, epoch)


if __name__ == "__main__":
    main()
