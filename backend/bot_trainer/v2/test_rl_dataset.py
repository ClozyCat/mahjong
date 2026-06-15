from __future__ import annotations

import json
from pathlib import Path

import pytest

from rl_dataset import (
    ArenaTrajectoryDataset,
    DISCARD_EVENT_FEATURE_COUNT,
    DISCARD_SEQUENCE_LENGTH,
    build_tensor_cache,
    compute_shaped_reward,
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
        "risk_probs": [0.0] * 34,
        "opponent_tenpai_target": [0.0, 0.0, 0.0],
        "opponent_risk_target": [[0.0] * 34 for _ in range(3)],
        "opponent_risk_mask": [[1.0] * 34 for _ in range(3)],
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
        "risk_probs": [0.0] * 34,
        "opponent_tenpai_target": [0.0, 0.0, 0.0],
        "opponent_risk_target": [[0.0] * 34 for _ in range(3)],
        "opponent_risk_mask": [[0.0] * 34 for _ in range(3)],
        "done": True,
    }
    path.write_text(json.dumps(row) + "\n", encoding="utf-8")

    dataset = ArenaTrajectoryDataset(path)
    row = dataset[0]

    assert row["action_index"].item() == 0
    assert row["reward"].item() == 1.0
    assert row["return"].item() == pytest.approx(1.0)
    assert row["advantage"].item() == pytest.approx(1.0)
    assert row["shanten_after"].item() == 0
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


def test_trajectory_dataset_encodes_risk_and_opponent_targets(tmp_path: Path) -> None:
    path = tmp_path / "trajectories.jsonl"
    row = base_trajectory_row("learner", 0, reward=0.0, value=0.0)
    row["risk_probs"] = [0.9, 0.1] + [0.0] * 32
    row["opponent_tenpai_target"] = [1.0, 0.0, 1.0]
    row["opponent_risk_target"] = [
        [1.0, 0.0] + [0.0] * 32,
        [0.0] * 34,
        [0.0, 1.0] + [0.0] * 32,
    ]
    row["opponent_risk_mask"] = [
        [1.0, 1.0] + [0.0] * 32,
        [0.0] * 34,
        [1.0, 1.0] + [0.0] * 32,
    ]
    path.write_text(json.dumps(row) + "\n", encoding="utf-8")

    dataset = ArenaTrajectoryDataset(path)
    sample = dataset[0]

    assert sample["risk_probs"].shape == (34,)
    assert sample["risk_probs"][0].item() == pytest.approx(0.9)
    assert sample["opponent_tenpai_target"].tolist() == [1.0, 0.0, 1.0]
    assert sample["opponent_risk_target"].shape == (3, 34)
    assert sample["opponent_risk_mask"].shape == (3, 34)
    assert sample["opponent_risk_target"][2, 1].item() == 1.0
    assert sample["opponent_risk_mask"][1].any().item() is False


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


def test_compute_shaped_reward_does_not_double_count_step_reward() -> None:
    row = {
        "reward": 0.25,
        "step_reward": 0.25,
        "terminal_reward": 0.0,
        "shanten_before": None,
        "shanten_after": None,
        "action_head": "discard",
        "action_index": 0,
    }

    assert compute_shaped_reward(row) == 0.25


def test_compute_shaped_reward_accepts_complete_hand_shanten() -> None:
    row = {
        "reward": 0.06,
        "step_reward": 0.06,
        "terminal_reward": 0.0,
        "shanten_before": 0,
        "shanten_after": -1,
        "action_head": "claim",
        "action_index": 0,
    }

    assert compute_shaped_reward(row) == pytest.approx(0.06)


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


