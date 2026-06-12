from __future__ import annotations

import json
from pathlib import Path


def trajectory_row(global_state: bool = True) -> dict[str, object]:
    row: dict[str, object] = {
        "schema_version": 1,
        "match_id": "m1",
        "decision_index": 0,
        "seat_index": 0,
        "policy_id": "learner",
        "decision_kind": "active_turn",
        "tile_planes": [0.0] * 340,
        "scalar_features": [0.0] * 12,
        "discard_sequence": [0.0] * (32 * 40),
        "discard_mask": [True] + [False] * 33,
        "claim_mask": [True] + [False] * 6,
        "self_kong_mask": [True, False, False],
        "hu_mask": [True, False],
        "action_head": "discard",
        "action_index": 0,
        "action_semantic": "discard:w1",
        "log_prob": -0.3,
        "value": 0.0,
        "reward": 1.0,
        "step_reward": 0.0,
        "terminal_reward": 1.0,
        "shanten_before": None,
        "shanten_after": None,
        "risk_probs": [0.0] * 34,
        "opponent_tenpai_target": [0.0, 0.0, 0.0],
        "opponent_risk_target": [[0.0] * 34 for _ in range(3)],
        "opponent_risk_mask": [[0.0] * 34 for _ in range(3)],
        "done": True,
    }
    if global_state:
        row["global_tile_planes"] = [0.0] * (40 * 34)
        row["global_scalar_features"] = [0.0] * 20
    return row


def write_trajectories(path: Path, rows: list[dict[str, object]]) -> None:
    path.write_text(
        "\n".join(json.dumps(row) for row in rows) + "\n",
        encoding="utf-8",
    )


def test_critic_pretrain_dataset_requires_global_features(tmp_path: Path) -> None:
    import pytest
    from pretrain_critic import build_critic_pretrain_loader

    path = tmp_path / "trajectories.jsonl"
    write_trajectories(path, [trajectory_row(global_state=False)])

    with pytest.raises(ValueError, match="global features"):
        build_critic_pretrain_loader(path, batch_size=1, require_global=True)


def test_pretrain_step_updates_critic_without_updating_actor(tmp_path: Path) -> None:
    import torch
    from model import ModelConfig, build_actor_critic
    from pretrain_critic import build_critic_pretrain_loader, pretrain_critic

    path = tmp_path / "trajectories.jsonl"
    write_trajectories(path, [trajectory_row(global_state=True)])
    loader = build_critic_pretrain_loader(path, batch_size=1, require_global=True)
    model = build_actor_critic(ModelConfig.from_dict({}))
    actor_before = {
        name: value.detach().clone()
        for name, value in model.actor.state_dict().items()
    }

    optimizer = torch.optim.AdamW(model.critic.parameters(), lr=1e-4)
    metrics = pretrain_critic(
        loader,
        model,
        optimizer,
        torch.device("cpu"),
        epochs=1,
    )

    assert metrics[-1]["loss"] >= 0.0
    for name, value in model.actor.state_dict().items():
        assert torch.equal(value, actor_before[name])


def test_pretrain_critic_cli_writes_checkpoint(tmp_path: Path) -> None:
    import subprocess
    import sys
    import torch
    from model import ModelConfig, build_actor_critic

    trajectories = tmp_path / "trajectories.jsonl"
    checkpoint = tmp_path / "actor_critic.pt"
    output = tmp_path / "critic_pretrained.pt"
    write_trajectories(trajectories, [trajectory_row(global_state=True)])
    model = build_actor_critic(ModelConfig.from_dict({}))
    torch.save(
        {
            "model_state": model.state_dict(),
            "model_config": ModelConfig.from_dict({}).to_dict(),
            "training_source": "actor_critic_bootstrap",
        },
        checkpoint,
    )

    result = subprocess.run(
        [
            sys.executable,
            "backend/bot_trainer/v2/pretrain_critic.py",
            "--trajectories",
            str(trajectories),
            "--checkpoint",
            str(checkpoint),
            "--output",
            str(output),
            "--epochs",
            "1",
            "--batch-size",
            "1",
            "--no-tensor-cache",
            "--device",
            "cpu",
        ],
        cwd=Path(__file__).parents[3],
        text=True,
        capture_output=True,
        check=False,
    )

    assert result.returncode == 0, result.stderr
    payload = torch.load(output, map_location="cpu")
    assert payload["training_source"] == "critic_pretrain"
    assert payload["trajectory_source"] == trajectories.as_posix()
    assert len(payload["critic_pretrain_metrics"]) == 1
