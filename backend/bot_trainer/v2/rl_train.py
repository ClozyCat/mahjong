from __future__ import annotations

import argparse
import json
import math
from datetime import UTC, datetime
from pathlib import Path

import torch
import torch.nn.functional as F
from torch.utils.data import DataLoader

from amp_config import AmpConfig, amp_dtype_name, resolve_amp_config
from model import ModelConfig, build_model, build_actor_critic
from rl_dataset import ArenaTrajectoryDataset, trajectory_diagnostics


DISCARD_BASE_RISK_WEIGHT = 0.90
DISCARD_VALUE_RISK_RANGE = 0.55
DISCARD_VALUE_SCALE = 8.0
DISCARD_MIN_RISK_WEIGHT = 0.25
DISCARD_MAX_RISK_WEIGHT = 1.45


class ReplayBuffer:
    def __init__(self, max_epochs: int = 3) -> None:
        self.max_epochs = max_epochs
        self.buffer: list[list[dict[str, torch.Tensor]]] = []

    def add_epoch(self, batches: list[dict[str, torch.Tensor]]) -> None:
        self.buffer.append([{k: v.cpu() for k, v in b.items()} for b in batches])
        if len(self.buffer) > self.max_epochs:
            self.buffer.pop(0)

    def sample(self, n_batches: int) -> list[dict[str, torch.Tensor]]:
        if not self.buffer or n_batches <= 0:
            return []
        all_batches = [b for epoch_batches in self.buffer for b in epoch_batches]
        if n_batches >= len(all_batches):
            return all_batches
        indices = torch.randperm(len(all_batches))[:n_batches].tolist()
        return [all_batches[i] for i in indices]


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
    parser.add_argument("--lr-warmup-epochs", type=int, default=3)
    parser.add_argument("--critic-lr-multiplier", type=float, default=2.0)
    parser.add_argument("--use-actor-critic", action="store_true")
    parser.add_argument("--gamma", type=float, default=0.995)
    parser.add_argument("--gae-lambda", type=float, default=0.95)
    parser.add_argument("--policy-id", default=None)
    parser.add_argument("--clip-epsilon", type=float, default=0.15)
    parser.add_argument("--value-clip-epsilon", type=float, default=0.2)
    parser.add_argument("--opponent-loss-coef", type=float, default=0.05)
    parser.add_argument("--entropy-coef", type=float, default=0.03)
    parser.add_argument("--entropy-end-coef", type=float, default=0.008)
    parser.add_argument("--entropy-decay-mode", choices=["linear", "cosine"], default="cosine")
    parser.add_argument("--kl-coef", type=float, default=0.01)
    parser.add_argument("--kl-target", type=float, default=0.02)
    parser.add_argument("--kl-adaptive", action="store_true", default=True)
    parser.add_argument("--target-kl", type=float, default=0.04)
    parser.add_argument("--replay-buffer-epochs", type=int, default=3)
    parser.add_argument("--replay-ratio", type=float, default=0.4)
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
    amp_group = parser.add_mutually_exclusive_group()
    amp_group.add_argument(
        "--amp",
        dest="amp",
        action="store_true",
        default=True,
        help="Enable BF16 automatic mixed precision on supported CUDA devices. Enabled by default.",
    )
    amp_group.add_argument(
        "--no-amp",
        dest="amp",
        action="store_false",
        help="Disable automatic mixed precision.",
    )
    return parser.parse_args()


def masked_head_log_probs(
    logits: torch.Tensor,
    mask: torch.Tensor,
    actions: torch.Tensor,
) -> torch.Tensor:
    masked = logits.masked_fill(~mask.bool(), -1.0e4)
    log_probs = F.log_softmax(masked, dim=1)
    return log_probs.gather(1, actions.long().unsqueeze(1)).squeeze(1)


def discard_value_for_risk_adjustment(
    outputs: dict[str, torch.Tensor],
    policy_config: dict[str, object] | None,
) -> torch.Tensor | None:
    value = outputs.get("value_for_risk")
    if value is None:
        return None
    return value


