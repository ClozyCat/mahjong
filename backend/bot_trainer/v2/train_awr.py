from __future__ import annotations

import argparse
from datetime import UTC, datetime
from pathlib import Path

import torch
import torch.nn.functional as F
from torch.utils.data import DataLoader
from tqdm import tqdm

from awr_dataset import ArenaTrajectoryDataset, compute_normalized_advantages
from model import ModelConfig, build_model


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--trajectories", type=Path, required=True)
    parser.add_argument("--checkpoint", type=Path, required=True,
                        help="Checkpoint with pretrained actor + value head")
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--epochs", type=int, default=5)
    parser.add_argument("--batch-size", type=int, default=512)
    parser.add_argument("--lr", type=float, default=3e-5)
    parser.add_argument("--gamma", type=float, default=0.995)
    parser.add_argument("--temperature", type=float, default=0.5,
                        help="AWR temperature for exp(adv/T)")
    parser.add_argument("--weight-clip", type=float, default=20.0,
                        help="Max advantage weight")
    parser.add_argument("--policy-filter", default="positive",
                        choices=["all", "positive"],
                        help="positive = only samples with adv>0; all = all samples")
    parser.add_argument("--adv-norm", default="per_match",
                        choices=["none", "per_match", "per_player", "batch"],
                        help="Advantage normalization mode")
    parser.add_argument("--head-weights", default="1.0,3.0,5.0,5.0",
                        help="Comma-separated weights for discard,claim,self_kong,hu")
    parser.add_argument("--kl-coef", type=float, default=0.01,
                        help="KL divergence penalty coefficient against SFT reference")
    parser.add_argument("--sft-checkpoint", type=Path, default=None,
                        help="SFT checkpoint for KL reference; defaults to --checkpoint")
    parser.add_argument("--policy-id", default=None)
    parser.add_argument("--device", default="cuda")
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--grad-clip-norm", type=float, default=1.0)
    return parser.parse_args()


def compute_ce_loss_for_action(
    logits: torch.Tensor,
    mask: torch.Tensor,
    action_index: torch.Tensor,
    weights: torch.Tensor,
) -> torch.Tensor:
    active = mask.any(dim=-1) & (weights > 0)
    if not active.any():
        return logits.sum() * 0.0
    logits = logits[active]
    mask = mask[active]
    action_index = action_index[active]
    weights = weights[active]
    masked = logits.clone()
    masked[~mask] = float("-inf")
    log_probs = F.log_softmax(masked, dim=-1)
    nll = -log_probs[range(len(action_index)), action_index]
    return (nll * weights).sum() / weights.numel()


def advantage_weights(
    returns: torch.Tensor,
    values: torch.Tensor,
    precomputed_advantage: torch.Tensor | None,
    *,
    adv_norm: str,
    temperature: float,
    weight_clip: float,
    policy_filter: str,
) -> tuple[torch.Tensor, torch.Tensor]:
    if precomputed_advantage is not None and adv_norm in ("per_match", "per_player"):
        advantage = precomputed_advantage.float()
    else:
        advantage = returns - values.detach()
        if adv_norm == "batch":
            adv_mean = advantage.mean()
            adv_std = advantage.std(unbiased=False) + 1e-8
            advantage = (advantage - adv_mean) / adv_std
            advantage = advantage.clamp(-5.0, 5.0)
        elif adv_norm == "none":
            advantage = advantage.clamp(-5.0, 5.0)
    weights = torch.exp(advantage / temperature).clamp(max=weight_clip)
    if policy_filter == "positive":
        weights = torch.where(advantage > 0, weights, torch.zeros_like(weights))
    return weights, advantage


def masked_categorical_kl(
    teacher_logits: torch.Tensor,
    student_logits: torch.Tensor,
    mask: torch.Tensor,
) -> torch.Tensor:
    valid_rows = mask.any(dim=-1)
    if not valid_rows.any():
        return teacher_logits.sum() * 0.0 + student_logits.sum() * 0.0
    teacher_logits = teacher_logits[valid_rows]
    student_logits = student_logits[valid_rows]
    mask = mask[valid_rows]
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


