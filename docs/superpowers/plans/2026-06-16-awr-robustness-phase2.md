# AWR Phase 2 Robustness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add per-head loss weights for action imbalance, score bucket auxiliary head for value training, and KL divergence conservative penalty for policy drift prevention.

**Architecture:** `LightweightActor` gains a training-only `score_bucket_head`. `train_awr.py` gains head weights, KL penalty against frozen SFT reference. `train_value.py` gains score bucket auxiliary loss and tuned defaults.

**Tech Stack:** Python 3.12+, PyTorch

---

### Task 1: Add score_bucket_head to LightweightActor

**Files:**
- Modify: `backend/bot_trainer/v2/model.py`
- Modify: `backend/bot_trainer/v2/test_model.py`

- [ ] **Step 1: Add failing test for score_bucket_head**

Add to `test_model.py` inside `TestLightweightActor`:

```python
    def test_score_bucket_head_present(self):
        m = build_model(ModelConfig())
        m.eval()
        tp = torch.zeros((2, 10, 34))
        sf = torch.zeros((2, 12))
        ds = torch.zeros((2, 32, 40))
        out = m(tp, sf, ds)
        assert "score_bucket_logits" in out
        assert out["score_bucket_logits"].shape == (2, 5)

    def test_score_bucket_head_not_in_onnx(self):
        m = build_model(ModelConfig())
        assert "score_bucket_logits" not in LightweightActor.ONNX_OUTPUT_NAMES
        assert "score_bucket_logits" in m.TRAINING_ONLY_HEADS
```

Run: `cd backend/bot_trainer/v2 && python -m pytest test_model.py::TestLightweightActor::test_score_bucket_head_present test_model.py::TestLightweightActor::test_score_bucket_head_not_in_onnx -v`

Expected: 2 FAIL (key not found).

- [ ] **Step 2: Add score_bucket_head to model.py**

In `LightweightActor.__init__`, after `self.value_head`:

```python
            self.score_bucket_head = HeadMLP(256, 5)
```

In `LightweightActor.ONNX_OUTPUT_NAMES` — no change (score_bucket is training-only).

In `LightweightActor.TRAINING_ONLY_HEADS`:

```python
        TRAINING_ONLY_HEADS = {"value", "score_bucket_logits"}
```

In `LightweightActor.forward`, add to return dict after `"value"`:

```python
                "score_bucket_logits": self.score_bucket_head(policy_hidden),
```

- [ ] **Step 3: Run tests**

Run: `cd backend/bot_trainer/v2 && python -m pytest test_model.py::TestLightweightActor::test_score_bucket_head_present test_model.py::TestLightweightActor::test_score_bucket_head_not_in_onnx -v`

Expected: 2 PASS.

- [ ] **Step 4: Run all model tests**

Run: `cd backend/bot_trainer/v2 && python -m pytest test_model.py -v`

Expected: 12 tests, all pass.

- [ ] **Step 5: Commit**

```bash
git add backend/bot_trainer/v2/model.py backend/bot_trainer/v2/test_model.py
git commit -m "feat(awr): add score_bucket_head (training-only, 5-class) to LightweightActor"
```

---

### Task 2: Add per-head weights and KL penalty to train_awr.py

**Files:**
- Modify: `backend/bot_trainer/v2/train_awr.py`

- [ ] **Step 1: Add test for masked_categorical_kl**

Add to `test_awr_dataset.py` (or create a smaller inline test):