def risk_adjusted_discard_logits(
    outputs: dict[str, torch.Tensor],
    policy_config: dict[str, object] | None = None,
) -> torch.Tensor:
    discard_logits = outputs["discard_logits"]
    opponent_risk_logits = outputs.get("opponent_risk_logits")
    value = discard_value_for_risk_adjustment(outputs, policy_config)
    if opponent_risk_logits is None or value is None:
        return discard_logits

    if policy_config is None:
        base_risk_weight = DISCARD_BASE_RISK_WEIGHT
        value_risk_range = DISCARD_VALUE_RISK_RANGE
        min_risk_weight = DISCARD_MIN_RISK_WEIGHT
        max_risk_weight = DISCARD_MAX_RISK_WEIGHT
    else:
        base_risk_weight = float(policy_config["base_risk_weight"])
        value_risk_range = float(policy_config["value_risk_range"])
        min_risk_weight = float(policy_config["min_risk_weight"])
        max_risk_weight = float(policy_config["max_risk_weight"])

    values = value.squeeze(-1)
    normalized_value = torch.clamp(values / DISCARD_VALUE_SCALE, -1.0, 1.0)
    risk_weight = torch.clamp(
        base_risk_weight - value_risk_range * normalized_value,
        min_risk_weight,
        max_risk_weight,
    ).unsqueeze(1)

    aggregated_risk = torch.sigmoid(opponent_risk_logits).max(dim=1)[0]
    adjusted = discard_logits - risk_weight * aggregated_risk
    finite = torch.isfinite(discard_logits) & torch.isfinite(aggregated_risk)
    return torch.where(finite, adjusted, discard_logits)


def head_logits(
    outputs: dict[str, torch.Tensor],
    logits_key: str,
    policy_config: dict[str, object] | None = None,
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
    total_steps: int,
    start_coef: float,
    end_coef: float,
    mode: str = "linear",
) -> float:
    if total_steps <= 0:
        return round(end_coef, 10)
    progress = min(max(step, 0), total_steps) / total_steps
    if mode == "cosine":
        value = end_coef + (start_coef - end_coef) * 0.5 * (1.0 + math.cos(progress * math.pi))
    else:
        value = start_coef + (end_coef - start_coef) * progress
    return round(value, 10)


def lr_warmup_multiplier(epoch: int, warmup_epochs: int) -> float:
    if warmup_epochs <= 0:
        return 1.0
    return min(1.0, (epoch + 1) / warmup_epochs)


def apply_lr_warmup(
    optimizer: torch.optim.Optimizer,
    epoch: int,
    warmup_epochs: int,
    actor_lr: float,
    critic_lr_multiplier: float,
) -> None:
    lr_mult = lr_warmup_multiplier(epoch, warmup_epochs)
    for param_group in optimizer.param_groups:
        group_name = param_group.get("name", "actor")
        base_lr = actor_lr * critic_lr_multiplier if group_name == "critic" else actor_lr
        param_group["lr"] = base_lr * lr_mult


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
        f"opponent_loss={metrics.get('opponent_loss', 0.0):.6f} "
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
    try:
        model.load_state_dict(state, strict=True)
    except RuntimeError as exc:
        raise SystemExit(
            f"Checkpoint state does not match the current model contract: {checkpoint}\n"
            "Regenerate actor-critic checkpoints with bootstrap_actor_critic_checkpoint.py "
            "or choose a checkpoint produced by the current code."
        ) from exc


def model_config_from_checkpoint(checkpoint: Path | None) -> ModelConfig:
    if checkpoint is None or not checkpoint.exists():
        return ModelConfig.from_dict({})
    payload = torch.load(checkpoint, map_location="cpu")
    return ModelConfig.from_dict(payload.get("model_config", {}))


def checkpoint_uses_actor_critic(checkpoint: Path) -> bool:
    payload = torch.load(checkpoint, map_location="cpu")
    state = payload.get("model_state", payload)
    return any(key.startswith("actor.") or key.startswith("critic.") for key in state)


def validate_checkpoint_architecture(
    checkpoint: Path | None,
    use_actor_critic: bool,
) -> None:
    if checkpoint is None:
        return
    if not checkpoint.exists():
        return

    uses_actor_critic = checkpoint_uses_actor_critic(checkpoint)
    if use_actor_critic and not uses_actor_critic:
        raise SystemExit(
            "Checkpoint architecture mismatch: --use-actor-critic requires an "
            "actor-critic checkpoint, but this checkpoint looks like an older "
            "shared policy/SFT checkpoint. Refusing to train because fresh "
            "actor/critic parameters would affect results."
        )
    if not use_actor_critic and uses_actor_critic:
        raise SystemExit(
            "Checkpoint architecture mismatch: shared policy training requires "
            "a shared policy checkpoint, but this checkpoint uses actor-critic "
            "state keys. Pass --use-actor-critic or choose a matching checkpoint."
        )