def test_league_config_preserves_original_onnx_paths(tmp_path: Path) -> None:
    import league_config
    from league_config import apply_rollout_model_override, build_eval_config

    rollout = tmp_path / "rollout.onnx"
    rollout.write_text("fp32", encoding="utf-8")
    rollout.with_name("rollout.quant.onnx").write_text("quant", encoding="utf-8")
    pool = {
        "learner": {"id": "learner", "model_path": "old.onnx"},
        "opponents": [
            {"id": "a", "model_path": rollout.as_posix()},
            {"id": "b", "model_path": rollout.as_posix()},
            {"id": "c", "model_path": rollout.as_posix()},
        ],
    }

    apply_rollout_model_override(pool, rollout)
    config = build_eval_config(pool, rollout, rollout, matches=1, seed=1, max_actions=10)

    assert pool["learner"]["model_path"] == rollout.as_posix()
    assert config["subjects"][0]["model_path"] == rollout.as_posix()
    assert config["subjects"][1]["model_path"] == rollout.as_posix()
    assert not hasattr(league_config, "resolve_quantized")
    assert not hasattr(league_config, "resolve_pool_model_paths")


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


def test_league_config_uses_single_evaluation_trajectory_config() -> None:
    from league_config import build_trajectory_configs

    learner = {
        "id": "learner",
        "model_path": "candidate.onnx",
        "sample_actions": True,
        "temperature": 1.0,
    }
    pool = {
        "learner": learner,
        "opponents": [
            {
                "id": "sft_one",
                "model_path": "backend/assets/sft/sft.onnx",
                "sample_actions": False,
                "temperature": 1.0,
                "weight": 3,
            },
            {
                "id": "sft_two",
                "model_path": "backend/assets/sft/sft.onnx",
                "sample_actions": False,
                "temperature": 1.0,
                "weight": 1,
            },
            {
                "id": "sft_three",
                "model_path": "backend/assets/sft/sft.onnx",
                "sample_actions": False,
                "temperature": 1.0,
                "weight": 1,
            }
        ],
    }

    configs = build_trajectory_configs(pool, matches=8, seed=10, max_actions=2400)

    assert len(configs) == 1
    config = configs[0]
    assert config["matches"] == 8
    assert config["seed"] == 10
    assert config["report_trajectories"] is True
    assert "policies" not in config
    assert "seat_rotation" not in config
    assert config["subjects"] == [
        {
            **learner,
            "display_name": "Learner",
        }
    ]
    assert [opponent["id"] for opponent in config["opponents"]] == [
        "sft_one",
        "sft_two",
        "sft_three",
    ]
    assert all("weight" not in opponent for opponent in config["opponents"])
    assert "record_heuristic_comparison" not in config


def test_rollout_override_keeps_neural_opponents_frozen() -> None:
    from league_config import apply_rollout_model_override

    pool = {
        "learner": {
            "id": "learner",
            "model_path": "backend/assets/sft/sft.onnx",
            "sample_actions": True,
            "temperature": 1.0,
        },
        "opponents": [
            {
                "id": "sft_default",
                "model_path": "backend/assets/sft/sft.onnx",
                "sample_actions": False,
                "temperature": 1.0,
                "weight": 1,
            },
        ],
    }

    apply_rollout_model_override(pool, Path("runs/iter_001/candidate.onnx"))

    assert pool["learner"]["model_path"] == "runs/iter_001/candidate.onnx"
    assert pool["opponents"][0]["model_path"] == "backend/assets/sft/sft.onnx"


def test_eval_config_has_no_heuristic_comparison() -> None:
    from league_config import build_eval_config

    pool = {
        "opponents": [
            {"id": "opp1", "model_path": "opp1.onnx"},
            {"id": "opp2", "model_path": "opp2.onnx"},
            {"id": "opp3", "model_path": "opp3.onnx"},
        ]
    }
    config = build_eval_config(
        pool=pool,
        candidate_onnx=Path("candidate.onnx"),
        baseline_onnx=Path("baseline.onnx"),
        matches=4,
        seed=10,
        max_actions=2400,
    )

    assert "record_heuristic_comparison" not in config


