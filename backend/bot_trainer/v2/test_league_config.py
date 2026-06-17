from pathlib import Path

from league_config import build_eval_config, build_matrix_configs, build_trajectory_configs


def test_trajectory_config_uses_learner_for_subject_and_three_sampled_opponents() -> None:
    pool = {
        "learner": {
            "id": "learner",
            "model_path": "backend/assets/sft/sft.onnx",
            "sample_actions": False,
            "temperature": 0.5,
        },
        "opponents": [],
    }

    configs = build_trajectory_configs(pool, matches=2, seed=7, max_actions=20)
    assert len(configs) == 1
    config = configs[0]

    assert config["matches"] == 2
    assert len(config["subjects"]) == 1
    assert config["subjects"][0]["id"] == "learner"
    assert config["subjects"][0]["display_name"] == "Learner"
    assert config["subjects"][0]["sample_actions"] is True
    assert len(config["opponents"]) == 3
    assert [opponent["id"] for opponent in config["opponents"]] == [
        "learner",
        "learner",
        "learner",
    ]
    assert all(opponent["sample_actions"] is True for opponent in config["opponents"])
    assert all("display_name" not in opponent for opponent in config["opponents"])


def test_eval_config_supplies_three_baseline_opponents() -> None:
    pool = {"learner": {"id": "learner", "model_path": "unused.onnx"}, "opponents": []}

    config = build_eval_config(
        pool,
        candidate_onnx=Path("candidate.onnx"),
        baseline_onnx=Path("baseline.onnx"),
        matches=2,
        seed=9,
        max_actions=30,
    )

    assert [subject["id"] for subject in config["subjects"]] == [
        "baseline_neural",
        "awr_candidate_neural",
    ]
    assert len(config["opponents"]) == 3
    assert [opponent["id"] for opponent in config["opponents"]] == [
        "baseline-opponent-1",
        "baseline-opponent-2",
        "baseline-opponent-3",
    ]
    assert all(opponent["model_path"] == "baseline.onnx" for opponent in config["opponents"])
    assert all(opponent["sample_actions"] is False for opponent in config["opponents"])


def test_matrix_config_compares_baseline_and_candidate_against_three_opponents() -> None:
    pool = {
        "rollout_opponents": [
            {
                "id": "sft_warm",
                "model_path": "sft.onnx",
                "sample_actions": True,
                "temperature": 1.0,
                "weight": 2,
            }
        ],
    }

    config = build_matrix_configs(
        pool,
        candidate_onnx=Path("candidate.onnx"),
        baseline_onnx=Path("baseline.onnx"),
        matches=4,
        seed=11,
        max_actions=50,
    )[0]

    assert [subject["id"] for subject in config["subjects"]] == [
        "baseline_neural",
        "awr_candidate_neural",
    ]
    assert [subject["model_path"] for subject in config["subjects"]] == [
        "baseline.onnx",
        "candidate.onnx",
    ]
    assert len(config["opponents"]) == 3
    assert [opponent["id"] for opponent in config["opponents"]] == [
        "sft_warm-opponent-1",
        "sft_warm-opponent-2",
        "sft_warm-opponent-3",
    ]
    assert all(opponent["model_path"] == "sft.onnx" for opponent in config["opponents"])
    assert all("weight" not in opponent for opponent in config["opponents"])


def test_awr_wrapper_uses_v2_sft_checkpoint_and_rollout_override() -> None:
    script = Path("backend/bot_trainer/train_awr_model.ps1").read_text(encoding="utf-8")

    assert '[string]$SftCheckpoint = "backend/bot_trainer/v2/checkpoints/best.pt"' in script
    assert "--rollout-onnx $SftOnnx" in script
    assert "cargo run --release --manifest-path backend/Cargo.toml --bin bot_arena" in script
    assert "trajectory_config_*.json" in script
    assert "--output \"$chunkSummary\"" in script
    assert "arena_summary.py" in script
    assert "--summary $matrixSummaries" in script
    assert "--temperature 0.5" in script
    assert "--sft-checkpoint $SftCheckpoint" in script
    assert "--policy-id learner" in script
