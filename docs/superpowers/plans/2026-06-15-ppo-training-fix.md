# PPO Training Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix PPO training so it improves over SFT baseline by correcting GAE advantage computation, restructuring the training loop, and tuning hyperparameters.

**Architecture:** Restructure `rl_train.py` from whole-batch single-epoch to mini-batch multi-epoch PPO; make value recomputation mandatory for actor-critic mode; add gradient clipping; remove replay buffer; tune hyperparameters.

**Tech Stack:** Python, PyTorch

---

## File Structure

| File | Change Type | Responsibility |
|------|-------------|----------------|
| `backend/bot_trainer/v2/rl_dataset.py` | Modify | Add `recompute_values_and_gae()` method to ArenaTrajectoryDataset |
| `backend/bot_trainer/v2/rl_train.py` | Major refactor | Rewrite training loop; auto value recomputation; gradient clipping; remove ReplayBuffer; update hyperparams |
| `backend/bot_trainer/v2/train_rl_model.ps1` | Parameter update | Update default params; remove replay params; add new params |
| `backend/bot_trainer/v2/test_rl_dataset.py` | Extension | Add GAE correctness tests |

---

### Task 1: Add `recompute_values_and_gae()` to `rl_dataset.py`

**Files:**
- Modify: `backend/bot_trainer/v2/rl_dataset.py:14-64`

- [ ] **Step 1: Add `recompute_values_and_gae` method to `ArenaTrajectoryDataset`**

Insert after the `__getitem__` method (after line 64) in `ArenaTrajectoryDataset`:

```python
    def recompute_values_and_gae(
        self,
        value_fn,
        device: torch.device,
        batch_size: int = 256,
        gamma: float = 0.995,
        gae_lambda: float = 0.95,
    ) -> None:
        from torch.utils.data import DataLoader

        loader = DataLoader(self, batch_size=batch_size, shuffle=False)
        values: list[float] = []
        with torch.no_grad():
            for batch in loader:
                batch = {k: v.to(device) for k, v in batch.items()}
                v = value_fn(batch)
                if isinstance(v, torch.Tensor):
                    while v.dim() > 1:
                        v = v.squeeze(-1)
                    values.extend(v.detach().cpu().tolist())
                else:
                    values.extend([float(v)] * len(batch["reward"]))

        if len(values) != len(self.rows):
            raise ValueError(
                f"Value count mismatch: {len(values)} values for {len(self.rows)} rows"
            )

        for row, val in zip(self.rows, values, strict=True):
            row["value"] = float(val)

        self.advantages, self.returns = compute_gae_for_rows(
            self.rows,
            gamma=gamma,
            gae_lambda=gae_lambda,
        )

        if hasattr(self, "tensors"):
            delattr(self, "tensors")
```

Also add the missing `Callable` import at the top. Change line 5 from:

```python
from typing import Any
```

to:

```python
from typing import Any, Callable
```

- [ ] **Step 2: Run existing tests to verify no regression**

Run: `python -m pytest backend/bot_trainer/v2/test_rl_dataset.py -q -p no:cacheprovider`
Expected: All existing tests PASS.

- [ ] **Step 3: Commit**

```bash
git add backend/bot_trainer/v2/rl_dataset.py
git commit -m "feat(rl_dataset): add recompute_values_and_gae method for critic-based value correction"
```

---

### Task 2: Refactor `rl_train.py` — Remove ReplayBuffer and update `parse_args()`

**Files:**
- Modify: `backend/bot_trainer/v2/rl_train.py`

This task removes ReplayBuffer and updates argument defaults in preparation for the training loop rewrite.

- [ ] **Step 1: Remove `ReplayBuffer` class**

Delete the `ReplayBuffer` class (lines 25-42):

```python
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
```

- [ ] **Step 2: Update `parse_args()` — remove replay params, add new params, update defaults**

In `parse_args()`, make these changes:

