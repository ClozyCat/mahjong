from __future__ import annotations

import argparse
import math
from datetime import UTC, datetime
from pathlib import Path

import torch
import torch.nn.functional as F
from torch.utils.data import DataLoader
from tqdm import tqdm

from awr_dataset import (
    ArenaTrajectoryDataset,
    compute_normalized_advantages,
    split_rows_by_match_id,
)
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
    parser.add_argument("--adv-source", default="value",
                        choices=["value", "terminal", "return"],
                        help="value = return - V(s); terminal = terminal_reward only; return = MC return directly")
    parser.add_argument("--head-weights", default="1.0,3.0,5.0,5.0",
                        help="Comma-separated weights for discard,claim,self_kong,hu")
    parser.add_argument("--kl-coef", type=float, default=0.01,
                        help="KL divergence penalty coefficient against SFT reference")
    parser.add_argument("--fan-distill-coef", type=float, default=0.05,
                        help="MSE distillation loss for qualifying_fan_value against SFT reference")
    parser.add_argument("--fan-value-distill-coef", type=float, default=0.05,
                        help="MSE distillation loss for fan_value against SFT reference")
    parser.add_argument("--value-loss-coef", type=float, default=4.0,
                        help="Weight for value loss in total loss")
    parser.add_argument("--val-fraction", type=float, default=0.1,
                        help="Fraction of match IDs held out for value validation")
    parser.add_argument("--value-lr-multiplier", type=float, default=20.0,
                        help="Value head LR = lr * multiplier for faster value convergence")
    parser.add_argument("--value-finetune-epochs", type=int, default=3,
                        help="After AWR epochs, freeze policy and train value head only for N epochs")
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
    terminal_reward: torch.Tensor | None = None,
    *,
    adv_norm: str,
    adv_source: str = "value",
    temperature: float,
    weight_clip: float,
    policy_filter: str,
) -> tuple[torch.Tensor, torch.Tensor]:
    if adv_source == "terminal" and terminal_reward is not None:
        advantage = terminal_reward.float()
        if adv_norm == "batch":
            adv_mean = advantage.mean()
            adv_std = advantage.std(unbiased=False) + 1e-8
            advantage = (advantage - adv_mean) / adv_std
            advantage = advantage.clamp(-5.0, 5.0)
        elif adv_norm == "none":
            advantage = advantage.clamp(-5.0, 5.0)
    elif adv_source == "return":
        advantage = returns.float()
        if adv_norm == "batch":
            adv_mean = advantage.mean()
            adv_std = advantage.std(unbiased=False) + 1e-8
            advantage = (advantage - adv_mean) / adv_std
            advantage = advantage.clamp(-5.0, 5.0)
        elif adv_norm == "none":
            advantage = advantage.clamp(-5.0, 5.0)
    elif precomputed_advantage is not None and adv_norm in ("per_match", "per_player"):
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


class AwrDiagnostics:
    def __init__(self) -> None:
        self.count = 0
        self.adv_sum = 0.0
        self.adv_sq_sum = 0.0
        self.adv_pos_count = 0
        self.weight_sum = 0.0
        self.weight_max = 0.0
        self.active_weight_count = 0

    def update(self, advantage: torch.Tensor, weights: torch.Tensor) -> None:
        adv = advantage.detach().float().cpu()
        w = weights.detach().float().cpu()
        count = int(adv.numel())
        if count == 0:
            return
        self.count += count
        self.adv_sum += float(adv.sum().item())
        self.adv_sq_sum += float((adv * adv).sum().item())
        self.adv_pos_count += int((adv > 0).sum().item())
        self.weight_sum += float(w.sum().item())
        self.weight_max = max(self.weight_max, float(w.max().item()))
        self.active_weight_count += int((w > 0).sum().item())

    def summary(self) -> dict[str, float]:
        if self.count == 0:
            return {
                "adv_mean": 0.0,
                "adv_std": 0.0,
                "adv_pos_rate": 0.0,
                "weight_mean": 0.0,
                "weight_max": 0.0,
                "active_weight_rate": 0.0,
            }
        adv_mean = self.adv_sum / self.count
        adv_var = max(self.adv_sq_sum / self.count - adv_mean * adv_mean, 0.0)
        return {
            "adv_mean": adv_mean,
            "adv_std": math.sqrt(adv_var),
            "adv_pos_rate": self.adv_pos_count / self.count,
            "weight_mean": self.weight_sum / self.count,
            "weight_max": self.weight_max,
            "active_weight_rate": self.active_weight_count / self.count,
        }


