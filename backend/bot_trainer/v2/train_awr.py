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
                        help="Checkpoint with pretrained actor + value head")
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--epochs", type=int, default=5)
    parser.add_argument("--batch-size", type=int, default=256)
    parser.add_argument("--lr", type=float, default=3e-5)
    parser.add_argument("--gamma", type=float, default=0.995)
    parser.add_argument("--temperature", type=float, default=1.0,
                        help="AWR temperature for exp(adv/T)")
    parser.add_argument("--weight-clip", type=float, default=10.0,
                        help="Max advantage weight")
    parser.add_argument("--policy-filter", default="positive",
                        choices=["all", "positive"],
                        help="positive = only samples with adv>0; all = all samples")
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
    masked = logits.clone()
    masked[~mask] = float("-inf")
    log_probs = F.log_softmax(masked, dim=-1)
    nll = -log_probs[range(len(action_index)), action_index]
    return (nll * weights).mean()


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
    optimizer = torch.optim.AdamW(model.parameters(), lr=args.lr)

    args.output_dir.mkdir(parents=True, exist_ok=True)

    for epoch in range(args.epochs):
        model.train()
        total_policy_loss = 0.0
        total_value_loss = 0.0
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
                advantage = returns - value.detach()
                weights = torch.exp(advantage / args.temperature).clamp(
                    max=args.weight_clip
                )
                if args.policy_filter == "positive":
                    weights = torch.where(advantage > 0, weights, torch.zeros_like(weights))

            action_head = batch["action_head"]

            discard_mask_t = action_head == 0
            claim_mask_t = action_head == 1
            self_kong_mask_t = action_head == 2
            hu_mask_t = action_head == 3

            policy_loss = 0.0
            valid = 0

            if discard_mask_t.any():
                loss = compute_ce_loss_for_action(
                    outputs["discard_logits"][discard_mask_t],
                    batch["discard_mask"][discard_mask_t],
                    batch["action_index"][discard_mask_t],
                    weights[discard_mask_t],
                )
                policy_loss = policy_loss + loss
                valid += 1

            if claim_mask_t.any():
                loss = compute_ce_loss_for_action(
                    outputs["claim_logits"][claim_mask_t],
                    batch["claim_mask"][claim_mask_t],
                    batch["action_index"][claim_mask_t],
                    weights[claim_mask_t],
                )
                policy_loss = policy_loss + loss
                valid += 1

            if self_kong_mask_t.any():
                loss = compute_ce_loss_for_action(
                    outputs["self_kong_logits"][self_kong_mask_t],
                    batch["self_kong_mask"][self_kong_mask_t],
                    batch["action_index"][self_kong_mask_t],
                    weights[self_kong_mask_t],
                )
                policy_loss = policy_loss + loss
                valid += 1

            if hu_mask_t.any():
                loss = compute_ce_loss_for_action(
                    outputs["hu_logits"][hu_mask_t],
                    batch["hu_mask"][hu_mask_t],
                    batch["action_index"][hu_mask_t],
                    weights[hu_mask_t],
                )
                policy_loss = policy_loss + loss
                valid += 1

            if valid > 0:
                policy_loss = policy_loss / valid

            total_loss = policy_loss + 0.5 * value_loss

            optimizer.zero_grad()
            total_loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), args.grad_clip_norm)
            optimizer.step()

            total_policy_loss += policy_loss.item() if isinstance(policy_loss, torch.Tensor) else 0.0
            total_value_loss += value_loss.item()
            total_samples += len(batch["return"])

        print(
            f"Epoch {epoch+1}: policy_loss={total_policy_loss/len(loader):.6f} "
            f"value_loss={total_value_loss/len(loader):.6f} "
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
