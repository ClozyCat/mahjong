# Design: AWR Phase 2 — Model Robustness (Hierarchical + Value + KL)

**Date:** 2026-06-16
**Status:** Approved
**Scope:** Per-head loss weights for action imbalance, value training with score bucket auxiliary head, KL divergence conservative penalty against SFT reference. Builds on Phase 1 (already completed).

## Motivation

Phase 1 improved training signal quality (diverse opponents, normalized advantages, tuned weights). Phase 2 addresses three remaining model-side weaknesses:

1. **Action head imbalance**: discard actions dominate 85% of training samples; claim/kong/hu heads barely receive gradient signal
2. **Value head weakness**: single MSE head on noisy MC returns is the sole support for AWR advantage estimation
3. **No drift guard**: AWR only reinforces advantage>0 actions; if value estimates drift, policy can silently degrade

## Design

### 1. Hierarchical Training — Per-Head Weights

**`train_awr.py`** — new CLI arg `--head-weights` (default `1.0,3.0,5.0,5.0`):

```
--head-weights 1.0,3.0,5.0,5.0   # discard, claim, self_kong, hu
```

Weighted policy loss computation replaces current equal weighting:

```python
head_weights = [float(w) for w in args.head_weights.split(",")]
# In training loop, per-batch:
policy_loss = 0.0
weight_sum = 0.0
for head_idx, weight in enumerate(head_weights):
    mask_t = action_head == head_idx
    if mask_t.any():
        loss = compute_ce_loss_for_action(...)
        policy_loss += weight * loss
        weight_sum += weight
if weight_sum > 0:
    policy_loss = policy_loss / weight_sum
```

### 2. Value Training — Score Bucket Auxiliary

**`model.py`** — add `score_bucket_head` (training-only, not exported to ONNX):

```python
# In LightweightActor.__init__:
self.score_bucket_head = HeadMLP(256, 5)  # 5 score buckets

# In LightweightActor.forward, add to output dict:
"score_bucket_logits": self.score_bucket_head(policy_hidden),

# Add to TRAINING_ONLY_HEADS:
TRAINING_ONLY_HEADS = {"value", "score_bucket_logits"}
```

**`train_value.py`** — new args + loss:

| Parameter | Default | Description |
|-----------|---------|-------------|
| `--epochs` | 30 | Was 10 |
| `--lr` | 5e-4 | Was 1e-3 |
| `--score-bucket-weight` | 0.1 | Auxiliary loss weight |

Score buckets (based on terminal reward):
- Bucket 0: ≤ -1.5 (dealt in)
- Bucket 1: -1.5 ~ -0.5 (big loss)
- Bucket 2: -0.5 ~ +0.5 (neutral)
- Bucket 3: +0.5 ~ +1.5 (big win)
- Bucket 4: > +1.5 (win + dealt in multiple times, rare)

```python
loss = mse_loss(value, returns) + args.score_bucket_weight * ce_loss(score_bucket_logits, bucket_targets)
```

Only rows with `done=True` contribute to score_bucket loss (other rows use ignore_index=-100).

### 3. KL Conservative Penalty

**`train_awr.py`** — load frozen SFT reference model, compute masked KL:

CLI args:
```python
parser.add_argument("--kl-coef", type=float, default=0.01)
parser.add_argument("--sft-checkpoint", type=Path, default=None,
                    help="SFT checkpoint for KL reference; defaults to --checkpoint")
```

KL computation (masked categorical, per head, averaged):
```python
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
```

Total loss:
```
total_loss = policy_loss + 0.5 * value_loss + kl_coef * kl_loss
```

KL is computed on ALL samples in the batch (not just advantage>0), acting as a global drift guard.

### Files Changed

| File | Change |
|------|--------|
| `model.py` | Add `score_bucket_head` (HeadMLP(256, 5)), add to forward output and TRAINING_ONLY_HEADS |
| `train_awr.py` | Add `--head-weights`, `--kl-coef`, `--sft-checkpoint`; weighted policy loss; KL computation |
| `train_value.py` | Change defaults (epochs 30, lr 5e-4); add `--score-bucket-weight`, score bucket loss |

### Files NOT Changed

| File | Reason |
|------|--------|
| `awr_dataset.py` | No new data fields needed |
| `export_onnx.py` | Score bucket head is training-only, not exported |
| `league_config.py` | No changes |
| `opponent_pool.json` | No changes |
| `test_model.py` | Already tests TRAINING_ONLY_HEADS |

### Testing

- Unit test: `score_bucket_logits` output shape = (B, 5), present in forward pass
- Unit test: `masked_categorical_kl` with identical inputs → KL ≈ 0
- Unit test: `masked_categorical_kl` with maximally different inputs → KL > 0
- Integration: `train_awr.py --help` shows new params
- Integration: `train_value.py --help` shows new params and defaults

### Risks

- **KL coefficient too large**: If kl_coef=0.01 is too aggressive, policy won't improve. Mitigation: start at 0.001, increase if policy_loss not decreasing.
- **Score bucket boundary mismatch**: Terminal rewards vary by match. Mitigation: use fixed buckets based on observed reward distribution from Phase 1 trajectories.
- **Per-head weights distort loss scale**: Warming up weights from 1.0 to target over first few epochs could help. Mitigation: add `--head-weight-warmup-epochs` (default 2).