def test_eval_config_compares_candidate_and_baseline_against_same_opponents() -> None:
    from league_config import build_eval_config

    pool = {
        "opponents": [
            {
                "id": "sft_one",
                "model_path": "backend/assets/sft/sft.onnx",
                "sample_actions": False,
                "temperature": 1.0,
                "weight": 3,
            },
            {
                "id": "sft_two",
                "model_path": "backend/assets/sft/sft.onnx",
                "sample_actions": False,
                "temperature": 1.0,
                "weight": 1,
            },
            {
                "id": "sft_three",
                "model_path": "backend/assets/sft/sft.onnx",
                "sample_actions": False,
                "temperature": 1.0,
                "weight": 1,
            },
        ]
    }
    config = build_eval_config(
        pool=pool,
        candidate_onnx=Path("candidate.onnx"),
        baseline_onnx=Path("sft.onnx"),
        matches=1000,
        seed=20260502,
        max_actions=2400,
    )

    assert "seat_rotation" not in config
    assert [subject["id"] for subject in config["subjects"]] == [
        "baseline_neural",
        "rl_candidate_neural",
    ]
    assert [opponent["id"] for opponent in config["opponents"]] == [
        "sft_one",
        "sft_two",
        "sft_three",
    ]
    assert all("weight" not in opponent for opponent in config["opponents"])


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


def test_actor_critic_training_rejects_shared_policy_checkpoint(tmp_path: Path) -> None:
    import torch
    from rl_train import validate_checkpoint_architecture

    checkpoint = tmp_path / "shared_policy.pt"
    torch.save({"model_state": {"policy_trunk.0.weight": torch.zeros((1, 1))}}, checkpoint)

    with pytest.raises(SystemExit, match="requires an actor-critic checkpoint"):
        validate_checkpoint_architecture(checkpoint, use_actor_critic=True)


def test_shared_policy_training_rejects_actor_critic_checkpoint(tmp_path: Path) -> None:
    import torch
    from rl_train import validate_checkpoint_architecture

    checkpoint = tmp_path / "actor_critic.pt"
    torch.save({"model_state": {"actor.policy_trunk.0.weight": torch.zeros((1, 1))}}, checkpoint)

    with pytest.raises(SystemExit, match="requires a shared policy checkpoint"):
        validate_checkpoint_architecture(checkpoint, use_actor_critic=False)


def test_prepare_model_for_ppo_updates_disables_dropout_without_freezing_params() -> None:
    import torch
    from rl_train import prepare_model_for_ppo_updates

    model = torch.nn.Sequential(
        torch.nn.Linear(2, 2),
        torch.nn.Dropout(0.5),
        torch.nn.LayerNorm(2),
    )

    prepare_model_for_ppo_updates(model)

    assert model.training is True
    assert model[1].training is False
    assert model[0].training is True
    assert all(parameter.requires_grad for parameter in model.parameters())


def test_actor_critic_lr_warmup_preserves_critic_multiplier() -> None:
    import torch
    from rl_train import apply_lr_warmup

    actor = torch.nn.Linear(1, 1)
    critic = torch.nn.Linear(1, 1)
    optimizer = torch.optim.AdamW(
        [
            {"params": actor.parameters(), "lr": 3e-6, "name": "actor"},
            {"params": critic.parameters(), "lr": 6e-6, "name": "critic"},
        ]
    )

    apply_lr_warmup(
        optimizer,
        epoch=0,
        warmup_epochs=3,
        actor_lr=3e-6,
        critic_lr_multiplier=2.0,
    )

    assert optimizer.param_groups[0]["lr"] == pytest.approx(1e-6)
    assert optimizer.param_groups[1]["lr"] == pytest.approx(2e-6)


