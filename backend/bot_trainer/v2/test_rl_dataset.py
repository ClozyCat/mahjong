from __future__ import annotations

import json
from pathlib import Path

import pytest

from rl_dataset import (
    ArenaTrajectoryDataset,
    DISCARD_EVENT_FEATURE_COUNT,
    DISCARD_SEQUENCE_LENGTH,
    build_tensor_cache,
    compute_discounted_returns_for_rows,
    compute_gae_for_rows,
    compute_returns,
)

DISCARD_SEQUENCE_SIZE = DISCARD_SEQUENCE_LENGTH * DISCARD_EVENT_FEATURE_COUNT


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
        "scalar_features": [0.0] * 12,
        "discard_sequence": [0.0] * DISCARD_SEQUENCE_SIZE,
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
        "scalar_features": [0.0] * 12,
        "discard_sequence": [0.0] * DISCARD_SEQUENCE_SIZE,
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
    assert row["scalar_features"].shape == (12,)
    assert row["discard_sequence"].shape == (
        DISCARD_SEQUENCE_LENGTH,
        DISCARD_EVENT_FEATURE_COUNT,
    )
    assert row["has_global_state"].item() is False


def test_loads_trajectory_jsonl_with_utf8_bom(tmp_path: Path) -> None:
    path = tmp_path / "trajectories.jsonl"
    row = base_trajectory_row("learner", 0, reward=1.0, value=0.0)
    path.write_text("\ufeff" + json.dumps(row) + "\n", encoding="utf-8")

    dataset = ArenaTrajectoryDataset(path)

    assert len(dataset) == 1
    assert dataset[0]["reward"].item() == 1.0


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


def test_dataset_writes_and_reuses_tensor_cache(tmp_path: Path) -> None:
    torch = pytest.importorskip("torch")
    path = tmp_path / "trajectories.jsonl"
    cache_path = tmp_path / "trajectories.pt"
    row = base_trajectory_row("learner", 0, reward=1.0, value=0.2)
    path.write_text(json.dumps(row) + "\n", encoding="utf-8")

    dataset = ArenaTrajectoryDataset(path, cache_path=cache_path, policy_id="learner")

    assert cache_path.exists()
    assert dataset[0]["reward"].item() == 1.0

    cached_dataset = ArenaTrajectoryDataset(path, cache_path=cache_path, policy_id="learner")

    assert len(cached_dataset) == 1
    assert torch.equal(cached_dataset[0]["tile_planes"], dataset[0]["tile_planes"])


def test_tensor_cache_rebuilds_when_policy_filter_changes(tmp_path: Path) -> None:
    path = tmp_path / "trajectories.jsonl"
    cache_path = tmp_path / "trajectories.pt"
    rows = [
        base_trajectory_row("learner", 0, reward=1.0, value=0.2),
        base_trajectory_row("opponent", 1, reward=9.0, value=0.0),
    ]
    path.write_text("\n".join(json.dumps(row) for row in rows) + "\n", encoding="utf-8")

    build_tensor_cache(path, cache_path, gamma=0.995, gae_lambda=0.95, policy_id="learner")
    dataset = ArenaTrajectoryDataset(path, cache_path=cache_path, policy_id="opponent")

    assert len(dataset) == 1
    assert dataset[0]["reward"].item() == 9.0


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


def test_epoch_checkpoint_name_is_zero_padded() -> None:
    from rl_train import epoch_checkpoint_name

    assert epoch_checkpoint_name(1) == "epoch_001.pt"
    assert epoch_checkpoint_name(12) == "epoch_012.pt"


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
    assert all(config["record_heuristic_comparison"] is False for config in configs)