def select_action_log_probs(
    outputs: dict[str, torch.Tensor],
    batch: dict[str, torch.Tensor],
    policy_config: dict[str, object] | None = None,
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
    return_both_critics: bool = False,
) -> dict[str, torch.Tensor]:
    has_global = batch.get("has_global_state")
    if has_global is not None and torch.any(has_global):
        if not torch.all(has_global):
            raise ValueError(
                "Mixed batches with and without global state are not supported. "
                "Use trajectories exported with global state for actor-critic PPO."
            )
        global_tile_planes = batch.get("global_tile_planes")
        global_scalar_features = batch.get("global_scalar_features")
    else:
        global_tile_planes = None
        global_scalar_features = None

    forward_kwargs = {
        "tile_planes": batch["tile_planes"].float(),
        "scalar_features": batch["scalar_features"].float(),
        "discard_sequence": batch["discard_sequence"].float(),
        "global_tile_planes": global_tile_planes.float() if global_tile_planes is not None else None,
        "global_scalar_features": global_scalar_features.float() if global_scalar_features is not None else None,
    }

    if return_both_critics and hasattr(model, "critic"):
        forward_kwargs["return_both_critics"] = True

    return model(**forward_kwargs)


def select_action_entropy(
    outputs: dict[str, torch.Tensor],
    batch: dict[str, torch.Tensor],
    policy_config: dict[str, object] | None = None,
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
    policy_config: dict[str, object] | None = None,
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


def prepare_model_for_ppo_updates(model: torch.nn.Module) -> None:
    model.train()
    for module in model.modules():
        if isinstance(module, torch.nn.Dropout):
            module.eval()


def old_policy_stats(
    batch: dict[str, torch.Tensor],
    old_policy_model: torch.nn.Module | None,
    policy_config: dict[str, object],
) -> tuple[torch.Tensor, torch.Tensor]:
    if old_policy_model is None:
        return batch["old_log_prob"].float(), batch["old_value"].float()
    with torch.no_grad():
        old_outputs = forward_model(old_policy_model, batch)
        old_log_probs = select_action_log_probs(old_outputs, batch, policy_config)
        old_values = old_outputs["value"].squeeze(1)
    return old_log_probs, old_values


def recompute_dataset_values_from_old_policy(
    dataset: ArenaTrajectoryDataset,
    old_policy_model: torch.nn.Module,
    device: torch.device,
    gamma: float,
    gae_lambda: float,
    batch_size: int,
) -> None:
    from rl_dataset import compute_gae_for_rows

    loader = DataLoader(dataset, batch_size=batch_size, shuffle=False)
    values: list[float] = []
    old_policy_model.eval()
    with torch.no_grad():
        for batch in loader:
            batch = {key: value.to(device) for key, value in batch.items()}
            outputs = forward_model(old_policy_model, batch)
            if "value" not in outputs:
                raise SystemExit(
                    "Cannot recompute old critic values: checkpoint forward pass "
                    "did not produce global critic 'value'."
                )
            values.extend(outputs["value"].squeeze(1).detach().cpu().tolist())

    if len(values) != len(dataset.rows):
        raise SystemExit(
            "Cannot recompute old critic values: row/value count mismatch "
            f"({len(dataset.rows)} rows, {len(values)} values)."
        )
    for row, value in zip(dataset.rows, values, strict=True):
        row["value"] = float(value)
    dataset.advantages, dataset.returns = compute_gae_for_rows(
        dataset.rows,
        gamma=gamma,
        gae_lambda=gae_lambda,
    )
    if hasattr(dataset, "tensors"):
        delattr(dataset, "tensors")


def policy_ratio_metrics(
    log_probs: torch.Tensor,
    old_log_probs: torch.Tensor,
    clip_epsilon: float,
) -> tuple[torch.Tensor, torch.Tensor]:
    with torch.no_grad():
        ratio = torch.exp(log_probs - old_log_probs)
        approx_kl = (old_log_probs - log_probs).mean()
        clip_fraction = (
            (torch.abs(ratio - 1.0) > clip_epsilon).float().mean()
        )
    return approx_kl, clip_fraction


def teacher_kl_loss(
    teacher_model: torch.nn.Module | None,
    outputs: dict[str, torch.Tensor],
    batch: dict[str, torch.Tensor],
    policy_config: dict[str, object],
    device: torch.device,
) -> torch.Tensor:
    if teacher_model is None:
        return torch.zeros((), device=device)
    with torch.no_grad():
        teacher_outputs = forward_model(teacher_model, batch)
    return select_action_head_kl(teacher_outputs, outputs, batch, policy_config)


def normalized_advantage_stats(
    batch: dict[str, torch.Tensor],
) -> tuple[torch.Tensor, torch.Tensor, torch.Tensor]:
    advantages = batch["advantage"].float()
    with torch.no_grad():
        raw_mean = advantages.mean()
        raw_std = advantages.std(unbiased=False)
    normalized = (advantages - advantages.mean()) / (
        advantages.std(unbiased=False) + 1.0e-8
    )
    return normalized, raw_mean, raw_std


def value_training_metrics(
    outputs: dict[str, torch.Tensor],
    old_values: torch.Tensor,
    returns: torch.Tensor,
    value_clip_epsilon: float,
) -> tuple[torch.Tensor, torch.Tensor, torch.Tensor]:
    values = outputs["value"].squeeze(1)
    value_loss = clipped_value_loss(
        values,
        old_values,
        returns,
        value_clip_epsilon,
    )

    if "value_2" in outputs:
        values_2 = outputs["value_2"].squeeze(1)
        value_loss_2 = clipped_value_loss(
            values_2,
            old_values,
            returns,
            value_clip_epsilon,
        )
        value_loss = (value_loss + value_loss_2) / 2.0
        values = torch.minimum(values, values_2)

    explained_variance = value_explained_variance(values.detach(), returns)
    with torch.no_grad():
        value_mse = (values - returns).pow(2).mean()
    return value_loss, explained_variance, value_mse


def opponent_auxiliary_loss(
    outputs: dict[str, torch.Tensor],
    batch: dict[str, torch.Tensor],
) -> torch.Tensor:
    device = batch["return"].device
    if "opponent_tenpai_logits" not in outputs or "opponent_risk_logits" not in outputs:
        return torch.tensor(0.0, device=device)

    tenpai_loss = F.binary_cross_entropy_with_logits(
        outputs["opponent_tenpai_logits"],
        batch["opponent_tenpai_target"].float(),
        reduction="mean",
    )
    risk_targets = batch["opponent_risk_target"].float()
    risk_masks = batch["opponent_risk_mask"].bool()
    risk_loss = F.binary_cross_entropy_with_logits(
        outputs["opponent_risk_logits"],
        risk_targets,
        reduction="none",
    )
    risk_loss = risk_loss[risk_masks].mean() if risk_masks.any() else torch.tensor(0.0, device=device)
    return tenpai_loss + risk_loss


def forward_and_compute_ppo_loss(
    model: torch.nn.Module,
    batch: dict[str, torch.Tensor],
    old_policy_model: torch.nn.Module | None,
    teacher_model: torch.nn.Module | None,
    policy_config: dict[str, object],
    args: argparse.Namespace,
    entropy_coef: float,
    kl_coef: float,
) -> dict[str, torch.Tensor]:
    outputs = forward_model(model, batch)
    old_log_probs, old_values = old_policy_stats(batch, old_policy_model, policy_config)
    log_probs = select_action_log_probs(outputs, batch, policy_config)
    returns = batch["return"].float()
    advantages, raw_advantage_mean, raw_advantage_std = normalized_advantage_stats(batch)
    policy_loss = ppo_policy_loss(
        log_probs,
        old_log_probs,
        advantages,
        args.clip_epsilon,
    )
    approx_kl, clip_fraction = policy_ratio_metrics(
        log_probs,
        old_log_probs,
        args.clip_epsilon,
    )
    value_loss, explained_variance, value_mse = value_training_metrics(
        outputs,
        old_values,
        returns,
        args.value_clip_epsilon,
    )
    entropy = select_action_entropy(outputs, batch, policy_config)
    kl_loss = teacher_kl_loss(teacher_model, outputs, batch, policy_config, returns.device)
    opponent_loss = opponent_auxiliary_loss(outputs, batch)
    loss = (
        policy_loss
        + 0.5 * value_loss
        + args.opponent_loss_coef * opponent_loss
        - entropy_coef * entropy
        + kl_coef * kl_loss
    )
    return {
        "loss": loss,
        "policy_loss": policy_loss,
        "value_loss": value_loss,
        "opponent_loss": opponent_loss,
        "entropy": entropy,
        "kl_loss": kl_loss,
        "approx_kl": approx_kl,
        "clip_fraction": clip_fraction,
        "explained_variance": explained_variance,
        "value_mse": value_mse,
        "raw_advantage_mean": raw_advantage_mean,
        "raw_advantage_std": raw_advantage_std,
    }


def autocast_ppo_loss(
    amp_config: AmpConfig,
    model: torch.nn.Module,
    batch: dict[str, torch.Tensor],
    old_policy_model: torch.nn.Module | None,
    teacher_model: torch.nn.Module | None,
    policy_config: dict[str, object],
    args: argparse.Namespace,
    entropy_coef: float,
    kl_coef: float,
) -> dict[str, torch.Tensor]:
    with torch.amp.autocast(
        amp_config.device_type,
        enabled=amp_config.enabled,
        dtype=amp_config.dtype,
    ):
        return forward_and_compute_ppo_loss(
            model,
            batch,
            old_policy_model,
            teacher_model,
            policy_config,
            args,
            entropy_coef,
            kl_coef,
        )


def main() -> None:
    args = parse_args()
    validate_checkpoint_architecture(args.checkpoint, args.use_actor_critic)
    args.output.mkdir(parents=True, exist_ok=True)
    device = resolve_device(args.device)
    amp_config = resolve_amp_config(device, args.amp)
    if amp_config.disabled_reason:
        print(f"Warning: AMP disabled: {amp_config.disabled_reason}")

    policy_config = dict(POLICY_CONFIGS[args.policy])
    print(f"Policy: {args.policy} - {policy_config['description']}")

    entropy_multiplier = float(policy_config["entropy_multiplier"])
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
    model_config = model_config_from_checkpoint(args.checkpoint)

    if args.use_actor_critic:
        model = build_actor_critic(model_config).to(device)
        print("Using separate actor-critic architecture with global features")
    else:
        model = build_model(model_config).to(device)
        print("Using shared policy-value network")

    load_checkpoint_if_present(model, args.checkpoint)
    prepare_model_for_ppo_updates(model)
    old_policy_model = (
        build_old_policy_model(args.checkpoint, device)
        if args.recompute_old_policy_stats
        else None
    )
    teacher_model = build_old_policy_model(args.checkpoint, device) if args.kl_coef > 0.0 else None

    if args.recompute_old_policy_stats:
        if old_policy_model is None:
            raise SystemExit(
                "--recompute-old-policy-stats requires --checkpoint so old "
                "critic values can be recomputed from global features."
            )
        recompute_dataset_values_from_old_policy(
            dataset,
            old_policy_model,
            device,
            gamma=args.gamma,
            gae_lambda=args.gae_lambda,
            batch_size=args.batch_size,
        )

    diagnostics = trajectory_diagnostics(dataset.rows)
    loader = DataLoader(dataset, batch_size=args.batch_size, shuffle=True)

    if args.use_actor_critic:
        actor_params = list(model.actor.parameters())
        critic_params = list(model.critic.parameters())
        critic_lr = args.lr * args.critic_lr_multiplier
        optimizer = torch.optim.AdamW([
            {"params": actor_params, "lr": args.lr, "name": "actor"},
            {"params": critic_params, "lr": critic_lr, "name": "critic"},
        ])
        print(f"Using separate optimizers: actor_lr={args.lr:.6f}, critic_lr={critic_lr:.6f}")
    else:
        optimizer = torch.optim.AdamW(model.parameters(), lr=args.lr)

    print(
        "RL train: "
        f"trajectories={len(dataset)} batches={len(loader)} "
        f"epochs={args.epochs} batch_size={args.batch_size} device={device} "
        f"amp={amp_config.enabled} amp_dtype={amp_dtype_name(amp_config)} "
        f"entropy_start={adjusted_entropy_coef:.6f} entropy_end={adjusted_entropy_end_coef:.6f} "
        f"entropy_decay={args.entropy_decay_mode} "
        f"(base={args.entropy_coef:.6f}, multiplier={entropy_multiplier:.2f}) "
        f"replay_epochs={args.replay_buffer_epochs} replay_ratio={args.replay_ratio}"
    )
    print("RL trajectory diagnostics: " + json.dumps(diagnostics, ensure_ascii=False))

    total_steps = args.epochs * max(len(loader), 1)
    history = []
    global_step = 0
    kl_coef = args.kl_coef

    replay_buffer = ReplayBuffer(max_epochs=args.replay_buffer_epochs)

    for epoch in range(args.epochs):
        apply_lr_warmup(
            optimizer,
            epoch,
            args.lr_warmup_epochs,
            args.lr,
            args.critic_lr_multiplier,
        )

        current_batches = list(loader)
        replay_batches = replay_buffer.sample(int(len(current_batches) * args.replay_ratio))
        all_batches = current_batches + replay_batches

        if replay_batches:
            print(f"Epoch {epoch+1}: using {len(current_batches)} new + {len(replay_batches)} replay batches")

        total_loss = 0.0
        total_policy_loss = 0.0
        total_value_loss = 0.0
        total_opponent_loss = 0.0
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

        for batch in all_batches:
            batch = {key: value.to(device) for key, value in batch.items()}
            entropy_coef = entropy_coef_for_progress(
                global_step,
                total_steps,
                adjusted_entropy_coef,
                adjusted_entropy_end_coef,
                args.entropy_decay_mode,
            )
            losses = autocast_ppo_loss(
                amp_config,
                model,
                batch,
                old_policy_model,
                teacher_model,
                policy_config,
                args,
                entropy_coef,
                kl_coef,
            )
            loss = losses["loss"]
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            optimizer.step()
            total_loss += float(loss.detach().cpu())
            total_policy_loss += float(losses["policy_loss"].detach().cpu())
            total_value_loss += float(losses["value_loss"].detach().cpu())
            total_opponent_loss += float(losses["opponent_loss"].detach().cpu())
            total_entropy += float(losses["entropy"].detach().cpu())
            total_entropy_coef += entropy_coef
            total_kl_loss += float(losses["kl_loss"].detach().cpu())
            total_kl_coef += kl_coef
            total_approx_kl += float(losses["approx_kl"].detach().cpu())
            total_clip_fraction += float(losses["clip_fraction"].detach().cpu())
            total_value_explained_variance += float(losses["explained_variance"].detach().cpu())
            total_value_mse += float(losses["value_mse"].detach().cpu())
            total_advantage_mean += float(losses["raw_advantage_mean"].detach().cpu())
            total_advantage_std += float(losses["raw_advantage_std"].detach().cpu())
            batch_count += 1
            global_step += 1

        replay_buffer.add_epoch(current_batches)

        avg_approx_kl = total_approx_kl / max(batch_count, 1)
        if args.kl_adaptive:
            if avg_approx_kl < args.kl_target * 0.5:
                kl_coef = max(kl_coef * 0.8, 0.0)
            elif avg_approx_kl > args.kl_target * 1.5:
                kl_coef = min(kl_coef * 1.5, 0.1)

        epoch_metrics = {
            "epoch": epoch + 1,
            "loss": total_loss / max(batch_count, 1),
            "policy_loss": total_policy_loss / max(batch_count, 1),
            "value_loss": total_value_loss / max(batch_count, 1),
            "opponent_loss": total_opponent_loss / max(batch_count, 1),
            "entropy": total_entropy / max(batch_count, 1),
            "entropy_coef": total_entropy_coef / max(batch_count, 1),
            "kl_loss": total_kl_loss / max(batch_count, 1),
            "kl_coef": kl_coef,
            "approx_kl": avg_approx_kl,
            "clip_fraction": total_clip_fraction / max(batch_count, 1),
            "value_explained_variance": total_value_explained_variance / max(batch_count, 1),
            "value_mse": total_value_mse / max(batch_count, 1),
            "advantage_mean": total_advantage_mean / max(batch_count, 1),
            "advantage_std": total_advantage_std / max(batch_count, 1),
            "lr": optimizer.param_groups[0]["lr"],
        }
        history.append(epoch_metrics)
        print(format_epoch_metrics(epoch_metrics, args.epochs))
        torch.save(
            checkpoint_payload(model, model_config, history),
            args.output / epoch_checkpoint_name(epoch + 1),
        )
        if args.target_kl > 0.0 and avg_approx_kl > args.target_kl:
            history[-1]["early_stop"] = 1.0
            print(
                "RL train early stop: "
                f"approx_kl={avg_approx_kl:.6f} "
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
