from __future__ import annotations

import argparse
import json
from pathlib import Path

import torch
import torch.nn.functional as F
from torch.utils.data import DataLoader

from model import ModelConfig, build_model
from rl_dataset import ArenaTrajectoryDataset


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--trajectories", type=Path, required=True)
    parser.add_argument("--checkpoint", type=Path, default=None)
    parser.add_argument("--epochs", type=int, default=1)
    parser.add_argument("--batch-size", type=int, default=256)
    parser.add_argument("--lr", type=float, default=1e-5)
    parser.add_argument("--gamma", type=float, default=0.99)
    parser.add_argument("--clip-epsilon", type=float, default=0.2)
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


def ppo_policy_loss(
    log_probs: torch.Tensor,
    old_log_probs: torch.Tensor,
    advantages: torch.Tensor,
    clip_epsilon: float,
) -> torch.Tensor:
    ratio = torch.exp(log_probs - old_log_probs)
    clipped = torch.clamp(ratio, 1.0 - clip_epsilon, 1.0 + clip_epsilon)
    return -torch.minimum(ratio * advantages, clipped * advantages).mean()


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
    model.load_state_dict(state, strict=False)


def select_action_log_probs(
    outputs: dict[str, torch.Tensor],
    batch: dict[str, torch.Tensor],
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
            result[active] = masked_head_log_probs(
                outputs[logits_key][active],
                batch[mask_key][active],
                batch["action_index"][active],
            )
    return result


def main() -> None:
    args = parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    device = resolve_device(args.device)
    dataset = ArenaTrajectoryDataset(args.trajectories)
    loader = DataLoader(dataset, batch_size=args.batch_size, shuffle=True)
    model = build_model(ModelConfig(tile_plane_count=10, scalar_feature_count=10)).to(device)
    load_checkpoint_if_present(model, args.checkpoint)
    optimizer = torch.optim.AdamW(model.parameters(), lr=args.lr)

    print(
        "RL train: "
        f"trajectories={len(dataset)} batches={len(loader)} "
        f"epochs={args.epochs} batch_size={args.batch_size} device={device}"
    )
    history = []
    for epoch in range(args.epochs):
        total_loss = 0.0
        total_policy_loss = 0.0
        total_value_loss = 0.0
        batch_count = 0
        for batch in loader:
            batch = {key: value.to(device) for key, value in batch.items()}
            outputs = model(batch["tile_planes"].float(), batch["scalar_features"].float())
            log_probs = select_action_log_probs(outputs, batch)
            advantages = batch["reward"].float() - batch["old_value"].float()
            policy_loss = ppo_policy_loss(
                log_probs,
                batch["old_log_prob"].float(),
                advantages,
                args.clip_epsilon,
            )
            values = outputs["value"].squeeze(1)
            value_loss = F.mse_loss(values, batch["reward"].float())
            loss = policy_loss + 0.5 * value_loss
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            optimizer.step()
            total_loss += float(loss.detach().cpu())
            total_policy_loss += float(policy_loss.detach().cpu())
            total_value_loss += float(value_loss.detach().cpu())
            batch_count += 1
        epoch_metrics = {
            "epoch": epoch + 1,
            "loss": total_loss / max(batch_count, 1),
            "policy_loss": total_policy_loss / max(batch_count, 1),
            "value_loss": total_value_loss / max(batch_count, 1),
        }
        history.append(epoch_metrics)
        print(
            "RL train epoch "
            f"{epoch_metrics['epoch']}/{args.epochs}: "
            f"loss={epoch_metrics['loss']:.6f} "
            f"policy_loss={epoch_metrics['policy_loss']:.6f} "
            f"value_loss={epoch_metrics['value_loss']:.6f}"
        )

    checkpoint_path = args.output / "best.pt"
    torch.save(
        {
            "model_state": model.state_dict(),
            "model_config": {"tile_plane_count": 10, "scalar_feature_count": 10},
            "rl_metrics": history,
        },
        checkpoint_path,
    )
    (args.output / "rl_metrics.json").write_text(
        json.dumps(history, indent=2),
        encoding="utf-8",
    )
    print(f"RL train saved checkpoint: {checkpoint_path}")


if __name__ == "__main__":
    main()