def test_rollout_override_keeps_neural_opponents_frozen() -> None:
    from league_config import apply_rollout_model_override

    pool = {
        "learner": {
            "id": "learner",
            "mode": "neural",
            "model_path": "backend/assets/models/mahjong_policy_net.onnx",
            "sample_actions": True,
            "temperature": 1.0,
        },
        "opponents": [
            {
                "id": "sft_default",
                "mode": "neural",
                "model_path": "backend/assets/history_models/sft.onnx",
                "sample_actions": False,
                "temperature": 1.0,
                "weight": 1,
            },
            {
                "id": "heuristic",
                "mode": "heuristic",
                "model_path": None,
                "sample_actions": False,
                "temperature": 1.0,
                "weight": 1,
            },
        ],
    }

    apply_rollout_model_override(pool, Path("runs/iter_001/candidate.onnx"))

    assert pool["learner"]["model_path"] == "runs/iter_001/candidate.onnx"
    assert pool["opponents"][0]["model_path"] == "backend/assets/history_models/sft.onnx"
    assert pool["opponents"][1]["model_path"] is None


def test_eval_config_disables_heuristic_comparison() -> None:
    from league_config import build_eval_config

    config = build_eval_config(
        candidate_onnx=Path("candidate.onnx"),
        baseline_onnx=Path("baseline.onnx"),
        matches=4,
        seed=10,
        max_actions=2400,
    )

    assert config["record_heuristic_comparison"] is False


def test_eval_config_uses_cyclic_rotation() -> None:
    from league_config import build_eval_config

    config = build_eval_config(
        candidate_onnx=Path("candidate.onnx"),
        baseline_onnx=Path("sft.onnx"),
        matches=1000,
        seed=20260502,
        max_actions=2400,
    )

    assert config["seat_rotation"] == "cyclic"
    assert config["seat_rotation_offset"] == 0
    assert [policy["id"] for policy in config["policies"]] == [
        "baseline_neural",
        "rl_candidate_neural",
    ]


def test_baseline_guard_rejects_rl_checkpoint_as_sft(tmp_path: Path) -> None:
    torch = pytest.importorskip("torch")
    from baseline_guard import validate_baseline_checkpoint

    checkpoint = tmp_path / "best.pt"
    torch.save(
        {
            "model_state": {},
            "model_config": {"tile_plane_count": 10, "scalar_feature_count": 12},
            "rl_metrics": [],
        },
        checkpoint,
    )

    with pytest.raises(ValueError, match="RL checkpoint"):
        validate_baseline_checkpoint(checkpoint, allow_rl_checkpoint=False)


def test_rollout_state_keeps_previous_best_when_candidate_regresses() -> None:
    from candidate_selector import choose_next_rollout

    current = {
        "checkpoint": "runs/iter_001/checkpoints/best.pt",
        "onnx": "runs/iter_001/candidate.onnx",
        "score_margin": 0.4,
    }
    candidate = {
        "checkpoint": "runs/iter_002/checkpoints/best.pt",
        "onnx": "runs/iter_002/candidate.onnx",
        "score_margin": -0.2,
        "accepted": False,
    }

    selected = choose_next_rollout(current, candidate)

    assert selected == current


def test_rollout_state_advances_when_candidate_is_accepted() -> None:
    from candidate_selector import choose_next_rollout

    current = {
        "checkpoint": "runs/iter_001/checkpoints/best.pt",
        "onnx": "runs/iter_001/candidate.onnx",
        "score_margin": 0.4,
    }
    candidate = {
        "checkpoint": "runs/iter_002/checkpoints/best.pt",
        "onnx": "runs/iter_002/candidate.onnx",
        "score_margin": 0.1,
        "accepted": True,
    }

    selected = choose_next_rollout(current, candidate)

    assert selected["checkpoint"] == candidate["checkpoint"]
    assert selected["onnx"] == candidate["onnx"]