def test_discard_log_probs_use_risk_adjusted_logits() -> None:
    import math
    import torch
    from rl_train import select_action_log_probs

    outputs = {
        "discard_logits": torch.tensor([[0.0, 0.0] + [-100.0] * 32]),
        "claim_logits": torch.zeros((1, 7)),
        "self_kong_logits": torch.zeros((1, 3)),
        "hu_logits": torch.zeros((1, 2)),
        "value_for_risk": torch.tensor([[-8.0]]),
        "opponent_tenpai_logits": torch.zeros((1, 3)),
        "opponent_risk_logits": torch.tensor([[[5.0, -5.0] + [0.0] * 32] * 3]),
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


def test_recompute_dataset_values_uses_global_critic_value(tmp_path: Path) -> None:
    import torch
    from rl_train import recompute_dataset_values_from_old_policy

    class FakeOldPolicy(torch.nn.Module):
        def forward(
            self,
            tile_planes: torch.Tensor,
            scalar_features: torch.Tensor,
            discard_sequence: torch.Tensor,
            global_tile_planes: torch.Tensor | None = None,
            global_scalar_features: torch.Tensor | None = None,
        ) -> dict[str, torch.Tensor]:
            assert global_tile_planes is not None
            assert global_scalar_features is not None
            assert global_tile_planes.shape == (2, 40, 34)
            assert global_scalar_features.shape == (2, 20)
            return {
                "discard_logits": torch.zeros((2, 34)),
                "claim_logits": torch.zeros((2, 7)),
                "self_kong_logits": torch.zeros((2, 3)),
                "hu_logits": torch.zeros((2, 2)),
                "value": torch.tensor([[0.5], [1.25]]),
                "value_for_risk": torch.tensor([[-8.0], [-8.0]]),
            }

    rows = [
        {
            **base_trajectory_row("learner", 0, reward=0.0, value=0.0, done=False),
            "global_tile_planes": [0.0] * (40 * 34),
            "global_scalar_features": [0.0] * 20,
        },
        {
            **base_trajectory_row("learner", 0, reward=1.0, value=0.0, done=True),
            "decision_index": 1,
            "global_tile_planes": [0.0] * (40 * 34),
            "global_scalar_features": [0.0] * 20,
        },
    ]
    path = tmp_path / "trajectories.jsonl"
    path.write_text(
        "\n".join(json.dumps(row) for row in rows) + "\n",
        encoding="utf-8",
    )
    dataset = ArenaTrajectoryDataset(path, gamma=1.0, gae_lambda=1.0)

    recompute_dataset_values_from_old_policy(
        dataset,
        FakeOldPolicy(),
        torch.device("cpu"),
        gamma=1.0,
        gae_lambda=1.0,
        batch_size=2,
    )

    assert dataset.rows[0]["value"] == pytest.approx(0.5)
    assert dataset.rows[1]["value"] == pytest.approx(1.25)
    assert dataset.advantages == pytest.approx([0.5, -0.25])
    assert dataset.returns == pytest.approx([1.0, 1.0])


def test_discard_log_probs_use_value_for_risk_not_critic_value() -> None:
    import math
    import torch
    from rl_train import select_action_log_probs

    outputs = {
        "discard_logits": torch.tensor([[0.0, 0.0] + [-100.0] * 32]),
        "claim_logits": torch.zeros((1, 7)),
        "self_kong_logits": torch.zeros((1, 3)),
        "hu_logits": torch.zeros((1, 2)),
        "value": torch.tensor([[8.0]]),
        "value_for_risk": torch.tensor([[-8.0]]),
        "opponent_tenpai_logits": torch.zeros((1, 3)),
        "opponent_risk_logits": torch.tensor([[[5.0, -5.0] + [0.0] * 32] * 3]),
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
    policy_config = {
        "base_risk_weight": 0.90,
        "value_risk_range": 0.55,
        "min_risk_weight": 0.25,
        "max_risk_weight": 1.45,
    }

    log_prob = select_action_log_probs(outputs, batch, policy_config)

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
            },
            "rl_candidate_neural": {
                "avg_score_delta": 1.5,
                "win_rate": 0.21,
                "deal_in_rate": 0.11,
                "avg_first_tenpai_turn": 7.8,
                "final_tenpai_rate": 0.55,
                "avg_latency_ms_per_decision": 22.0,
                "avg_claims": 6.0,
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
        },
    ]

    diagnostics = trajectory_diagnostics(rows)

    assert diagnostics["row_count"] == 2
    assert diagnostics["action_head_claim"] == 1
    assert diagnostics["terminal_reward_mean"] == pytest.approx(0.5)
    assert diagnostics["step_reward_abs_sum"] == pytest.approx(0.2)
    assert diagnostics["shanten_improvement_count"] == 1


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