**Remove these arguments:**
- `--replay-buffer-epochs` (line 80)
- `--replay-ratio` (line 81)

**Add these new arguments** after the `--kl-adaptive` argument (after line 78):

```python
    parser.add_argument("--grad-clip-norm", type=float, default=1.0,
        help="Max gradient norm for clipping. 0 to disable.")
    parser.add_argument("--value-early-stop-patience", type=int, default=5,
        help="Stop value updates if explained variance doesn't improve for N mini-batches. 0 to disable.")
    parser.add_argument("--mini-batch-size", type=int, default=None,
        help="Mini-batch size for PPO updates. Defaults to --batch-size if not set (whole-batch mode).")
```

**Update these default values:**

| Argument | Old | New |
|----------|-----|-----|
| `--epochs` | 1 | 4 |
| `--lr` | 3e-6 | 3e-5 |
| `--clip-epsilon` | 0.15 | 0.2 |
| `--entropy-coef` | 0.03 | 0.06 |
| `--entropy-end-coef` | 0.008 | 0.02 |
| `--kl-coef` | 0.01 | 0.02 |
| `--kl-target` | 0.02 | 0.03 |
| `--opponent-loss-coef` | 0.05 | 0.3 |

Specifically, change these lines in `parse_args()`:
- `parser.add_argument("--epochs", type=int, default=1)` → `default=4`
- `parser.add_argument("--lr", type=float, default=3e-6)` → `default=3e-5`
- `parser.add_argument("--clip-epsilon", type=float, default=0.15)` → `default=0.2`
- `parser.add_argument("--entropy-coef", type=float, default=0.03)` → `default=0.06`
- `parser.add_argument("--entropy-end-coef", type=float, default=0.008)` → `default=0.02`
- `parser.add_argument("--kl-coef", type=float, default=0.01)` → `default=0.02`
- `parser.add_argument("--kl-target", type=float, default=0.02)` → `default=0.03`
- `parser.add_argument("--opponent-loss-coef", type=float, default=0.05)` → `default=0.3`

- [ ] **Step 3: Remove replay-related code from `main()`**

In the `main()` function:
- Delete `replay_buffer = ReplayBuffer(max_epochs=args.replay_buffer_epochs)` (line 842)
- Delete `replay_batches = replay_buffer.sample(int(len(current_batches) * args.replay_ratio))` (line 854)
- Delete `all_batches = current_batches + replay_batches` (line 855)
- Delete the `if replay_batches:` print block (lines 857-858)
- Change `for batch in all_batches:` to `for batch in current_batches:` (line 876)
- Delete `replay_buffer.add_epoch(current_batches)` (line 917)
- Remove `replay_epochs` and `replay_ratio` from the print statement (line 833)

- [ ] **Step 4: Run existing tests**

Run: `python -m pytest backend/bot_trainer/v2/test.py -q -p no:cacheprovider`
Expected: PASS (or at minimum, no new failures introduced by arg changes).

- [ ] **Step 5: Commit**

```bash
git add backend/bot_trainer/v2/rl_train.py
git commit -m "refactor(rl_train): remove ReplayBuffer, update hyperparameter defaults, add grad-clip and mini-batch args"
```

---

### Task 3: Refactor `rl_train.py` — Add mandatory value recomputation for actor-critic

**Files:**
- Modify: `backend/bot_trainer/v2/rl_train.py:779-808`

- [ ] **Step 1: Add auto value recomputation when `--use-actor-critic` is active**

In `main()`, replace the existing `--recompute-old-policy-stats` block (lines 795-808) with this logic that handles both auto value recomputation and old-policy-stats recomputation:

```python
    if args.use_actor_critic:
        print("Auto-recomputing values with critic before GAE...")
        critic_value_fn = _make_critic_value_fn(model, device)
        dataset.recompute_values_and_gae(
            critic_value_fn,
            device,
            batch_size=args.batch_size,
            gamma=args.gamma,
            gae_lambda=args.gae_lambda,
        )
        print("Critic values recomputed. GAE advantages updated.")

    if args.recompute_old_policy_stats:
        if old_policy_model is None:
            raise SystemExit(
                "--recompute-old-policy-stats requires --checkpoint."
            )
        recompute_dataset_values_from_old_policy(
            dataset,
            old_policy_model,
            device,
            gamma=args.gamma,
            gae_lambda=args.gae_lambda,
            batch_size=args.batch_size,
        )
```

- [ ] **Step 2: Add the `_make_critic_value_fn` helper**

Add this function before `main()` (e.g., after `build_old_policy_model` around line 498):

```python
def _make_critic_value_fn(model: torch.nn.Module, device: torch.device):
    def value_fn(batch: dict[str, torch.Tensor]) -> torch.Tensor:
        outputs = forward_model(model, batch)
        if "value" not in outputs:
            raise SystemExit(
                "Cannot auto-recompute values: model forward pass did not "
                "produce 'value'. Use --use-actor-critic with a checkpoint "
                "that has a value head."
            )
        return outputs["value"].squeeze(-1)
    return value_fn
```

- [ ] **Step 3: Remove the `trajectory_stats_are_all_zero` early check for actor-critic**

When using actor-critic, values will be recomputed anyway, so the all-zero check is moot. Change lines 772-776 from:

```python
    if trajectory_stats_are_all_zero(dataset) and not args.recompute_old_policy_stats:
        raise SystemExit(
            "Trajectory old policy stats are all zero. Regenerate trajectories with "
            "log_prob/value, or pass --recompute-old-policy-stats with the rollout checkpoint."
        )
```

to:

```python
    if trajectory_stats_are_all_zero(dataset) and not args.recompute_old_policy_stats and not args.use_actor_critic:
        raise SystemExit(
            "Trajectory old policy stats are all zero. Regenerate trajectories with "
            "log_prob/value, or pass --recompute-old-policy-stats with the rollout checkpoint, "
            "or use --use-actor-critic to auto-recompute values."
        )
```

- [ ] **Step 4: Run tests**

Run: `python -m pytest backend/bot_trainer/v2/test.py -q -p no:cacheprovider`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add backend/bot_trainer/v2/rl_train.py
git commit -m "feat(rl_train): auto-recompute critic values for actor-critic before GAE"
```

---

### Task 4: Refactor `rl_train.py` — Rewrite training loop to mini-batch multi-epoch

**Files:**
- Modify: `backend/bot_trainer/v2/rl_train.py:844-958`

This is the core refactor. Replace the entire training loop section.

- [ ] **Step 1: Add mini-batch split helper**

Add this function before `main()` (e.g., after `_make_critic_value_fn`):

```python
def split_into_mini_batches(
    batch: dict[str, torch.Tensor],
    mini_batch_size: int,
) -> list[dict[str, torch.Tensor]]:
    batch_size = batch["reward"].shape[0]
    if mini_batch_size >= batch_size:
        return [batch]
    indices = torch.randperm(batch_size)
    mini_batches = []
    for start in range(0, batch_size, mini_batch_size):
        end = min(start + mini_batch_size, batch_size)
        mb_indices = indices[start:end]
        mini_batches.append({
            key: tensor[mb_indices] for key, tensor in batch.items()
        })
    return mini_batches