def test_discard_log_probs_use_risk_adjusted_logits() -> None:
    import math
    import torch
    from rl_train import select_action_log_probs

    outputs = {
        "discard_logits": torch.tensor([[0.0, 0.0] + [-100.0] * 32]),
        "claim_logits": torch.zeros((1, 7)),
        "self_kong_logits": torch.zeros((1, 3)),
        "hu_logits": torch.zeros((1, 2)),
        "value": torch.tensor([[-8.0]]),
        "risk_logits": torch.tensor([[5.0, -5.0] + [0.0] * 32]),
    }
    batch = {
        "reward": torch.tensor([0.0]),
        "action_head": torch.tensor([0]),
        "action_index": torch.tensor([0]),
        "discard_mask": torch.tensor([[True, True] + [False] * 32]),
        "claim_mask": torch.zeros((1, 7), dtype=torch.bool),
        "self_kong_mask": torch.zeros((1, 3), dtype=torch.bool),
        "hu_mask": torch.tensor([[True, False]]),
    }

    log_prob = select_action_log_probs(outputs, batch)

    risk_weight = 1.45
    first = -risk_weight * (1.0 / (1.0 + math.exp(-5.0)))
    second = -risk_weight * (1.0 / (1.0 + math.exp(5.0)))
    expected = first - max(first, second) - math.log(
        math.exp(first - max(first, second)) + math.exp(second - max(first, second))
    )
    assert log_prob.item() == pytest.approx(expected, abs=1e-5)


def test_candidate_gate_rejects_excessive_claim_rate() -> None:
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
                "avg_claims": 2.0,
                "same_as_heuristic_rate": 0.40,
            },
            "rl_candidate_neural": {
                "avg_score_delta": 1.5,
                "win_rate": 0.21,
                "deal_in_rate": 0.11,
                "avg_first_tenpai_turn": 7.8,
                "final_tenpai_rate": 0.55,
                "avg_latency_ms_per_decision": 22.0,
                "avg_claims": 6.0,
                "same_as_heuristic_rate": 0.39,
            },
        }
    }

    result = evaluate_candidate(summary, "baseline_neural", "rl_candidate_neural")

    assert result["accepted"] is False
    assert "claim_rate" in result["failures"]


def test_trajectory_diagnostics_reports_reward_breakdown() -> None:
    from rl_dataset import trajectory_diagnostics

    rows = [
        base_trajectory_row("learner", 0, reward=0.1, value=0.0, done=False),
        {
            **base_trajectory_row("learner", 0, reward=1.2, value=0.0, done=True),
            "step_reward": 0.2,
            "terminal_reward": 1.0,
            "action_head": "claim",
            "shanten_before": 2,
            "shanten_after": 1,
            "fan_potential_before": 1,
            "fan_potential_after": 2,
        },
    ]

    diagnostics = trajectory_diagnostics(rows)

    assert diagnostics["row_count"] == 2
    assert diagnostics["action_head_claim"] == 1
    assert diagnostics["terminal_reward_mean"] == pytest.approx(0.5)
    assert diagnostics["step_reward_abs_sum"] == pytest.approx(0.2)
    assert diagnostics["shanten_improvement_count"] == 1
    assert diagnostics["fan_potential_improvement_count"] == 1


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


def test_candidate_gate_does_not_limit_latency() -> None:
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
                "avg_latency_ms_per_decision": 2000.0,
            },
        }
    }

    result = evaluate_candidate(summary, "baseline_neural", "rl_candidate_neural")

    assert result["accepted"] is True
    assert "latency" not in result["failures"]


def test_candidate_selector_prefers_accepted_candidate() -> None:
    from candidate_selector import select_best_candidate

    rejected = {
        "epoch": 1,
        "checkpoint": "epoch_001.pt",
        "onnx": "epoch_001.onnx",
        "gate": {
            "accepted": False,
            "failures": ["avg_score_delta"],
            "baseline": {
                "avg_score_delta": 10.0,
                "win_rate": 0.30,
                "deal_in_rate": 0.12,
                "avg_first_tenpai_turn": 10.0,
                "final_tenpai_rate": 0.60,
                "avg_latency_ms_per_decision": 70.0,
            },
            "candidate": {
                "avg_score_delta": 9.0,
                "win_rate": 0.32,
                "deal_in_rate": 0.11,
                "avg_first_tenpai_turn": 9.8,
                "final_tenpai_rate": 0.62,
                "avg_latency_ms_per_decision": 72.0,
            },
        },
    }
    accepted = {
        "epoch": 2,
        "checkpoint": "epoch_002.pt",
        "onnx": "epoch_002.onnx",
        "gate": {
            "accepted": True,
            "failures": [],
            "baseline": rejected["gate"]["baseline"],
            "candidate": {
                "avg_score_delta": 10.5,
                "win_rate": 0.31,
                "deal_in_rate": 0.13,
                "avg_first_tenpai_turn": 10.0,
                "final_tenpai_rate": 0.60,
                "avg_latency_ms_per_decision": 74.0,
            },
        },
    }

    selected = select_best_candidate([rejected, accepted])

    assert selected["epoch"] == 2
    assert selected["accepted"] is True