def test_candidate_gate_rejects_high_latency() -> None:
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
            },
            "rl_candidate_neural": {
                "avg_score_delta": 1.5,
                "win_rate": 0.21,
                "deal_in_rate": 0.10,
                "avg_first_tenpai_turn": 7.8,
                "final_tenpai_rate": 0.56,
                "avg_latency_ms_per_decision": 220.0,
                "avg_claims": 2.2,
            },
        }
    }

    result = evaluate_candidate(summary, "baseline_neural", "rl_candidate_neural")

    assert result["accepted"] is False
    assert "latency" in result["failures"]
    assert result["failure_details"] == [
        {
            "metric": "latency",
            "baseline": 20.0,
            "candidate": 220.0,
            "threshold": 200.0,
            "margin": -20.0,
        }
    ]
    assert result["promotion_report"]["latency"]["candidate_avg_ms_per_decision"] == 220.0
    assert result["promotion_report"]["latency"]["limit_ms"] == 200.0


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


def test_candidate_selector_preserves_policy_metadata() -> None:
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
            "policy": "ppo",
            "checkpoint": "ppo/epoch_001.pt",
            "onnx": "ppo/epoch_001.onnx",
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

    assert selected["policy"] == "ppo"
    assert selected["selected"]["policy"] == "ppo"


def test_candidate_selector_preserves_promotion_report_metrics() -> None:
    from candidate_selector import select_best_candidate

    selected = select_best_candidate([
        {
            "epoch": 1,
            "policy": "ppo",
            "checkpoint": "ppo/epoch_001.pt",
            "onnx": "ppo/epoch_001.onnx",
            "gate": {
                "accepted": True,
                "failures": [],
                "baseline": {
                    "avg_score_delta": 0.0,
                    "win_rate": 0.30,
                    "deal_in_rate": 0.12,
                    "avg_first_tenpai_turn": 10.0,
                    "final_tenpai_rate": 0.60,
                    "avg_latency_ms_per_decision": 70.0,
                },
                "candidate": {
                    "avg_score_delta": 1.0,
                    "win_rate": 0.31,
                    "deal_in_rate": 0.11,
                    "avg_first_tenpai_turn": 9.8,
                    "final_tenpai_rate": 0.62,
                    "avg_latency_ms_per_decision": 72.0,
                },
                "paired": {
                    "paired_match_count": 8,
                    "avg_score_delta": 2.5,
                    "confidence95_low": 0.75,
                    "confidence95_high": 4.25,
                    "positive_delta_rate": 0.75,
                },
                "promotion_report": {
                    "paired": {
                        "paired_match_count": 8,
                        "avg_score_delta": 2.5,
                        "confidence95_low": 0.75,
                        "confidence95_high": 4.25,
                        "positive_delta_rate": 0.75,
                    },
                    "warnings": [],
                },
            },
        }
    ])

    summary = selected["selected"]
    assert selected["promotion_report"]["paired"]["avg_score_delta"] == pytest.approx(2.5)
    assert summary["promotion_report"]["paired"]["positive_delta_rate"] == pytest.approx(0.75)
    assert summary["paired_avg_score_delta"] == pytest.approx(2.5)
    assert summary["paired_confidence95_low"] == pytest.approx(0.75)
    assert summary["paired_positive_delta_rate"] == pytest.approx(0.75)


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
                "neural_action_count": 3,
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
    assert summary["policies"]["a"]["neural_action_count"] == 3
    assert summary["policies"]["a"]["latency_sample_count"] == 1
    assert summary["policies"]["a"]["latency_ms_p50"] == pytest.approx(5.0)
    assert summary["policies"]["a"]["latency_ms_p95"] == pytest.approx(5.0)
    assert summary["policies"]["a"]["latency_ms_max"] == pytest.approx(5.0)
    assert "fallback_count" not in summary["policies"]["a"]
    assert "same_as_heuristic_rate" not in summary["policies"]["a"]
    assert summary["policies"]["b"]["deal_in_rate"] == 1.0
    assert summary["policies"]["b"]["avg_first_tenpai_turn"] is None


