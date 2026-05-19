from __future__ import annotations

import argparse
import json
from datetime import UTC, datetime
from pathlib import Path

import torch
import torch.nn.functional as F
from torch.utils.data import DataLoader

from model import ModelConfig, build_model, build_actor_critic
from rl_dataset import ArenaTrajectoryDataset, trajectory_diagnostics


DISCARD_BASE_RISK_WEIGHT = 0.90
DISCARD_VALUE_RISK_RANGE = 0.55
DISCARD_VALUE_SCALE = 8.0
DISCARD_MIN_RISK_WEIGHT = 0.25
DISCARD_MAX_RISK_WEIGHT = 1.45


POLICY_CONFIGS = {
    "ppo": {
        "base_risk_weight": 0.90,
        "value_risk_range": 0.55,
        "min_risk_weight": 0.25,
        "max_risk_weight": 1.45,
        "entropy_multiplier": 1.0,
        "description": "PPO 策略：基于自博弈强化学习的生产策略",
    },
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--trajectories", type=Path, required=True)
    parser.add_argument("--checkpoint", type=Path, default=None)
    parser.add_argument("--epochs", type=int, default=1)
    parser.add_argument("--batch-size", type=int, default=256)
    parser.add_argument("--lr", type=float, default=3e-6)
    parser.add_argument("--critic-lr-multiplier", type=float, default=2.0)
    parser.add_argument("--use-actor-critic", action="store_true")
    parser.add_argument("--gamma", type=float, default=0.995)
    parser.add_argument("--gae-lambda", type=float, default=0.95)
    parser.add_argument("--policy-id", default=None)
    parser.add_argument("--clip-epsilon", type=float, default=0.2)
    parser.add_argument("--value-clip-epsilon", type=float, default=0.2)
    parser.add_argument("--entropy-coef", type=float, default=0.02)
    parser.add_argument("--entropy-end-coef", type=float, default=0.005)
    parser.add_argument("--entropy-decay-steps", type=int, default=0)
    parser.add_argument("--kl-coef", type=float, default=0.01)
    parser.add_argument("--kl-end-coef", type=float, default=0.0)
    parser.add_argument("--target-kl", type=float, default=0.03)
    parser.add_argument(
        "--policy",
        choices=["ppo"],
        default="ppo",
        help="训练策略：ppo。运行时用 temperature 区分 bot 行为。",
    )
    parser.add_argument("--recompute-old-policy-stats", action="store_true")
    parser.add_argument("--tensor-cache", type=Path, default=None)
    parser.add_argument("--no-tensor-cache", action="store_true")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--device", default="auto")
    return parser.parse_args()


def masked_head_log_probs(
    logits: torch.Tensor,
    mask: torch.Tensor,
    actions: torch.Tensor,
) -> torch.Tensor:
    masked = logits.masked_fill(~mask.bool(), -1.0e4)
    log_probs = F.log_softmax(masked, dim=1)
    return log_probs.gather(1, actions.long().unsqueeze(1)).squeeze(1)


def risk_adjusted_discard_logits(
    outputs: dict[str, torch.Tensor],
    policy_config: dict[str, float] | None = None,
) -> torch.Tensor:
    discard_logits = outputs["discard_logits"]
    risk_logits = outputs.get("risk_logits")
    value = outputs.get("value")
    if risk_logits is None or value is None:
        return discard_logits

    if policy_config is None:
        base_risk_weight = DISCARD_BASE_RISK_WEIGHT
        value_risk_range = DISCARD_VALUE_RISK_RANGE
        min_risk_weight = DISCARD_MIN_RISK_WEIGHT
        max_risk_weight = DISCARD_MAX_RISK_WEIGHT
    else:
        base_risk_weight = policy_config["base_risk_weight"]
        value_risk_range = policy_config["value_risk_range"]
        min_risk_weight = policy_config["min_risk_weight"]
        max_risk_weight = policy_config["max_risk_weight"]

    values = value.squeeze(-1)
    normalized_value = torch.clamp(values / DISCARD_VALUE_SCALE, -1.0, 1.0)
    risk_weight = torch.clamp(
        base_risk_weight - value_risk_range * normalized_value,
        min_risk_weight,
        max_risk_weight,
    ).unsqueeze(1)
    risk_probability = torch.sigmoid(risk_logits)
    adjusted = discard_logits - risk_weight * risk_probability
    finite = torch.isfinite(discard_logits) & torch.isfinite(risk_logits)
    return torch.where(finite, adjusted, discard_logits)


def head_logits(
    outputs: dict[str, torch.Tensor],
    logits_key: str,
    policy_config: dict[str, float] | None = None,
) -> torch.Tensor:
    if logits_key == "discard_logits":
        return risk_adjusted_discard_logits(outputs, policy_config)
    return outputs[logits_key]


def ppo_policy_loss(
    log_probs: torch.Tensor,
    old_log_probs: torch.Tensor,
    advantages: torch.Tensor,
    clip_epsilon: float,
) -> torch.Tensor:
    ratio = torch.exp(log_probs - old_log_probs)
    clipped = torch.clamp(ratio, 1.0 - clip_epsilon, 1.0 + clip_epsilon)
    return -torch.minimum(ratio * advantages, clipped * advantages).mean()


def entropy_coef_for_progress(
    step: int,
    decay_steps: int,
    start_coef: float,
    end_coef: float,
) -> float:
    if decay_steps <= 0:
        return round(end_coef, 10)
    progress = min(max(step, 0), decay_steps) / decay_steps
    value = start_coef + (end_coef - start_coef) * progress
    return round(value, 10)


def format_epoch_metrics(metrics: dict[str, float], total_epochs: int) -> str:
    base_msg = (
        "RL train epoch "
        f"{int(metrics['epoch'])}/{total_epochs}: "
        f"loss={metrics['loss']:.6f} "
        f"policy_loss={metrics['policy_loss']:.6f} "
        f"value_loss={metrics['value_loss']:.6f} "
        f"entropy={metrics['entropy']:.6f} "
        f"entropy_coef={metrics['entropy_coef']:.6f} "
        f"kl_loss={metrics['kl_loss']:.6f} "
        f"kl_coef={metrics['kl_coef']:.6f} "
        f"approx_kl={metrics.get('approx_kl', 0.0):.6f} "
        f"clip_fraction={metrics.get('clip_fraction', 0.0):.6f} "
        f"value_ev={metrics.get('value_explained_variance', 0.0):.6f}"
    )

    # Add critic-specific metrics if available
    if 'value_mse' in metrics:
        base_msg += f" value_mse={metrics['value_mse']:.6f}"
    if 'advantage_mean' in metrics:
        base_msg += f" adv_mean={metrics['advantage_mean']:.6f}"
    if 'advantage_std' in metrics:
        base_msg += f" adv_std={metrics['advantage_std']:.6f}"

    return base_msg


def epoch_checkpoint_name(epoch: int) -> str:
    return f"epoch_{epoch:03d}.pt"


def checkpoint_payload(
    model: torch.nn.Module,
    model_config: ModelConfig,
    history: list[dict[str, float]],
) -> dict[str, object]:
    return {
        "model_state": model.state_dict(),
        "model_config": model_config.to_dict(),
        "training_source": "rl",
        "created_at_utc": datetime.now(UTC).isoformat(),
        "rl_metrics": history,
    }


def clipped_value_loss(
    values: torch.Tensor,
    old_values: torch.Tensor,
    returns: torch.Tensor,
    clip_epsilon: float,
) -> torch.Tensor:
    clipped = old_values + (values - old_values).clamp(-clip_epsilon, clip_epsilon)
    unclipped_loss = (values - returns).pow(2)
    clipped_loss = (clipped - returns).pow(2)
    return torch.maximum(unclipped_loss, clipped_loss).mean()


def masked_categorical_kl(
    teacher_logits: torch.Tensor,
    student_logits: torch.Tensor,
    mask: torch.Tensor,
) -> torch.Tensor:
    teacher_masked = teacher_logits.masked_fill(~mask.bool(), -1.0e4)
    student_masked = student_logits.masked_fill(~mask.bool(), -1.0e4)
    teacher_log_probs = F.log_softmax(teacher_masked, dim=1)
    student_log_probs = F.log_softmax(student_masked, dim=1)
    teacher_probs = teacher_log_probs.exp()
    return (teacher_probs * (teacher_log_probs - student_log_probs)).sum(dim=1).mean()


def resolve_device(requested: str) -> torch.device:
    if requested == "auto":
        return torch.device("cuda" if torch.cuda.is_available() else "cpu")
    return torch.device(requested)


def load_checkpoint_if_present(model: torch.nn.Module, checkpoint: Path | None) -> None:
    if checkpoint is None:
        return
    if not checkpoint.exists():
        raise SystemExit(
            f"Baseline checkpoint not found: {checkpoint}\n"
            "Run supervised training first, or pass --checkpoint with an existing .pt file."
        )
    payload = torch.load(checkpoint, map_location="cpu")
    state = payload.get("model_state", payload)
    missing, _ = model.load_state_dict(state, strict=False)
    if missing:
        print(f"Checkpoint missing keys (new params initialized fresh): {missing}")


def model_config_from_checkpoint(checkpoint: Path | None) -> ModelConfig:
    if checkpoint is None or not checkpoint.exists():
        return ModelConfig.from_dict({})
    payload = torch.load(checkpoint, map_location="cpu")
    return ModelConfig.from_dict(payload.get("model_config", {}))


def checkpoint_uses_actor_critic(checkpoint: Path) -> bool:
    payload = torch.load(checkpoint, map_location="cpu")
    state = payload.get("model_state", payload)
    return any(key.startswith("actor.") or key.startswith("critic.") for key in state)


def select_action_log_probs(
    outputs: dict[str, torch.Tensor],
    batch: dict[str, torch.Tensor],
    policy_config: dict[str, float] | None = None,
) -> torch.Tensor:
    result = torch.zeros_like(batch["reward"].float())
    heads = [
        (0, "discard_logits", "discard_mask"),
        (1, "claim_logits", "claim_mask"),
        (2, "self_kong_logits", "self_kong_mask"),
        (3, "hu_logits", "hu_mask"),
    ]
    for head_index, logits_key, mask_key in heads:
        active = batch["action_head"] == head_index
        if torch.any(active):
            logits = head_logits(outputs, logits_key, policy_config)
            result[active] = masked_head_log_probs(
                logits[active],
                batch[mask_key][active],
                batch["action_index"][active],
            )
    return result


def forward_model(
    model: torch.nn.Module,
    batch: dict[str, torch.Tensor],
) -> dict[str, torch.Tensor]:
    has_global = batch.get("has_global_state")
    if has_global is not None and torch.any(has_global):
        global_tile_planes = batch.get("global_tile_planes")
        global_scalar_features = batch.get("global_scalar_features")
    else:
        global_tile_planes = None
        global_scalar_features = None

    return model(
        batch["tile_planes"].float(),
        batch["scalar_features"].float(),
        batch["discard_sequence"].float(),
        global_tile_planes=global_tile_planes.float() if global_tile_planes is not None else None,
        global_scalar_features=global_scalar_features.float() if global_scalar_features is not None else None,
    )


def select_action_entropy(
    outputs: dict[str, torch.Tensor],
    batch: dict[str, torch.Tensor],
    policy_config: dict[str, float] | None = None,
) -> torch.Tensor:
    result = torch.zeros_like(batch["reward"].float())
    heads = [
        (0, "discard_logits", "discard_mask"),
        (1, "claim_logits", "claim_mask"),
        (2, "self_kong_logits", "self_kong_mask"),
        (3, "hu_logits", "hu_mask"),
    ]
    for head_index, logits_key, mask_key in heads:
        active = batch["action_head"] == head_index
        if torch.any(active):
            logits = head_logits(outputs, logits_key, policy_config)
            masked = logits[active].masked_fill(
                ~batch[mask_key][active].bool(),
                -1.0e4,
            )
            log_probs = F.log_softmax(masked, dim=1)
            probs = log_probs.exp()
            result[active] = -(probs * log_probs).sum(dim=1)
    return result.mean()


def select_action_head_kl(
    teacher_outputs: dict[str, torch.Tensor],
    student_outputs: dict[str, torch.Tensor],
    batch: dict[str, torch.Tensor],
    policy_config: dict[str, float] | None = None,
) -> torch.Tensor:
    result = torch.zeros((), device=batch["reward"].device)
    count = 0
    heads = [
        (0, "discard_logits", "discard_mask"),
        (1, "claim_logits", "claim_mask"),
        (2, "self_kong_logits", "self_kong_mask"),
        (3, "hu_logits", "hu_mask"),
    ]
    for head_index, logits_key, mask_key in heads:
        active = batch["action_head"] == head_index
        if torch.any(active):
            result = result + masked_categorical_kl(
                head_logits(teacher_outputs, logits_key, policy_config)[active],
                head_logits(student_outputs, logits_key, policy_config)[active],
                batch[mask_key][active],
            )
            count += 1
    return result / max(count, 1)


def value_explained_variance(values: torch.Tensor, returns: torch.Tensor) -> torch.Tensor:
    variance = torch.var(returns, unbiased=False)
    if float(variance.detach().cpu()) <= 1.0e-8:
        return torch.zeros((), device=values.device)
    return 1.0 - torch.var(returns - values, unbiased=False) / (variance + 1.0e-8)


def trajectory_stats_are_all_zero(dataset: ArenaTrajectoryDataset) -> bool:
    if len(dataset) == 0:
        return False
    return all(
        float(row.get("log_prob", 0.0)) == 0.0 and float(row.get("value", 0.0)) == 0.0
        for row in dataset.rows
    )


def build_old_policy_model(
    checkpoint: Path | None,
    device: torch.device,
) -> torch.nn.Module | None:
    if checkpoint is None:
        return None
    model_config = model_config_from_checkpoint(checkpoint)
    if checkpoint_uses_actor_critic(checkpoint):
        model = build_actor_critic(model_config).to(device)
    else:
        model = build_model(model_config).to(device)
    load_checkpoint_if_present(model, checkpoint)
    model.eval()
    for parameter in model.parameters():
        parameter.requires_grad_(False)
    return model


def main() -> None:
    args = parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    device = resolve_device(args.device)

    policy_config = POLICY_CONFIGS[args.policy]
    print(f"Policy: {args.policy} - {policy_config['description']}")

    entropy_multiplier = policy_config["entropy_multiplier"]
    adjusted_entropy_coef = args.entropy_coef * entropy_multiplier
    adjusted_entropy_end_coef = args.entropy_end_coef * entropy_multiplier

    tensor_cache = None
    if not args.no_tensor_cache:
        tensor_cache = args.tensor_cache
        if tensor_cache is None:
            tensor_cache = args.trajectories.with_suffix(args.trajectories.suffix + ".pt")

    dataset = ArenaTrajectoryDataset(
        args.trajectories,
        gamma=args.gamma,
        gae_lambda=args.gae_lambda,
        policy_id=args.policy_id,
        cache_path=tensor_cache,
    )
    if trajectory_stats_are_all_zero(dataset) and not args.recompute_old_policy_stats:
        raise SystemExit(
            "Trajectory old policy stats are all zero. Regenerate trajectories with "
            "log_prob/value, or pass --recompute-old-policy-stats with the rollout checkpoint."
        )
    diagnostics = trajectory_diagnostics(dataset.rows)
    loader = DataLoader(dataset, batch_size=args.batch_size, shuffle=True)
    model_config = model_config_from_checkpoint(args.checkpoint)

    if args.use_actor_critic:
        model = build_actor_critic(model_config).to(device)
        print("Using separate actor-critic architecture with global features")
    else:
        model = build_model(model_config).to(device)
        print("Using shared policy-value network")

    load_checkpoint_if_present(model, args.checkpoint)
    old_policy_model = (
        build_old_policy_model(args.checkpoint, device)
        if args.recompute_old_policy_stats
        else None
    )
    teacher_model = build_old_policy_model(args.checkpoint, device) if args.kl_coef > 0.0 else None

    if args.use_actor_critic:
        actor_params = list(model.actor.parameters())
        critic_params = list(model.critic.parameters())
        critic_lr = args.lr * args.critic_lr_multiplier
        optimizer = torch.optim.AdamW([
            {"params": actor_params, "lr": args.lr},
            {"params": critic_params, "lr": critic_lr},
        ])
        print(f"Using separate optimizers: actor_lr={args.lr:.6f}, critic_lr={critic_lr:.6f}")
    else:
        optimizer = torch.optim.AdamW(model.parameters(), lr=args.lr)

    print(
        "RL train: "
        f"trajectories={len(dataset)} batches={len(loader)} "
        f"epochs={args.epochs} batch_size={args.batch_size} device={device} "
        f"entropy_start={adjusted_entropy_coef:.6f} entropy_end={adjusted_entropy_end_coef:.6f} "
        f"(base={args.entropy_coef:.6f}, multiplier={entropy_multiplier:.2f})"
    )
    print("RL trajectory diagnostics: " + json.dumps(diagnostics, ensure_ascii=False))
    entropy_decay_steps = args.entropy_decay_steps
    if entropy_decay_steps <= 0:
        entropy_decay_steps = max(args.epochs * max(len(loader), 1) - 1, 1)
    history = []
    global_step = 0
    for epoch in range(args.epochs):
        total_loss = 0.0
        total_policy_loss = 0.0
        total_value_loss = 0.0
        total_entropy = 0.0
        total_entropy_coef = 0.0
        total_kl_loss = 0.0
        total_kl_coef = 0.0
        total_approx_kl = 0.0
        total_clip_fraction = 0.0
        total_value_explained_variance = 0.0
        total_value_mse = 0.0
        total_advantage_mean = 0.0
        total_advantage_std = 0.0
        batch_count = 0
        for batch in loader:
            batch = {key: value.to(device) for key, value in batch.items()}
            entropy_coef = entropy_coef_for_progress(
                global_step,
                entropy_decay_steps,
                adjusted_entropy_coef,
                adjusted_entropy_end_coef,
            )
            kl_coef = entropy_coef_for_progress(
                global_step,
                entropy_decay_steps,
                args.kl_coef,
                args.kl_end_coef,
            )
            outputs = forward_model(model, batch)
            if old_policy_model is not None:
                with torch.no_grad():
                    old_outputs = forward_model(old_policy_model, batch)
                    old_log_probs = select_action_log_probs(old_outputs, batch, policy_config)
                    old_values = old_outputs["value"].squeeze(1)
            else:
                old_log_probs = batch["old_log_prob"].float()
                old_values = batch["old_value"].float()
            log_probs = select_action_log_probs(outputs, batch, policy_config)
            returns = batch["return"].float()
            advantages = batch["advantage"].float()

            # Track raw advantage statistics before normalization
            with torch.no_grad():
                raw_advantage_mean = advantages.mean()
                raw_advantage_std = advantages.std(unbiased=False)

            advantages = (advantages - advantages.mean()) / (
                advantages.std(unbiased=False) + 1.0e-8
            )
            policy_loss = ppo_policy_loss(
                log_probs,
                old_log_probs,
                advantages,
                args.clip_epsilon,
            )
            with torch.no_grad():
                ratio = torch.exp(log_probs - old_log_probs)
                approx_kl = (old_log_probs - log_probs).mean()
                clip_fraction = (
                    (torch.abs(ratio - 1.0) > args.clip_epsilon).float().mean()
                )
            values = outputs["value"].squeeze(1)
            value_loss = clipped_value_loss(
                values,
                old_values,
                returns,
                args.value_clip_epsilon,
            )
            entropy = select_action_entropy(outputs, batch, policy_config)
            kl_loss = torch.zeros((), device=device)
            if teacher_model is not None:
                with torch.no_grad():
                    teacher_outputs = forward_model(teacher_model, batch)
                kl_loss = select_action_head_kl(teacher_outputs, outputs, batch, policy_config)
            explained_variance = value_explained_variance(values.detach(), returns)

            # Compute value MSE for monitoring
            with torch.no_grad():
                value_mse = (values - returns).pow(2).mean()

            loss = policy_loss + 0.5 * value_loss - entropy_coef * entropy + kl_coef * kl_loss
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            optimizer.step()
            total_loss += float(loss.detach().cpu())
            total_policy_loss += float(policy_loss.detach().cpu())
            total_value_loss += float(value_loss.detach().cpu())
            total_entropy += float(entropy.detach().cpu())
            total_entropy_coef += entropy_coef
            total_kl_loss += float(kl_loss.detach().cpu())
            total_kl_coef += kl_coef
            total_approx_kl += float(approx_kl.detach().cpu())
            total_clip_fraction += float(clip_fraction.detach().cpu())
            total_value_explained_variance += float(explained_variance.detach().cpu())
            total_value_mse += float(value_mse.detach().cpu())
            total_advantage_mean += float(raw_advantage_mean.detach().cpu())
            total_advantage_std += float(raw_advantage_std.detach().cpu())
            batch_count += 1
            global_step += 1
        epoch_metrics = {
            "epoch": epoch + 1,
            "loss": total_loss / max(batch_count, 1),
            "policy_loss": total_policy_loss / max(batch_count, 1),
            "value_loss": total_value_loss / max(batch_count, 1),
            "entropy": total_entropy / max(batch_count, 1),
            "entropy_coef": total_entropy_coef / max(batch_count, 1),
            "kl_loss": total_kl_loss / max(batch_count, 1),
            "kl_coef": total_kl_coef / max(batch_count, 1),
            "approx_kl": total_approx_kl / max(batch_count, 1),
            "clip_fraction": total_clip_fraction / max(batch_count, 1),
            "value_explained_variance": total_value_explained_variance / max(batch_count, 1),
            "value_mse": total_value_mse / max(batch_count, 1),
            "advantage_mean": total_advantage_mean / max(batch_count, 1),
            "advantage_std": total_advantage_std / max(batch_count, 1),
        }
        history.append(epoch_metrics)
        print(format_epoch_metrics(epoch_metrics, args.epochs))
        torch.save(
            checkpoint_payload(model, model_config, history),
            args.output / epoch_checkpoint_name(epoch + 1),
        )
        if args.target_kl > 0.0 and epoch_metrics["approx_kl"] > args.target_kl:
            history[-1]["early_stop"] = 1.0
            print(
                "RL train early stop: "
                f"approx_kl={epoch_metrics['approx_kl']:.6f} "
                f"target_kl={args.target_kl:.6f}"
            )
            break

    checkpoint_path = args.output / "best.pt"
    payload = checkpoint_payload(model, model_config, history)
    payload["policy"] = args.policy
    payload["policy_config"] = policy_config
    torch.save(payload, checkpoint_path)
    (args.output / "trajectory_diagnostics.json").write_text(
        json.dumps(diagnostics, indent=2, ensure_ascii=False),
        encoding="utf-8",
    )
    (args.output / "rl_metrics.json").write_text(
        json.dumps(history, indent=2),
        encoding="utf-8",
    )
    print(f"RL train saved checkpoint: {checkpoint_path}")
    print(f"Policy: {args.policy}")


if __name__ == "__main__":
    main()