def main() -> None:
    args = parse_args()
    torch.manual_seed(args.seed)
    device = torch.device(args.device if torch.cuda.is_available() else "cpu")

    ds = ArenaTrajectoryDataset(args.trajectories, gamma=args.gamma, policy_id=args.policy_id)

    if args.adv_norm in ("per_match", "per_player"):
        values = [float(row.get("value", 0.0)) for row in ds.rows]
        norm_adv = compute_normalized_advantages(
            ds.rows, ds.returns, values, mode=args.adv_norm
        )
        for i, row in enumerate(ds.rows):
            row["advantage"] = norm_adv[i]

    loader = DataLoader(ds, batch_size=args.batch_size, shuffle=True)

    checkpoint = torch.load(args.checkpoint, map_location="cpu")
    model_config = ModelConfig.from_dict(checkpoint.get("model_config", {}))
    model = build_model(model_config).to(device)
    model.load_state_dict(checkpoint["model_state"], strict=True)
    optimizer = torch.optim.AdamW(model.parameters(), lr=args.lr)

    sft_model = None
    if args.kl_coef > 0:
        sft_path = args.sft_checkpoint or args.checkpoint
        sft_checkpoint = torch.load(sft_path, map_location="cpu")
        sft_model = build_model(model_config).to(device)
        sft_model.load_state_dict(sft_checkpoint["model_state"], strict=True)
        sft_model.eval()
        for p in sft_model.parameters():
            p.requires_grad = False

    head_weights = [float(w) for w in args.head_weights.split(",")]
    if len(head_weights) != 4:
        raise ValueError("--head-weights must have exactly 4 values (discard,claim,self_kong,hu)")

    args.output_dir.mkdir(parents=True, exist_ok=True)

    head_logits_keys = ["discard_logits", "claim_logits", "self_kong_logits", "hu_logits"]
    head_mask_keys = ["discard_mask", "claim_mask", "self_kong_mask", "hu_mask"]

    for epoch in range(args.epochs):
        model.train()
        total_policy_loss = 0.0
        total_value_loss = 0.0
        total_kl_loss = 0.0
        total_samples = 0

        for batch in tqdm(loader, desc=f"AWR epoch {epoch+1}/{args.epochs}"):
            batch = {k: v.to(device) for k, v in batch.items()}

            outputs = model(
                batch["tile_planes"],
                batch["scalar_features"],
                batch["discard_sequence"],
            )

            value = outputs["value"].squeeze(-1)
            returns = batch["return"].float()
            value_loss = F.mse_loss(value, returns)

            with torch.no_grad():
                weights, _advantage = advantage_weights(
                    returns,
                    value,
                    batch.get("advantage"),
                    adv_norm=args.adv_norm,
                    temperature=args.temperature,
                    weight_clip=args.weight_clip,
                    policy_filter=args.policy_filter,
                )

            action_head = batch["action_head"]

            policy_loss = torch.tensor(0.0, device=device)
            weight_sum = 0.0

            for head_idx in range(4):
                mask_t = action_head == head_idx
                if not mask_t.any():
                    continue
                loss = compute_ce_loss_for_action(
                    outputs[head_logits_keys[head_idx]][mask_t],
                    batch[head_mask_keys[head_idx]][mask_t],
                    batch["action_index"][mask_t],
                    weights[mask_t],
                )
                policy_loss = policy_loss + head_weights[head_idx] * loss
                weight_sum += head_weights[head_idx]

            if weight_sum > 0:
                policy_loss = policy_loss / weight_sum

            kl_loss = torch.tensor(0.0, device=device)
            if sft_model is not None and args.kl_coef > 0:
                with torch.no_grad():
                    sft_outputs = sft_model(
                        batch["tile_planes"],
                        batch["scalar_features"],
                        batch["discard_sequence"],
                    )
                kl_parts = []
                for head_idx in range(4):
                    kl_parts.append(
                        masked_categorical_kl(
                            sft_outputs[head_logits_keys[head_idx]],
                            outputs[head_logits_keys[head_idx]],
                            batch[head_mask_keys[head_idx]],
                        )
                    )
                kl_loss = sum(kl_parts) / 4.0

            total_loss = policy_loss + 0.5 * value_loss + args.kl_coef * kl_loss

            optimizer.zero_grad()
            total_loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), args.grad_clip_norm)
            optimizer.step()

            total_policy_loss += policy_loss.item() if isinstance(policy_loss, torch.Tensor) else 0.0
            total_value_loss += value_loss.item()
            total_kl_loss += kl_loss.item()
            total_samples += len(batch["return"])

        print(
            f"Epoch {epoch+1}: policy_loss={total_policy_loss/len(loader):.6f} "
            f"value_loss={total_value_loss/len(loader):.6f} "
            f"kl_loss={total_kl_loss/len(loader):.6f} "
            f"samples={total_samples}"
        )

        torch.save(
            {
                "model_state": model.state_dict(),
                "model_config": model_config.to_dict(),
                "training_source": "awr",
                "created_at_utc": datetime.now(UTC).isoformat(),
                "awr_epoch": epoch + 1,
            },
            args.output_dir / f"awr_epoch_{epoch+1:03d}.pt",
        )

    torch.save(
        {
            "model_state": model.state_dict(),
            "model_config": model_config.to_dict(),
            "training_source": "awr",
            "created_at_utc": datetime.now(UTC).isoformat(),
        },
        args.output_dir / "awr_best.pt",
    )
    print(f"Saved to {args.output_dir}")


if __name__ == "__main__":
    main()
