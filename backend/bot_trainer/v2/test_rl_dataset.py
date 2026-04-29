from __future__ import annotations

import json
from pathlib import Path

from rl_dataset import (
    ArenaTrajectoryDataset,
    compute_discounted_returns_for_rows,
    compute_gae_for_rows,
    compute_returns,
)


def base_trajectory_row(
    policy_id: str,
    seat_index: int,
    reward: float,
    value: float,
    done: bool = True,
) -> dict[str, object]:
    return {
        "schema_version": 1,
        "match_id": "m1",
        "decision_index": seat_index,
        "seat_index": seat_index,
        "policy_id": policy_id,
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
        "log_prob": -0.3,
        "value": value,
        "reward": reward,
        "step_reward": 0.0,
        "terminal_reward": reward,
        "shanten_before": None,
        "shanten_after": None,
        "fan_potential_before": None,
        "fan_potential_after": None,
        "done": done,
    }


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
    assert row["advantage"].item() == 1.0
    assert row["shanten_after"].item() == 0
    assert row["fan_potential_after"].item() == 3
    assert row["tile_planes"].shape == (10, 34)
    assert row["has_global_state"].item() is False


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


def test_dataset_filters_policy_id(tmp_path: Path) -> None:
    path = tmp_path / "trajectories.jsonl"
    rows = [
        base_trajectory_row("learner", 0, reward=1.0, value=0.2),
        base_trajectory_row("opponent", 1, reward=9.0, value=0.0),
    ]
    path.write_text("\n".join(json.dumps(row) for row in rows) + "\n", encoding="utf-8")

    dataset = ArenaTrajectoryDataset(path, policy_id="learner")

    assert len(dataset) == 1
    assert dataset[0]["reward"].item() == 1.0


def test_dataset_accepts_missing_global_state(tmp_path: Path) -> None:
    path = tmp_path / "trajectories.jsonl"
    row = base_trajectory_row("learner", 0, reward=1.0, value=0.0)
    path.write_text(json.dumps(row) + "\n", encoding="utf-8")

    dataset = ArenaTrajectoryDataset(path, policy_id="learner")

    assert dataset[0]["has_global_state"].item() is False


def test_compute_gae_for_rows_is_per_seat_episode() -> None:
    rows = [
        {"match_id": "m1", "seat_index": 0, "reward": 0.0, "value": 0.5},
        {"match_id": "m1", "seat_index": 1, "reward": 5.0, "value": 0.0},
        {"match_id": "m1", "seat_index": 0, "reward": 1.0, "value": 0.25},
    ]

    advantages, returns = compute_gae_for_rows(rows, gamma=1.0, gae_lambda=1.0)

    assert advantages == [0.5, 5.0, 0.75]
    assert returns == [1.0, 5.0, 1.0]


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


def test_entropy_coef_decays_linearly() -> None:
    from rl_train import entropy_coef_for_progress

    assert entropy_coef_for_progress(0, 10, 0.05, 0.005) == 0.05
    assert entropy_coef_for_progress(5, 10, 0.05, 0.005) == 0.0275
    assert entropy_coef_for_progress(10, 10, 0.05, 0.005) == 0.005
    assert entropy_coef_for_progress(20, 10, 0.05, 0.005) == 0.005


def test_epoch_log_line_includes_entropy_metrics() -> None:
    from rl_train import format_epoch_metrics

    line = format_epoch_metrics(
        {
            "epoch": 1,
            "loss": 1.0,
            "policy_loss": 0.5,
            "value_loss": 0.25,
            "entropy": 1.75,
            "entropy_coef": 0.02,
            "kl_loss": 0.03,
            "kl_coef": 0.01,
        },
        total_epochs=3,
    )

    assert "entropy=1.750000" in line
    assert "entropy_coef=0.020000" in line
    assert "kl_loss=0.030000" in line
    assert "kl_coef=0.010000" in line


def test_clipped_value_loss_uses_larger_loss() -> None:
    import torch
    from rl_train import clipped_value_loss

    values = torch.tensor([3.0])
    old_values = torch.tensor([0.0])
    returns = torch.tensor([1.0])

    loss = clipped_value_loss(values, old_values, returns, clip_epsilon=0.2)

    assert loss.item() == 4.0


def test_masked_categorical_kl_is_finite() -> None:
    import torch
    from rl_train import masked_categorical_kl

    teacher_logits = torch.tensor([[1.0, 0.0, 99.0]])
    student_logits = torch.tensor([[0.5, 0.2, -99.0]])
    mask = torch.tensor([[True, True, False]])

    kl = masked_categorical_kl(teacher_logits, student_logits, mask)

    assert torch.isfinite(kl)
    assert kl.item() >= 0.0


def test_league_config_rotates_learner_seat() -> None:
    from league_config import build_trajectory_configs

    learner = {
        "id": "learner",
        "mode": "neural",
        "model_path": "candidate.onnx",
        "sample_actions": True,
        "temperature": 1.0,
    }
    pool = {
        "learner": learner,
        "opponents": [
            {
                "id": "heuristic",
                "mode": "heuristic",
                "model_path": None,
                "sample_actions": False,
                "temperature": 1.0,
                "weight": 1,
            }
        ],
    }

    configs = build_trajectory_configs(pool, matches=8, seed=10, max_actions=2400)

    assert len(configs) == 4
    assert [config["policies"].index(learner) for config in configs] == [0, 1, 2, 3]
    assert all(config["matches"] == 2 for config in configs)


def test_candidate_gate_accepts_safe_improvement() -> None:
    from candidate_gate import evaluate_candidate

    summary = {
        "policies": {
            "baseline_neural": {
                "avg_score_delta": 0.0,
                "win_rate": 0.20,
                "deal_in_rate": 0.10,
                "avg_first_tenpai_turn": 8.0,
                "final_tenpai_rate": 0.55,
                "avg_latency_ms_per_decision": 20.0,
            },
            "rl_candidate_neural": {
                "avg_score_delta": 1.5,
                "win_rate": 0.21,
                "deal_in_rate": 0.11,
                "avg_first_tenpai_turn": 7.8,
                "final_tenpai_rate": 0.55,
                "avg_latency_ms_per_decision": 22.0,
            },
        }
    }

    result = evaluate_candidate(summary, "baseline_neural", "rl_candidate_neural")

    assert result["accepted"] is True


def test_candidate_gate_rejects_higher_deal_in() -> None:
    from candidate_gate import evaluate_candidate

    summary = {
        "policies": {
            "baseline_neural": {
                "avg_score_delta": 0.0,
                "win_rate": 0.20,
                "deal_in_rate": 0.10,
                "avg_first_tenpai_turn": 8.0,
                "final_tenpai_rate": 0.55,
                "avg_latency_ms_per_decision": 20.0,
            },
            "rl_candidate_neural": {
                "avg_score_delta": 2.0,
                "win_rate": 0.22,
                "deal_in_rate": 0.14,
                "avg_first_tenpai_turn": 7.7,
                "final_tenpai_rate": 0.56,
                "avg_latency_ms_per_decision": 23.0,
            },
        }
    }

    result = evaluate_candidate(summary, "baseline_neural", "rl_candidate_neural")

    assert result["accepted"] is False
    assert "deal_in_rate" in result["failures"]


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