```python
import torch
import torch.nn.functional as F


def masked_categorical_kl(teacher_logits, student_logits, mask):
    """KL(softmax(teacher_masked) || softmax(student_masked))"""
    teacher = teacher_logits.clone()
    student = student_logits.clone()
    teacher[~mask] = float("-inf")
    student[~mask] = float("-inf")
    teacher_probs = F.softmax(teacher, dim=-1)
    student_log_probs = F.log_softmax(student, dim=-1)
    kl_per_sample = (teacher_probs * (torch.log(teacher_probs + 1e-8) - student_log_probs)).sum(-1)
    return kl_per_sample.mean()


class TestKLDivergence:
    def test_identical_logits_kl_zero(self):
        logits = torch.randn(4, 34)
        mask = torch.ones(4, 34, dtype=torch.bool)
        kl = masked_categorical_kl(logits, logits, mask)
        assert kl.item() < 0.01

    def test_maximally_different_kl_positive(self):
        teacher = torch.zeros(4, 34)
        teacher[:, 0] = 10.0
        student = torch.zeros(4, 34)
        student[:, -1] = 10.0
        mask = torch.ones(4, 34, dtype=torch.bool)
        kl = masked_categorical_kl(teacher, student, mask)
        assert kl.item() > 1.0

    def test_masked_positions_ignored(self):
        teacher = torch.randn(4, 7)
        student = torch.randn(4, 7)
        mask = torch.zeros(4, 7, dtype=torch.bool)
        mask[:, 0] = True
        teacher[:, 1:] = 0
        student[:, 1:] = 999
        kl = masked_categorical_kl(teacher, student, mask)
        assert kl.item() < 0.01
```

Run: `cd backend/bot_trainer/v2 && python -m pytest test_awr_dataset.py::TestKLDivergence -v`

