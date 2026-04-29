from __future__ import annotations

import json
from pathlib import Path

from rl_dataset import (
    ArenaTrajectoryDataset,
    compute_discounted_returns_for_rows,
    compute_returns,
)


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
        "step_reward": 0.0,
        "terminal_reward": 1.0,
        "shanten_before": 1,
        "shanten_after": 0,
        "fan_potential_before": 2,
        "fan_potential_after": 3,
        "done": True,
    }
    path.write_text(json.dumps(row) + "\n", encoding="utf-8")

    dataset = ArenaTrajectoryDataset(path)
    row = dataset[0]

    assert row["action_index"].item() == 0
    assert row["reward"].item() == 1.0
    assert row["return"].item() == 1.0
    assert row["shanten_after"].item() == 0
    assert row["fan_potential_after"].item() == 3
    assert row["tile_planes"].shape == (10, 34)


def test_compute_returns_resets_on_done() -> None:
    returns = compute_returns(
        [0.0, 1.0, 0.0, 2.0],
        [False, True, False, True],
        gamma=0.9,
    )

    assert returns == [0.9, 1.0, 1.8, 2.0]


def test_compute_discounted_returns_are_per_seat_episode() -> None:
    rows = [
        {"match_id": "m1", "seat_index": 0, "reward": 0.0},
        {"match_id": "m1", "seat_index": 1, "reward": 10.0},
        {"match_id": "m1", "seat_index": 0, "reward": 1.0},
        {"match_id": "m1", "seat_index": 1, "reward": 0.0},
    ]

    returns = compute_discounted_returns_for_rows(rows, gamma=0.9)

    assert returns == [0.9, 10.0, 1.0, 0.0]


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


def test_arena_summary_aggregates_policy_metrics(tmp_path: Path) -> None:
    from arena_summary import load_reports, summarize_reports

    path = tmp_path / "arena.jsonl"
    report = {
        "match_index": 0,
        "seed": 1,
        "completed": True,
        "action_count": 10,
        "seats": [
            {
                "seat_index": 0,
                "policy_id": "a",
                "score_delta": 10,
                "wins": 1,
                "dealt_in": 0,
                "first_tenpai_turn": 4,
                "final_tenpai": True,
                "claim_count": 1,
                "discard_count": 2,
                "decision_count": 4,
                "decision_latency_ms_sum": 20,
            },
            {
                "seat_index": 1,
                "policy_id": "b",
                "score_delta": -10,
                "wins": 0,
                "dealt_in": 1,
                "first_tenpai_turn": None,
                "final_tenpai": False,
                "claim_count": 0,
                "discard_count": 3,
                "decision_count": 5,
                "decision_latency_ms_sum": 50,
            },
        ],
    }
    path.write_text(json.dumps(report) + "\n", encoding="utf-8")

    summary = summarize_reports(load_reports(path))

    assert summary["matches"] == 1
    assert summary["policies"]["a"]["avg_score_delta"] == 10.0
    assert summary["policies"]["a"]["win_rate"] == 1.0
    assert summary["policies"]["a"]["avg_first_tenpai_turn"] == 4.0
    assert summary["policies"]["b"]["deal_in_rate"] == 1.0
    assert summary["policies"]["b"]["avg_first_tenpai_turn"] is None