def main() -> None:
    args = parse_args()
    torch.manual_seed(args.seed)
    device = torch.device(args.device if torch.cuda.is_available() else "cpu")
    best_value_ev = -1.0

    full_ds = ArenaTrajectoryDataset(args.trajectories, gamma=args.gamma, policy_id=args.policy_id)
    train_rows, val_rows = split_rows_by_match_id(full_ds.rows, args.val_fraction, args.seed)
    ds = ArenaTrajectoryDataset.from_rows(train_rows, gamma=args.gamma)
    val_ds = ArenaTrajectoryDataset.from_rows(val_rows, gamma=args.gamma) if val_rows else None

    if args.adv_norm in ("per_match", "per_player"):
        values = [float(row.get("value", 0.0)) for row in ds.rows]
        norm_adv = compute_normalized_advantages(
            ds.rows, ds.returns, values, mode=args.adv_norm
        )
        for i, row in enumerate(ds.rows):
            row["advantage"] = norm_adv[i]

    loader = DataLoader(ds, batch_size=args.batch_size, shuffle=True)
    val_loader = (
        DataLoader(val_ds, batch_size=args.batch_size, shuffle=False)
        if val_ds is not None
        else None
    )
    val_variance = (
        torch.tensor(val_ds.returns).float().var().item() + 1e-8
        if val_ds is not None
        else 0.0
    )

    checkpoint = torch.load(args.checkpoint, map_location="cpu")
    model_config = ModelConfig.from_dict(checkpoint.get("model_config", {}))
    model = build_model(model_config).to(device)
    model.load_state_dict(checkpoint["model_state"], strict=True)

    value_head_params = [p for n, p in model.named_parameters() if "value_head" in n and p.requires_grad]
    policy_params = [p for n, p in model.named_parameters() if "value_head" not in n and p.requires_grad]
    optimizer = torch.optim.AdamW([
        {"params": policy_params, "lr": args.lr},
        {"params": value_head_params, "lr": args.lr * args.value_lr_multiplier},
    ])

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
        total_fan_distill_loss = 0.0
        total_fan_value_distill_loss = 0.0
        total_samples = 0
        diagnostics = AwrDiagnostics()

        for batch in tqdm(loader, desc=f"AWR epoch {epoch+1}/{args.epochs}"):
            batch = {k: v.to(device) for k, v in batch.items()}

            outputs = model(
                batch["tile_planes"],
                batch["scalar_features"],
                batch["discard_sequence"],
            )

            value = outputs["value"].squeeze(-1)
            returns = batch["return"].float()
            value_loss = F.mse_loss(value, returns) if args.adv_source == "value" else torch.tensor(0.0, device=device)

            with torch.no_grad():
                weights, _advantage = advantage_weights(
                    returns,
                    value,
                    batch.get("advantage"),
                    terminal_reward=batch.get("terminal_reward"),
                    adv_norm=args.adv_norm,
                    adv_source=args.adv_source,
                    temperature=args.temperature,
                    weight_clip=args.weight_clip,
                    policy_filter=args.policy_filter,
                )
                diagnostics.update(_advantage, weights)

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

            # KL divergence penalty and fan distillation (both use SFT reference)
            kl_loss = torch.tensor(0.0, device=device)
            fan_distill_loss = torch.tensor(0.0, device=device)
            fan_value_distill_loss = torch.tensor(0.0, device=device)
            sft_outputs = None
            if sft_model is not None:
                need_sft = (args.kl_coef > 0 or args.fan_distill_coef > 0
                            or args.fan_value_distill_coef > 0)
                if need_sft:
                    with torch.no_grad():
                        sft_outputs = sft_model(
                            batch["tile_planes"],
                            batch["scalar_features"],
                            batch["discard_sequence"],
                        )
                if args.kl_coef > 0 and sft_outputs is not None:
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
                if args.fan_distill_coef > 0 and sft_outputs is not None:
                    fan_distill_loss = F.mse_loss(
                        outputs["qualifying_fan_value"],
                        sft_outputs["qualifying_fan_value"],
                    )
                if args.fan_value_distill_coef > 0 and sft_outputs is not None:
                    fan_value_distill_loss = F.mse_loss(
                        outputs["fan_value"],
                        sft_outputs["fan_value"],
                    )

            total_loss = (policy_loss + args.value_loss_coef * value_loss
                          + args.kl_coef * kl_loss
                          + args.fan_distill_coef * fan_distill_loss
                          + args.fan_value_distill_coef * fan_value_distill_loss)

            optimizer.zero_grad()
            total_loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), args.grad_clip_norm)
            optimizer.step()

            total_policy_loss += policy_loss.item() if isinstance(policy_loss, torch.Tensor) else 0.0
            total_value_loss += value_loss.item()
            total_kl_loss += kl_loss.item()
            total_fan_distill_loss += fan_distill_loss.item()
            total_fan_value_distill_loss += fan_value_distill_loss.item()
            total_samples += len(batch["return"])

        avg_policy = total_policy_loss / len(loader)
        avg_value = total_value_loss / len(loader)
        avg_kl = total_kl_loss / len(loader)
        avg_fan = total_fan_distill_loss / len(loader)
        avg_fan_value = total_fan_value_distill_loss / len(loader)
        diag_metrics = diagnostics.summary()
        explained_var = 1.0 - avg_value / (torch.tensor(ds.returns).float().var().item() + 1e-8)
        val_mse = evaluate_value_mse(model, val_loader, device) if val_loader is not None else None
        val_ev = 1.0 - val_mse / val_variance if val_mse is not None else None
        val_text = (
            f" val_mse={val_mse:.6f} val_ev={val_ev:.4f}"
            if val_mse is not None and val_ev is not None
            else ""
        )
        selection_ev = val_ev if val_ev is not None else explained_var
        print(
            f"Epoch {epoch+1}: policy_loss={avg_policy:.6f} "
            f"value_mse={avg_value:.6f} value_ev={explained_var:.4f} "
            f"kl_loss={avg_kl:.6f}{val_text} fan_d={avg_fan:.6f} fanv_d={avg_fan_value:.6f} "
            f"adv_mean={diag_metrics['adv_mean']:.4f} adv_std={diag_metrics['adv_std']:.4f} "
            f"adv_pos={diag_metrics['adv_pos_rate']:.4f} weight_mean={diag_metrics['weight_mean']:.4f} "
            f"weight_max={diag_metrics['weight_max']:.4f} active_weight={diag_metrics['active_weight_rate']:.4f} "
            f"samples={total_samples}"
        )

        torch.save(
            {
                "model_state": model.state_dict(),
                "model_config": model_config.to_dict(),
                "training_source": "awr",
                "created_at_utc": datetime.now(UTC).isoformat(),
                "awr_epoch": epoch + 1,
                "awr_metrics": {
                    "policy_loss": avg_policy,
                    "value_mse": avg_value,
                    "value_explained_variance": explained_var,
                    "val_value_mse": val_mse,
                    "val_value_explained_variance": val_ev,
                    "kl_loss": avg_kl,
                    **diag_metrics,
                },
            },
            args.output_dir / f"awr_epoch_{epoch+1:03d}.pt",
        )

        # Track best checkpoint by validation EV when available, otherwise train EV.
        if selection_ev > best_value_ev:
            best_value_ev = selection_ev
            torch.save(
                {
                    "model_state": {k: v.clone() for k, v in model.state_dict().items()},
                    "model_config": model_config.to_dict(),
                    "training_source": "awr",
                    "created_at_utc": datetime.now(UTC).isoformat(),
                    "awr_epoch": epoch + 1,
                    "awr_metrics": {
                        "policy_loss": avg_policy,
                        "value_mse": avg_value,
                        "value_explained_variance": explained_var,
                        "val_value_mse": val_mse,
                        "val_value_explained_variance": val_ev,
                        "kl_loss": avg_kl,
                        **diag_metrics,
                    },
                },
                args.output_dir / "awr_best.pt",
            )

    # Value-only fine-tune: freeze policy, train value head aggressively
    value_finetuned = False
    if args.value_finetune_epochs > 0:
        print("Value fine-tune: freezing policy, training value head...")
        for name, param in model.named_parameters():
            param.requires_grad = "value_head" in name
        value_optimizer = torch.optim.AdamW(
            [p for p in model.parameters() if p.requires_grad],
            lr=args.lr * 50.0,
        )
        for ve in range(args.value_finetune_epochs):
            total_vl = 0.0
            for batch in loader:
                batch = {k: v.to(device) for k, v in batch.items()}
                outputs = model(
                    batch["tile_planes"],
                    batch["scalar_features"],
                    batch["discard_sequence"],
                )
                val = outputs["value"].squeeze(-1)
                ret = batch["return"].float()
                vl = F.mse_loss(val, ret)
                value_optimizer.zero_grad()
                vl.backward()
                value_optimizer.step()
                total_vl += vl.item()
            avg_vl = total_vl / len(loader)
            vev = 1.0 - avg_vl / (torch.tensor(ds.returns).float().var().item() + 1e-8)
            print(f"  value_ft epoch {ve+1}: mse={avg_vl:.6f} ev={vev:.4f}")
            avg_value = avg_vl
            explained_var = vev
            val_mse = evaluate_value_mse(model, val_loader, device) if val_loader is not None else None
            val_ev = 1.0 - val_mse / val_variance if val_mse is not None else None
        value_finetuned = True

    if value_finetuned or best_value_ev < 0:
        torch.save(
            {
                "model_state": model.state_dict(),
                "model_config": model_config.to_dict(),
                "training_source": "awr",
                "created_at_utc": datetime.now(UTC).isoformat(),
                "awr_epoch": args.epochs,
                "awr_metrics": {
                    "policy_loss": avg_policy,
                    "value_mse": avg_value,
                    "value_explained_variance": explained_var,
                    "val_value_mse": val_mse,
                    "val_value_explained_variance": val_ev,
                    "kl_loss": avg_kl,
                    "value_finetune_epochs": args.value_finetune_epochs,
                    **diag_metrics,
                },
            },
            args.output_dir / "awr_best.pt",
        )
        best_value_ev = max(best_value_ev, explained_var)
    print(f"Saved to {args.output_dir} (best value_ev={best_value_ev:.4f})")


def evaluate_value_mse(
    model: torch.nn.Module,
    loader: DataLoader | None,
    device: torch.device,
) -> float | None:
    if loader is None:
        return None
    model.eval()
    total_loss = 0.0
    with torch.no_grad():
        for batch in loader:
            batch = {k: v.to(device) for k, v in batch.items()}
            outputs = model(
                batch["tile_planes"],
                batch["scalar_features"],
                batch["discard_sequence"],
            )
            value = outputs["value"].squeeze(-1)
            returns = batch["return"].float()
            total_loss += F.mse_loss(value, returns).item()
    model.train()
    return total_loss / len(loader)


if __name__ == "__main__":
    main()