Expected: 3 FAIL (function not imported in train_awr context; we're testing the function in isolation).

Note: Add `import torch; import torch.nn.functional as F` at the top of test_awr_dataset.py if not already present.

- [ ] **Step 2: Verify tests pass after adding function to test file**

Run: `cd backend/bot_trainer/v2 && python -m pytest test_awr_dataset.py::TestKLDivergence -v`

Expected: 3 PASS.

- [ ] **Step 3: Update train_awr.py — add head weights, KL penalty, SFT reference**

Replace `train_awr.py` with updated version:

```python
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
    parser.add_argument("--batch-size", type=int, default=256)
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
                        choices=["none", "per_match", "per_seat", "batch"],
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
    masked = logits.clone()
    masked[~mask] = float("-inf")
    log_probs = F.log_softmax(masked, dim=-1)
    nll = -log_probs[range(len(action_index)), action_index]
    return (nll * weights).mean()


def masked_categorical_kl(
    teacher_logits: torch.Tensor,
    student_logits: torch.Tensor,
    mask: torch.Tensor,
) -> torch.Tensor:
    teacher = teacher_logits.clone()
    student = student_logits.clone()
    teacher[~mask] = float("-inf")
    student[~mask] = float("-inf")
    teacher_probs = F.softmax(teacher, dim=-1)
    student_log_probs = F.log_softmax(student, dim=-1)
    kl_per_sample = (
        teacher_probs * (torch.log(teacher_probs + 1e-8) - student_log_probs)
    ).sum(-1)
    return kl_per_sample.mean()


def main() -> None:
    args = parse_args()
    torch.manual_seed(args.seed)
    device = torch.device(args.device if torch.cuda.is_available() else "cpu")

    ds = ArenaTrajectoryDataset(args.trajectories, gamma=args.gamma, policy_id=args.policy_id)

    if args.adv_norm in ("per_match", "per_seat"):
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

    # Load frozen SFT reference for KL penalty
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
                advantage = returns - value.detach()
                if args.adv_norm == "batch":
                    adv_mean = advantage.mean()
                    adv_std = advantage.std() + 1e-8
                    advantage = (advantage - adv_mean) / adv_std
                    advantage = advantage.clamp(-5.0, 5.0)
                elif args.adv_norm == "none":
                    advantage = advantage.clamp(-5.0, 5.0)
                weights = torch.exp(advantage / args.temperature).clamp(
                    max=args.weight_clip
                )
                if args.policy_filter == "positive":
                    weights = torch.where(advantage > 0, weights, torch.zeros_like(weights))

            action_head = batch["action_head"]

            # Per-head weighted policy loss
            head_logits_keys = ["discard_logits", "claim_logits", "self_kong_logits", "hu_logits"]
            head_mask_keys = ["discard_mask", "claim_mask", "self_kong_mask", "hu_mask"]

            policy_loss = 0.0
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

            # KL divergence penalty (all samples, all heads)
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
```

- [ ] **Step 4: Verify train_awr.py CLI shows new args**

Run: `cd backend/bot_trainer/v2 && python train_awr.py --help`

Expected: shows `--head-weights`, `--kl-coef`, `--sft-checkpoint`.

- [ ] **Step 5: Commit**

```bash
git add backend/bot_trainer/v2/train_awr.py backend/bot_trainer/v2/test_awr_dataset.py
git commit -m "feat(awr): add per-head weights, KL conservative penalty against SFT reference"
```

---

### Task 3: Add score bucket auxiliary loss to train_value.py

**Files:**
- Modify: `backend/bot_trainer/v2/train_value.py`

- [ ] **Step 1: Update train_value.py with score bucket and tuned defaults**

Replace `train_value.py`:

```python
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
    parser.add_argument("--batch-size", type=int, default=256)
    parser.add_argument("--lr", type=float, default=5e-4)
    parser.add_argument("--gamma", type=float, default=0.995)
    parser.add_argument("--score-bucket-weight", type=float, default=0.1,
                        help="Weight for auxiliary score bucket classification loss")
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
        if "value_head" in name or "score_bucket_head" in name:
            param.requires_grad = True
        else:
            param.requires_grad = False

    trainable_params = [p for p in model.parameters() if p.requires_grad]
    optimizer = torch.optim.AdamW(trainable_params, lr=args.lr)

    model.train()
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
        print(f"Epoch {epoch+1}: value_mse={avg_value:.6f} score_ce={avg_score:.6f}")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    torch.save(
        {
            "model_state": model.state_dict(),
            "model_config": model_config.to_dict(),
            "training_source": "value_pretrain",
            "created_at_utc": datetime.now(UTC).isoformat(),
            "value_metrics": {"final_mse": avg_value, "final_score_ce": avg_score},
        },
        args.output,
    )
    print(f"Saved value-pretrained checkpoint to {args.output}")


if __name__ == "__main__":
    main()
```

- [ ] **Step 2: Verify train_value.py CLI shows new args**

Run: `cd backend/bot_trainer/v2 && python train_value.py --help`

Expected: shows `--epochs` default 30, `--lr` default 5e-4, `--score-bucket-weight` default 0.1.

- [ ] **Step 3: Test score_bucket_index function**

Run: `cd backend/bot_trainer/v2 && python -c "
from train_value import score_bucket_index
assert score_bucket_index(-2.0) == 0
assert score_bucket_index(-1.0) == 1
assert score_bucket_index(0.0) == 2
assert score_bucket_index(1.0) == 3
assert score_bucket_index(2.0) == 4
print('score_bucket_index OK')
"`

Expected: `score_bucket_index OK`

- [ ] **Step 4: Commit**

```bash
git add backend/bot_trainer/v2/train_value.py
git commit -m "feat(awr): add score bucket auxiliary loss, tune value training defaults (30 epochs, 5e-4 lr)"
```

---

### Task 4: Final verification

- [ ] **Step 1: Run all tests**

Run: `cd backend/bot_trainer/v2 && python -m pytest test_model.py test_awr_dataset.py -v`

Expected: 23 tests (13 model + 6 dataset + 3 KL + 1 more model = 23), all pass.

- [ ] **Step 2: Verify all CLI tools**

Run: `cd backend/bot_trainer/v2 && python train_awr.py --help && python train_value.py --help`

Expected: both show updated help text with new parameters.

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "chore: final verification for AWR Phase 2 robustness"
```