def test_arena_summary_reports_paired_subject_score_deltas(tmp_path: Path) -> None:
    from arena_summary import load_reports, summarize_reports

    path = tmp_path / "arena.jsonl"
    reports = [
        {
            "match_index": 0,
            "seed": 10,
            "completed": True,
            "action_count": 10,
            "subject_id": "baseline_neural",
            "subject_final_score": 5,
            "seats": [],
        },
        {
            "match_index": 0,
            "seed": 10,
            "completed": True,
            "action_count": 10,
            "subject_id": "rl_candidate_neural",
            "subject_final_score": 8,
            "seats": [],
        },
        {
            "match_index": 1,
            "seed": 11,
            "completed": True,
            "action_count": 10,
            "subject_id": "baseline_neural",
            "subject_final_score": 2,
            "seats": [],
        },
        {
            "match_index": 1,
            "seed": 11,
            "completed": True,
            "action_count": 10,
            "subject_id": "rl_candidate_neural",
            "subject_final_score": 4,
            "seats": [],
        },
    ]
    path.write_text("\n".join(json.dumps(report) for report in reports) + "\n", encoding="utf-8")

    summary = summarize_reports(load_reports(path))

    paired = summary["paired_subjects"]["baseline_neural__vs__rl_candidate_neural"]
    assert paired["paired_match_count"] == 2
    assert paired["avg_score_delta"] == pytest.approx(2.5)
    assert paired["deltas"] == [3, 2]
    assert paired["stddev_score_delta"] == pytest.approx(0.70710678)
    assert paired["stderr_score_delta"] == pytest.approx(0.5)
    assert paired["confidence95_low"] == pytest.approx(1.52)
    assert paired["confidence95_high"] == pytest.approx(3.48)
    assert paired["positive_delta_rate"] == pytest.approx(1.0)
    assert paired["min_score_delta"] == 2
    assert paired["max_score_delta"] == 3


def test_candidate_gate_includes_paired_score_delta() -> None:
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
            },
            "rl_candidate_neural": {
                "avg_score_delta": 1.5,
                "win_rate": 0.21,
                "deal_in_rate": 0.11,
                "avg_first_tenpai_turn": 7.8,
                "final_tenpai_rate": 0.55,
                "avg_latency_ms_per_decision": 22.0,
                "avg_claims": 2.2,
            },
        },
        "paired_subjects": {
            "baseline_neural__vs__rl_candidate_neural": {
                "paired_match_count": 2,
                "avg_score_delta": 2.5,
                "confidence95_low": 1.52,
                "confidence95_high": 3.48,
                "positive_delta_rate": 1.0,
                "deltas": [3, 2],
            }
        },
    }

    result = evaluate_candidate(summary, "baseline_neural", "rl_candidate_neural")

    assert result["paired"]["avg_score_delta"] == pytest.approx(2.5)
    assert result["paired"]["paired_match_count"] == 2
    assert result["failure_details"] == []
    report = result["promotion_report"]
    assert report["metrics"]["avg_score_delta"]["margin"] == pytest.approx(1.5)
    assert report["metrics"]["deal_in_rate"]["margin"] == pytest.approx(0.0)
    assert report["paired"]["confidence95_low"] == pytest.approx(1.52)
    assert report["paired"]["positive_delta_rate"] == pytest.approx(1.0)
    assert report["claim_rate"]["margin"] == pytest.approx(1.8)
    assert report["warnings"] == []
