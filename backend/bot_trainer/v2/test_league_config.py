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


def test_trajectory_config_splits_matches_and_resamples_opponents() -> None:
    pool = {
        "learner": {
            "id": "learner",
            "model_path": "backend/assets/sft/sft.onnx",
        },
        "rollout_opponents": [
            {"id": "sft_cold", "model_path": "sft.onnx", "weight": 1},
            {"id": "sft_warm", "model_path": "sft.onnx", "weight": 1},
            {"id": "sft_hot", "model_path": "sft.onnx", "weight": 1},
            {"id": "sft_wild", "model_path": "sft.onnx", "weight": 1},
        ],
    }

    configs = build_trajectory_configs(
        pool,
        matches=5,
        seed=7,
        max_actions=20,
        chunk_matches=2,
    )

    assert [config["matches"] for config in configs] == [2, 2, 1]
    assert [config["seed"] for config in configs] == [7, 1007, 2007]
    opponent_sets = [
        tuple(opponent["id"] for opponent in config["opponents"])
        for config in configs
    ]
    assert len(set(opponent_sets)) > 1


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
    script_path = Path(__file__).resolve().parent.parent / "train_awr_pipeline.py"
    script = script_path.read_text(encoding="utf-8")

    assert '"backend/bot_trainer/v2/checkpoints/best.pt"' in script
    assert "--rollout-onnx" in script
    assert "cargo" in script and "bot_arena" in script
    assert "trajectory_config_" in script
    assert "arena_summary.py" in script
    assert "matrix_summaries" in script
    assert "--temperature" in script and "args.awr_temperature" in script
    assert "--weight-clip" in script and "args.awr_weight_clip" in script
    assert "--awr-epochs" in script
    assert "--awr-lr" in script and "args.awr_lr" in script
    assert "--awr-kl-coef" in script and "args.awr_kl_coef" in script
    assert "--awr-value-finetune-epochs" in script
    assert "args.awr_value_finetune_epochs" in script
    assert "--adv-norm" in script and "batch" in script
    assert "--value-loss-coef" in script and "0.0" in script
    assert "--kl-coef" in script
    assert "--sft-checkpoint" in script
    assert '"--sft-checkpoint", args.sft_checkpoint' in script
    assert '"--risk-value-checkpoint", args.sft_checkpoint' in script
    assert "--risk-value-checkpoint" in script
    assert "--matrix-matches" in script and "default=80" in script
    assert "--policy-id" in script and "learner" in script
