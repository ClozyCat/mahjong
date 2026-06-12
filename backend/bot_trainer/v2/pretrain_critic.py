from __future__ import annotations

import argparse
from pathlib import Path

import torch
import torch.nn.functional as F
from torch.utils.data import DataLoader

from dataset import MahjongDecisionDataset
from model import ModelConfig, build_actor_critic


def pretrain_critic(
    train_loader: DataLoader,
    model: torch.nn.Module,
    optimizer: torch.optim.Optimizer,
    device: torch.device,
    epochs: int,
) -> None:
    model.train()
    for param in model.actor.parameters():
        param.requires_grad_(False)

    for epoch in range(epochs):
        total_loss = 0.0
        batch_count = 0
        for batch in train_loader:
            batch = {key: value.to(device) for key, value in batch.items()}

            outputs = model(
                batch["tile_planes"].float(),
                batch["scalar_features"].float(),
                batch["discard_sequence"].float(),
                global_tile_planes=batch.get("global_tile_planes"),
                global_scalar_features=batch.get("global_scalar_features"),
                return_both_critics=True,
            )

            value_target = batch["value_target"].float()
            loss = F.mse_loss(outputs["value"], value_target)
            if "value_2" in outputs:
                loss = loss + F.mse_loss(outputs["value_2"], value_target)

            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            optimizer.step()

            total_loss += float(loss.detach().cpu())
            batch_count += 1

        avg_loss = total_loss / max(batch_count, 1)
        print(f"Critic pretrain epoch {epoch+1}/{epochs}: loss={avg_loss:.6f}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--data", type=Path, required=True)
    parser.add_argument("--checkpoint", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--epochs", type=int, default=5)
    parser.add_argument("--batch-size", type=int, default=512)
    parser.add_argument("--lr", type=float, default=1e-4)
    parser.add_argument("--device", default="auto")
    args = parser.parse_args()

    device = torch.device("cuda" if args.device == "auto" and torch.cuda.is_available() else args.device)

    print("Loading dataset...")
    train_dataset = MahjongDecisionDataset(
        args.data / "train.jsonl",
        args.data / "metadata.json",
    )
    train_loader = DataLoader(train_dataset, batch_size=args.batch_size, shuffle=True)

    print("Building model...")
    checkpoint = torch.load(args.checkpoint, map_location="cpu")
    model_config = ModelConfig.from_dict(checkpoint.get("model_config", {}))
    model = build_actor_critic(model_config).to(device)
    model.load_state_dict(checkpoint["model_state"], strict=False)

    print("Pretraining critic...")
    optimizer = torch.optim.AdamW(model.critic.parameters(), lr=args.lr)
    pretrain_critic(train_loader, model, optimizer, device, args.epochs)

    print(f"Saving pretrained critic to {args.output}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    torch.save({
        "model_state": model.state_dict(),
        "model_config": model_config.to_dict(),
        "training_source": "critic_pretrain",
    }, args.output)


if __name__ == "__main__":
    main()
