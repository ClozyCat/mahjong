from __future__ import annotations

import json
from pathlib import Path

from rl_dataset import ArenaTrajectoryDataset, compute_returns


def test_loads_trajectory_row(tmp_path: Path) -> None:
    path = tmp_path / "trajectories.jsonl"
    row = {
        "schema_version": 1,
        "match_id": "m1",
        "decision_index": 0,
        "seat_index": 0,
        "policy_id": "p",
        "decision_kind": "active_turn",
        "tile_planes": [0.0] * 340,
        "scalar_features": [0.0] * 10,
        "discard_mask": [True] + [False] * 33,
        "claim_mask": [True] + [False] * 6,
        "self_kong_mask": [True, False, False],
        "hu_mask": [True, False],
        "action_head": "discard",
        "action_index": 0,
        "action_semantic": "discard:w1",
        "log_prob": 0.0,
        "value": 0.0,
        "reward": 1.0,
        "done": True,
    }
    path.write_text(json.dumps(row) + "\n", encoding="utf-8")

    dataset = ArenaTrajectoryDataset(path)
    row = dataset[0]

    assert row["action_index"].item() == 0
    assert row["reward"].item() == 1.0
    assert row["tile_planes"].shape == (10, 34)


def test_compute_returns_resets_on_done() -> None:
    returns = compute_returns(
        [0.0, 1.0, 0.0, 2.0],
        [False, True, False, True],
        gamma=0.9,
    )

    assert returns == [0.9, 1.0, 1.8, 2.0]


def test_masked_ppo_loss_is_finite() -> None:
    import torch
    from rl_train import masked_head_log_probs, ppo_policy_loss

    logits = torch.tensor([[2.0, 0.0, -5.0]])
    mask = torch.tensor([[True, True, False]])
    actions = torch.tensor([0])
    old_log_probs = torch.tensor([-0.2])
    advantages = torch.tensor([1.0])

    log_probs = masked_head_log_probs(logits, mask, actions)
    loss = ppo_policy_loss(log_probs, old_log_probs, advantages, clip_epsilon=0.2)

    assert torch.isfinite(loss)
