from __future__ import annotations

import argparse
from datetime import UTC, datetime
from pathlib import Path

import torch
import torch.nn.functional as F
from torch.utils.data import DataLoader
from tqdm import tqdm

from awr_dataset import ArenaTrajectoryDataset, split_rows_by_match_id
from model import ModelConfig, build_model


def score_bucket_index(terminal_reward: float) -> int:
    """Map terminal reward to 5 bucket indices."""
    if terminal_reward <= -1.5:
        return 0
    elif terminal_reward <= -0.5:
        return 1
    elif terminal_reward < 0.5:
        return 2
    elif terminal_reward <= 1.5:
        return 3
    else:
        return 4


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--trajectories", type=Path, required=True)
    parser.add_argument("--checkpoint", type=Path, required=True,
                        help="SFT checkpoint to load actor weights from")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--epochs", type=int, default=30)
    parser.add_argument("--batch-size", type=int, default=512)
    parser.add_argument("--lr", type=float, default=1e-3)
    parser.add_argument("--gamma", type=float, default=0.995)
    parser.add_argument("--score-bucket-weight", type=float, default=0.0,
                        help="Weight for auxiliary score bucket classification loss")
    parser.add_argument("--val-fraction", type=float, default=0.1,
                        help="Fraction of match IDs held out for value validation")
    parser.add_argument("--policy-id", default=None,
                        help="Only train on this policy's data")
    parser.add_argument("--device", default="cuda")
    parser.add_argument("--seed", type=int, default=42)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    torch.manual_seed(args.seed)
    device = torch.device(args.device if torch.cuda.is_available() else "cpu")

    full_ds = ArenaTrajectoryDataset(args.trajectories, gamma=args.gamma, policy_id=args.policy_id)
    train_rows, val_rows = split_rows_by_match_id(full_ds.rows, args.val_fraction, args.seed)
    ds = ArenaTrajectoryDataset.from_rows(train_rows, gamma=args.gamma)
    val_ds = ArenaTrajectoryDataset.from_rows(val_rows, gamma=args.gamma) if val_rows else None
    loader = DataLoader(ds, batch_size=args.batch_size, shuffle=True)
    return_variance = torch.tensor(ds.returns).float().var().item() + 1e-8
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

    for name, param in model.named_parameters():
        if "value_head" in name or "score_bucket_head" in name:
            param.requires_grad = True
        else:
            param.requires_grad = False

    trainable_params = [p for p in model.parameters() if p.requires_grad]
    optimizer = torch.optim.AdamW(trainable_params, lr=args.lr)

    model.train()
    val_mse = None
    val_ev = None
    for epoch in range(args.epochs):
        total_value_loss = 0.0
        total_score_loss = 0.0
        for batch in tqdm(loader, desc=f"Value epoch {epoch+1}/{args.epochs}"):
            batch = {k: v.to(device) for k, v in batch.items()}
            outputs = model(
                batch["tile_planes"],
                batch["scalar_features"],
                batch["discard_sequence"],
            )
            value = outputs["value"].squeeze(-1)
            returns = batch["return"].float()
            value_loss = F.mse_loss(value, returns)

            score_loss = torch.tensor(0.0, device=device)
            if args.score_bucket_weight > 0:
                done_mask = batch["done"]
                if done_mask.any():
                    score_logits = outputs["score_bucket_logits"][done_mask]
                    terminal_rewards = batch["terminal_reward"][done_mask].cpu().tolist()
                    bucket_targets = torch.tensor(
                        [score_bucket_index(r) for r in terminal_rewards],
                        dtype=torch.long, device=device,
                    )
                    score_loss = F.cross_entropy(score_logits, bucket_targets)

            loss = value_loss + args.score_bucket_weight * score_loss

            optimizer.zero_grad()
            loss.backward()
            optimizer.step()
            total_value_loss += value_loss.item()
            total_score_loss += score_loss.item()

        avg_value = total_value_loss / len(loader)
        avg_score = total_score_loss / len(loader)
        value_ev = 1.0 - avg_value / return_variance
        if val_loader is not None:
            val_mse = evaluate_value_mse(model, val_loader, device)
            val_ev = 1.0 - val_mse / val_variance
            val_text = f" val_mse={val_mse:.6f} val_ev={val_ev:.4f}"
        else:
            val_text = ""
        print(
            f"Epoch {epoch+1}: value_mse={avg_value:.6f} "
            f"value_ev={value_ev:.4f}{val_text} score_ce={avg_score:.6f}"
        )

    args.output.parent.mkdir(parents=True, exist_ok=True)
    torch.save(
        {
            "model_state": model.state_dict(),
            "model_config": model_config.to_dict(),
            "training_source": "value_pretrain",
            "created_at_utc": datetime.now(UTC).isoformat(),
            "value_metrics": {
                "final_mse": avg_value,
                "final_explained_variance": value_ev,
                "final_val_mse": val_mse,
                "final_val_explained_variance": val_ev,
                "final_score_ce": avg_score,
            },
        },
        args.output,
    )
    print(f"Saved value-pretrained checkpoint to {args.output}")


def evaluate_value_mse(
    model: torch.nn.Module,
    loader: DataLoader,
    device: torch.device,
) -> float:
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