```

- [ ] **Step 2: Rewrite the training loop in `main()`**

Replace the entire training loop (from `for epoch in range(args.epochs):` through the `if args.target_kl > 0.0` early stop block) with:

```python
    total_steps = args.epochs * max(len(loader), 1)
    history = []
    global_step = 0
    kl_coef = args.kl_coef
    mini_batch_size = args.mini_batch_size or args.batch_size

    for epoch in range(args.epochs):
        apply_lr_warmup(
            optimizer,
            epoch,
            args.lr_warmup_epochs,
            args.lr,
            args.critic_lr_multiplier,
        )

        epoch_batches = list(loader)
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
        mini_batch_count = 0
        value_no_improve_count = 0
        skip_value_update = False

        for full_batch in epoch_batches:
            full_batch = {key: value.to(device) for key, value in full_batch.items()}
            mini_batches = split_into_mini_batches(full_batch, mini_batch_size)

            for mb in mini_batches:
                mb["advantage"] = (mb["advantage"].float() - mb["advantage"].float().mean()) / (mb["advantage"].float().std(unbiased=False) + 1e-8)

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
                    mb,
                    old_policy_model,
                    teacher_model,
                    policy_config,
                    args,
                    entropy_coef,
                    kl_coef,
                )
                loss = losses["loss"]

                if skip_value_update:
                    loss = loss - 0.5 * losses["value_loss"] + 0.5 * losses["value_loss"].detach()

                optimizer.zero_grad(set_to_none=True)
                loss.backward()

                if args.grad_clip_norm > 0:
                    if args.use_actor_critic:
                        torch.nn.utils.clip_grad_norm_(model.actor.parameters(), args.grad_clip_norm)
                        torch.nn.utils.clip_grad_norm_(model.critic.parameters(), args.grad_clip_norm)
                    else:
                        torch.nn.utils.clip_grad_norm_(model.parameters(), args.grad_clip_norm)

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
                mini_batch_count += 1
                global_step += 1

                if args.value_early_stop_patience > 0 and not skip_value_update:
                    ev = float(losses["explained_variance"].detach().cpu())
                    if ev < 0.0:
                        value_no_improve_count += 1
                    else:
                        value_no_improve_count = 0
                    if value_no_improve_count >= args.value_early_stop_patience:
                        skip_value_update = True
                        print(f"  Value early stop triggered at mini-batch {mini_batch_count}")

        avg_approx_kl = total_approx_kl / max(mini_batch_count, 1)
        if args.kl_adaptive:
            if avg_approx_kl < args.kl_target * 0.5:
                kl_coef = max(kl_coef * 0.8, 0.0)
            elif avg_approx_kl > args.kl_target * 1.5:
                kl_coef = min(kl_coef * 1.5, 0.1)

        epoch_metrics = {
            "epoch": epoch + 1,
            "loss": total_loss / max(mini_batch_count, 1),
            "policy_loss": total_policy_loss / max(mini_batch_count, 1),
            "value_loss": total_value_loss / max(mini_batch_count, 1),
            "opponent_loss": total_opponent_loss / max(mini_batch_count, 1),
            "entropy": total_entropy / max(mini_batch_count, 1),
            "entropy_coef": total_entropy_coef / max(mini_batch_count, 1),
            "kl_loss": total_kl_loss / max(mini_batch_count, 1),
            "kl_coef": kl_coef,
            "approx_kl": avg_approx_kl,
            "clip_fraction": total_clip_fraction / max(mini_batch_count, 1),
            "value_explained_variance": total_value_explained_variance / max(mini_batch_count, 1),
            "value_mse": total_value_mse / max(mini_batch_count, 1),
            "advantage_mean": total_advantage_mean / max(mini_batch_count, 1),
            "advantage_std": total_advantage_std / max(mini_batch_count, 1),
            "lr": optimizer.param_groups[0]["lr"],
            "mini_batch_count": mini_batch_count,
            "value_early_stopped": skip_value_update,
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
```

- [ ] **Step 3: Remove `normalized_advantage_stats` call from `forward_and_compute_ppo_loss`**

In `forward_and_compute_ppo_loss()` (around line 657-712), change the advantage normalization. Currently:

```python
    advantages, raw_advantage_mean, raw_advantage_std = normalized_advantage_stats(batch)
```

Replace with:

```python
    advantages = batch["advantage"].float()
    with torch.no_grad():
        raw_advantage_mean = advantages.mean()
        raw_advantage_std = advantages.std(unbiased=False)
```

This is because advantages are now normalized per-mini-batch in the training loop BEFORE being passed to the loss function.

- [ ] **Step 4: Update the print statement in `main()`**

Replace the print block that references replay params (around line 825-834):

```python
    print(
        "RL train: "
        f"trajectories={len(dataset)} batches={len(loader)} "
        f"epochs={args.epochs} batch_size={args.batch_size} "
        f"mini_batch_size={mini_batch_size} "
        f"device={device} "
        f"amp={amp_config.enabled} amp_dtype={amp_dtype_name(amp_config)} "
        f"entropy_start={adjusted_entropy_coef:.6f} entropy_end={adjusted_entropy_end_coef:.6f} "
        f"entropy_decay={args.entropy_decay_mode} "
        f"(base={args.entropy_coef:.6f}, multiplier={entropy_multiplier:.2f}) "
        f"grad_clip={args.grad_clip_norm} "
        f"value_early_stop_patience={args.value_early_stop_patience}"
    )
```

- [ ] **Step 5: Run tests**

Run: `python -m pytest backend/bot_trainer/v2/test.py backend/bot_trainer/v2/test_rl_dataset.py -q -p no:cacheprovider`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add backend/bot_trainer/v2/rl_train.py
git commit -m "refactor(rl_train): rewrite training loop as mini-batch multi-epoch PPO with per-mb advantage normalization and gradient clipping"
```

---

### Task 5: Update `train_rl_model.ps1` defaults

**Files:**
- Modify: `backend/bot_trainer/v2/train_rl_model.ps1`

- [ ] **Step 1: Update parameter defaults**

Change these default values in the `param()` block:

| Parameter | Old | New |
|-----------|-----|-----|
| `$Epochs` | 1 | 4 |
| `$LearningRate` | 0.000003 | 0.00003 |
| `$ClipEpsilon` | 0.15 | 0.2 |
| `$EntropyCoef` | 0.03 | 0.06 |
| `$EntropyEndCoef` | 0.008 | 0.02 |
| `$KlCoef` | 0.01 | 0.02 |
| `$KlTarget` | 0.02 | 0.03 |

- [ ] **Step 2: Add new parameters**

Add these after the existing params in the `param()` block:

```powershell
    [double]$GradClipNorm = 1.0,
    [int]$ValueEarlyStopPatience = 5,
    [int]$MiniBatchSize = 0,
    [double]$OpponentLossCoef = 0.3,
```

- [ ] **Step 3: Remove replay buffer parameters**

Remove these from the `param()` block:
- `[int]$ReplayBufferEpochs = 3`
- `[double]$ReplayRatio = 0.4`

- [ ] **Step 4: Update the `Invoke-PolicyTraining` function**

In `Invoke-PolicyTraining`, add the new arguments to `$rlTrainArgs` and remove replay args:

Remove:
```powershell
        "--replay-buffer-epochs", "$ReplayBufferEpochs",
        "--replay-ratio", "$ReplayRatio",
```

Add:
```powershell
        "--grad-clip-norm", "$GradClipNorm",
        "--value-early-stop-patience", "$ValueEarlyStopPatience",
        "--opponent-loss-coef", "$OpponentLossCoef",
```

If `$MiniBatchSize -gt 0`, add:
```powershell
        "--mini-batch-size", "$MiniBatchSize",
```

- [ ] **Step 5: Update the summary print block**

Remove the `replay_epochs` and `replay_ratio` lines from the print block. Add:

```powershell
    Write-Host "Grad clip norm:      $GradClipNorm"
    Write-Host "Value early stop:    $ValueEarlyStopPatience"
    Write-Host "Mini-batch size:     $(if ($MiniBatchSize -gt 0) { $MiniBatchSize } else { 'same as batch' })"
    Write-Host "Opponent loss coef:  $OpponentLossCoef"
```

- [ ] **Step 6: Commit**

```bash
git add backend/bot_trainer/v2/train_rl_model.ps1
git commit -m "feat(train_rl_model): update PPO default hyperparameters, add grad-clip and mini-batch params"
```

---

### Task 6: Add tests for GAE correctness and value recomputation

**Files:**
- Modify: `backend/bot_trainer/v2/test_rl_dataset.py`

- [ ] **Step 1: Add test for GAE with zero values**

```python
def test_gae_with_zero_values():
    rows = [
        {"match_id": "m1", "seat_index": 0, "reward": 0.1, "value": 0.0, "done": False},
        {"match_id": "m1", "seat_index": 0, "reward": 0.2, "value": 0.0, "done": False},
        {"match_id": "m1", "seat_index": 0, "reward": 1.0, "value": 0.0, "done": True},
    ]
    gamma = 0.99
    gae_lambda = 0.95
    advantages, returns = compute_gae_for_rows(rows, gamma, gae_lambda)

    assert len(advantages) == 3
    assert len(returns) == 3

    # With value=0.0, delta_t = r_t + gamma * 0 - 0 = r_t
    # advantage_2 = delta_2 = 1.0
    assert abs(advantages[2] - 1.0) < 1e-6
    # advantage_1 = delta_1 + gamma*lambda*advantage_2 = 0.2 + 0.99*0.95*1.0
    expected_adv1 = 0.2 + gamma * gae_lambda * 1.0
    assert abs(advantages[1] - expected_adv1) < 1e-4
    # return_2 = value_2 + advantage_2 = 0 + 1.0 = 1.0
    assert abs(returns[2] - 1.0) < 1e-6
```

- [ ] **Step 2: Add test for GAE with known values**

```python
def test_gae_with_known_values():
    rows = [
        {"match_id": "m1", "seat_index": 0, "reward": 0.1, "value": 1.0, "done": False},
        {"match_id": "m1", "seat_index": 0, "reward": 0.2, "value": 0.8, "done": False},
        {"match_id": "m1", "seat_index": 0, "reward": 1.0, "value": 0.5, "done": True},
    ]
    gamma = 0.99
    gae_lambda = 0.95
    advantages, returns = compute_gae_for_rows(rows, gamma, gae_lambda)

    # delta_2 = r_2 + gamma * 0 - V(s_2) = 1.0 + 0 - 0.5 = 0.5
    # advantage_2 = delta_2 = 0.5
    # return_2 = V(s_2) + advantage_2 = 0.5 + 0.5 = 1.0
    assert abs(advantages[2] - 0.5) < 1e-6
    assert abs(returns[2] - 1.0) < 1e-6

    # delta_1 = r_1 + gamma * V(s_2) - V(s_1) = 0.2 + 0.99*0.5 - 0.8 = -0.105
    # advantage_1 = delta_1 + gamma*lambda*advantage_2
    delta_1 = 0.2 + gamma * 0.5 - 0.8
    expected_adv1 = delta_1 + gamma * gae_lambda * 0.5
    assert abs(advantages[1] - expected_adv1) < 1e-4
```

- [ ] **Step 3: Add test for `recompute_values_and_gae`**

```python
def test_recompute_values_and_gae(tmp_path):
    import torch
    from rl_dataset import ArenaTrajectoryDataset, compute_gae_for_rows

    jsonl_content = (
        '{"schema_version":1,"match_id":"m1","decision_index":0,"seat_index":0,'
        '"policy_id":"learner","decision_kind":"active_turn",'
        '"tile_planes":' + str([0.0]*340) + ','
        '"scalar_features":' + str([0.0]*12) + ','
        '"discard_sequence":' + str([0.0]*1280) + ','
        '"discard_mask":' + str([True]*34) + ','
        '"claim_mask":' + str([True]*7) + ','
        '"self_kong_mask":' + str([True]*3) + ','
        '"hu_mask":' + str([True, False]) + ','
        '"action_head":"discard","action_index":0,"action_semantic":"discard:w1",'
        '"log_prob":-3.5,"value":0.0,'
        '"reward":0.1,"step_reward":0.1,"terminal_reward":0.0,'
        '"shanten_before":3,"shanten_after":2,'
        '"risk_probs":' + str([0.0]*34) + ','
        '"opponent_tenpai_target":' + str([0.0]*3) + ','
        '"opponent_risk_target":' + str([[0.0]*34]*3) + ','
        '"opponent_risk_mask":' + str([[0.0]*34]*3) + ','
        '"global_tile_planes":' + str([0.0]*1360) + ','
        '"global_scalar_features":' + str([0.0]*20) + ','
        '"done":false}\n'
    )
    jsonl_path = tmp_path / "test.jsonl"
    jsonl_path.write_text(jsonl_content, encoding="utf-8")

    dataset = ArenaTrajectoryDataset(jsonl_path, gamma=0.995, gae_lambda=0.95)

    # Before recomputation: value should be 0.0
    assert float(dataset.rows[0]["value"]) == 0.0

    # Define a mock value function that returns 1.0 for everything
    def mock_value_fn(batch):
        return torch.ones(batch["reward"].shape[0], 1)

    dataset.recompute_values_and_gae(
        mock_value_fn,
        device=torch.device("cpu"),
        batch_size=256,
        gamma=0.995,
        gae_lambda=0.95,
    )

    # After recomputation: value should be 1.0
    assert float(dataset.rows[0]["value"]) == 1.0
    # Advantages and returns should have been recomputed
    assert len(dataset.advantages) == 1
    assert len(dataset.returns) == 1
```

- [ ] **Step 4: Run all tests**

Run: `python -m pytest backend/bot_trainer/v2/test_rl_dataset.py -v -p no:cacheprovider`
Expected: All tests PASS, including new ones.

- [ ] **Step 5: Run the full test suite**

Run: `python -m pytest backend/bot_trainer/v2/test.py backend/bot_trainer/v2/test_model.py backend/bot_trainer/v2/test_dataset.py backend/bot_trainer/v2/test_rl_dataset.py -q -p no:cacheprovider`
Expected: All PASS.

- [ ] **Step 6: Commit**

```bash
git add backend/bot_trainer/v2/test_rl_dataset.py
git commit -m "test(rl_dataset): add GAE correctness tests and value recomputation test"
```

---

### Task 7: Final cleanup and validation

**Files:**
- Modify: `backend/bot_trainer/v2/rl_train.py` (minor cleanup)

- [ ] **Step 1: Remove the now-unused `normalized_advantage_stats` function**

Since advantages are now normalized per-mini-batch in the training loop (not in `forward_and_compute_ppo_loss`), the `normalized_advantage_stats` function is no longer called. Remove it (lines 589-599):

```python
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
```

- [ ] **Step 2: Verify `format_epoch_metrics` handles the new `mini_batch_count` and `value_early_stopped` fields**

In `format_epoch_metrics`, add these to the output string if present:

```python
    if 'mini_batch_count' in metrics:
        base_msg += f" mb_count={int(metrics['mini_batch_count'])}"
    if metrics.get('value_early_stopped'):
        base_msg += " value_early_stopped=true"
```

- [ ] **Step 3: Run full test suite one final time**

Run: `python -m pytest backend/bot_trainer/v2/ -q -p no:cacheprovider`
Expected: All PASS.

- [ ] **Step 4: Commit**

```bash
git add backend/bot_trainer/v2/rl_train.py
git commit -m "cleanup(rl_train): remove unused normalized_advantage_stats, update metrics formatting"
```

---

## Self-Review Checklist

- [x] **Spec coverage:** GAE fix (Task 1+3), training loop restructure (Task 4), hyperparams (Task 2+5), tests (Task 6), cleanup (Task 7)
- [x] **Placeholder scan:** No TBD, TODO, or vague steps. All code is concrete.
- [x] **Type consistency:** `recompute_values_and_gae()` signature matches usage in Task 3. `_make_critic_value_fn` returns callable matching expected signature. `split_into_mini_batches` returns list of dicts matching batch format.
