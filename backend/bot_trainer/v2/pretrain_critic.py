from __future__ import annotations

import argparse
import json
from datetime import UTC, datetime
from pathlib import Path

import torch
import torch.nn.functional as F
from torch.utils.data import DataLoader

from model import ModelConfig, build_actor_critic
from rl_dataset import ArenaTrajectoryDataset


def build_critic_pretrain_loader(
    trajectories: Path,
    batch_size: int,
    gamma: float = 0.995,
    gae_lambda: float = 0.95,
    policy_id: str | None = "learner",
    tensor_cache: Path | None = None,
    require_global: bool = True,
) -> DataLoader:
    dataset = ArenaTrajectoryDataset(
        trajectories,
        gamma=gamma,
        gae_lambda=gae_lambda,
        policy_id=policy_id,
        cache_path=tensor_cache,
    )
    if len(dataset) == 0:
        raise ValueError(f"no trajectory rows found: {trajectories}")
    if require_global:
        missing = sum(1 for row in dataset.rows if row.get("global_tile_planes") is None)
        if missing > 0:
            raise ValueError(
                f"critic pretrain requires global features; missing rows={missing}"
            )
    return DataLoader(dataset, batch_size=batch_size, shuffle=True)


def pretrain_critic(
    train_loader: DataLoader,
    model: torch.nn.Module,
    optimizer: torch.optim.Optimizer,
    device: torch.device,
    epochs: int,
) -> list[dict[str, float]]:
    model.train()
    for parameter in model.actor.parameters():
        parameter.requires_grad_(False)

    history: list[dict[str, float]] = []
    for epoch in range(epochs):
        total_loss = 0.0
        batch_count = 0
        for batch in train_loader:
            batch = {key: value.to(device) for key, value in batch.items()}
            outputs = forward_critic_pretrain_model(model, batch)
            returns = batch["return"].float()
            loss = F.mse_loss(outputs["value"].squeeze(1), returns)
            if "value_2" in outputs:
                loss = loss + F.mse_loss(outputs["value_2"].squeeze(1), returns)

            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            optimizer.step()

            total_loss += float(loss.detach().cpu())
            batch_count += 1

        metrics = {
            "epoch": float(epoch + 1),
            "loss": total_loss / max(batch_count, 1),
        }
        history.append(metrics)
        print(
            f"Critic pretrain epoch {epoch + 1}/{epochs}: "
            f"loss={metrics['loss']:.6f}"
        )
    return history


def forward_critic_pretrain_model(
    model: torch.nn.Module,
    batch: dict[str, torch.Tensor],
) -> dict[str, torch.Tensor]:
    return model(
        batch["tile_planes"].float(),
        batch["scalar_features"].float(),
        batch["discard_sequence"].float(),
        global_tile_planes=batch["global_tile_planes"].float(),
        global_scalar_features=batch["global_scalar_features"].float(),
        return_both_critics=True,
    )


def resolve_device(requested: str) -> torch.device:
    if requested == "auto":
        return torch.device("cuda" if torch.cuda.is_available() else "cpu")
    return torch.device(requested)


def main() -> None:
    args = parse_args()
    device = resolve_device(args.device)

    print("Loading arena trajectories...")
    tensor_cache = None if args.no_tensor_cache else args.tensor_cache
    if tensor_cache is None and not args.no_tensor_cache:
        tensor_cache = args.trajectories.with_suffix(args.trajectories.suffix + ".critic.pt")
    train_loader = build_critic_pretrain_loader(
        args.trajectories,
        batch_size=args.batch_size,
        gamma=args.gamma,
        gae_lambda=args.gae_lambda,
        policy_id=args.policy_id,
        tensor_cache=tensor_cache,
        require_global=True,
    )

    print("Building actor-critic model...")
    checkpoint = torch.load(args.checkpoint, map_location="cpu")
    model_config = ModelConfig.from_dict(checkpoint.get("model_config", {}))
    model = build_actor_critic(model_config).to(device)
    missing, unexpected = model.load_state_dict(checkpoint["model_state"], strict=False)
    if unexpected:
        raise SystemExit(f"unexpected checkpoint keys: {unexpected}")
    if missing:
        print(f"Critic pretrain: missing keys initialized fresh: {missing}")

    print("Pretraining critic from trajectory returns...")
    optimizer = torch.optim.AdamW(model.critic.parameters(), lr=args.lr)
    history = pretrain_critic(train_loader, model, optimizer, device, args.epochs)

    print(f"Saving pretrained critic to {args.output}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    torch.save(
        {
            "model_state": model.state_dict(),
            "model_config": model_config.to_dict(),
            "training_source": "critic_pretrain",
            "created_at_utc": datetime.now(UTC).isoformat(),
            "critic_pretrain_metrics": history,
            "trajectory_source": args.trajectories.as_posix(),
            "policy_id": args.policy_id,
        },
        args.output,
    )
    metrics_path = args.output.with_suffix(args.output.suffix + ".metrics.json")
    metrics_path.write_text(
        json.dumps(history, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--trajectories", type=Path, required=True)
    parser.add_argument("--checkpoint", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--epochs", type=int, default=5)
    parser.add_argument("--batch-size", type=int, default=256)
    parser.add_argument("--lr", type=float, default=1e-4)
    parser.add_argument("--gamma", type=float, default=0.995)
    parser.add_argument("--gae-lambda", type=float, default=0.95)
    parser.add_argument("--policy-id", default="learner")
    parser.add_argument("--tensor-cache", type=Path, default=None)
    parser.add_argument("--no-tensor-cache", action="store_true")
    parser.add_argument("--device", default="auto")
    return parser.parse_args()


if __name__ == "__main__":
    main()
