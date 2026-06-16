from __future__ import annotations

import argparse
from datetime import UTC, datetime
from pathlib import Path

import torch
import torch.nn.functional as F
from torch.utils.data import DataLoader
from tqdm import tqdm

from awr_dataset import ArenaTrajectoryDataset
from model import ModelConfig, build_model


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--trajectories", type=Path, required=True)
    parser.add_argument("--checkpoint", type=Path, required=True,
                        help="SFT checkpoint to load actor weights from")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--epochs", type=int, default=10)
    parser.add_argument("--batch-size", type=int, default=256)
    parser.add_argument("--lr", type=float, default=1e-3)
    parser.add_argument("--gamma", type=float, default=0.995)
    parser.add_argument("--policy-id", default=None,
                        help="Only train on this policy's data")
    parser.add_argument("--device", default="cuda")
    parser.add_argument("--seed", type=int, default=42)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    torch.manual_seed(args.seed)
    device = torch.device(args.device if torch.cuda.is_available() else "cpu")

    ds = ArenaTrajectoryDataset(args.trajectories, gamma=args.gamma, policy_id=args.policy_id)
    loader = DataLoader(ds, batch_size=args.batch_size, shuffle=True)

    checkpoint = torch.load(args.checkpoint, map_location="cpu")
    model_config = ModelConfig.from_dict(checkpoint.get("model_config", {}))
    model = build_model(model_config).to(device)

    model.load_state_dict(checkpoint["model_state"], strict=True)

    for name, param in model.named_parameters():
        if "value_head" in name:
            param.requires_grad = True
        else:
            param.requires_grad = False

    optimizer = torch.optim.AdamW(
        [p for p in model.parameters() if p.requires_grad],
        lr=args.lr,
    )

    model.train()
    for epoch in range(args.epochs):
        total_loss = 0.0
        for batch in tqdm(loader, desc=f"Value epoch {epoch+1}/{args.epochs}"):
            batch = {k: v.to(device) for k, v in batch.items()}
            outputs = model(
                batch["tile_planes"],
                batch["scalar_features"],
                batch["discard_sequence"],
            )
            value = outputs["value"].squeeze(-1)
            returns = batch["return"].float()
            loss = F.mse_loss(value, returns)

            optimizer.zero_grad()
            loss.backward()
            optimizer.step()
            total_loss += loss.item()

        avg_loss = total_loss / len(loader)
        print(f"Epoch {epoch+1}: avg_value_mse={avg_loss:.6f}")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    torch.save(
        {
            "model_state": model.state_dict(),
            "model_config": model_config.to_dict(),
            "training_source": "value_pretrain",
            "created_at_utc": datetime.now(UTC).isoformat(),
            "value_metrics": {"final_mse": avg_loss},
        },
        args.output,
    )
    print(f"Saved value-pretrained checkpoint to {args.output}")


if __name__ == "__main__":
    main()