def test_candidate_selector_uses_margin_score_when_all_rejected() -> None:
    from candidate_selector import select_best_candidate

    baseline = {
        "avg_score_delta": 10.0,
        "win_rate": 0.30,
        "deal_in_rate": 0.12,
        "avg_first_tenpai_turn": 10.0,
        "final_tenpai_rate": 0.60,
        "avg_latency_ms_per_decision": 70.0,
    }
    worse = {
        "epoch": 1,
        "checkpoint": "epoch_001.pt",
        "onnx": "epoch_001.onnx",
        "gate": {
            "accepted": False,
            "failures": ["avg_score_delta", "win_rate"],
            "baseline": baseline,
            "candidate": {
                "avg_score_delta": 2.0,
                "win_rate": 0.20,
                "deal_in_rate": 0.13,
                "avg_first_tenpai_turn": 10.5,
                "final_tenpai_rate": 0.58,
                "avg_latency_ms_per_decision": 72.0,
            },
        },
    }
    closer = {
        "epoch": 2,
        "checkpoint": "epoch_002.pt",
        "onnx": "epoch_002.onnx",
        "gate": {
            "accepted": False,
            "failures": ["avg_score_delta"],
            "baseline": baseline,
            "candidate": {
                "avg_score_delta": 8.0,
                "win_rate": 0.31,
                "deal_in_rate": 0.13,
                "avg_first_tenpai_turn": 10.1,
                "final_tenpai_rate": 0.60,
                "avg_latency_ms_per_decision": 72.0,
            },
        },
    }

    selected = select_best_candidate([worse, closer])

    assert selected["epoch"] == 2
    assert selected["accepted"] is False
    assert selected["score_margin"] == -2.0


def test_candidate_selector_preserves_play_style_metadata() -> None:
    from candidate_selector import select_best_candidate

    baseline = {
        "avg_score_delta": 0.0,
        "win_rate": 0.30,
        "deal_in_rate": 0.12,
        "avg_first_tenpai_turn": 10.0,
        "final_tenpai_rate": 0.60,
        "avg_latency_ms_per_decision": 70.0,
    }
    selected = select_best_candidate([
        {
            "epoch": 1,
            "play_style": "defensive",
            "checkpoint": "defensive/epoch_001.pt",
            "onnx": "defensive/epoch_001.onnx",
            "gate": {
                "accepted": True,
                "failures": [],
                "baseline": baseline,
                "candidate": {
                    "avg_score_delta": 1.0,
                    "win_rate": 0.31,
                    "deal_in_rate": 0.11,
                    "avg_first_tenpai_turn": 9.8,
                    "final_tenpai_rate": 0.62,
                    "avg_latency_ms_per_decision": 72.0,
                },
            },
        }
    ])

    assert selected["play_style"] == "defensive"
    assert selected["selected"]["play_style"] == "defensive"


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
                "model_loaded": True,
                "fallback_count": 1,
                "neural_action_count": 3,
                "same_as_heuristic_count": 2,
                "heuristic_comparison_count": 3,
                "same_as_heuristic_rate": 2 / 3,
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
    assert summary["policies"]["a"]["model_loaded_seats"] == 1
    assert summary["policies"]["a"]["fallback_count"] == 1
    assert summary["policies"]["a"]["neural_action_count"] == 3
    assert summary["policies"]["a"]["same_as_heuristic_rate"] == 2 / 3
    assert summary["policies"]["b"]["deal_in_rate"] == 1.0
    assert summary["policies"]["b"]["avg_first_tenpai_turn"] is None
