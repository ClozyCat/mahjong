import importlib.util
from pathlib import Path

from league_config import build_eval_config, build_matrix_configs, build_trajectory_configs


def load_policy_improvement_module():
    script_path = Path(__file__).resolve().parent.parent / "train_policy_improvement_pipeline.py"
    spec = importlib.util.spec_from_file_location("train_policy_improvement_pipeline", script_path)
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


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


def test_trajectory_config_balances_three_rollout_opponents_per_chunk() -> None:
    pool = {
        "learner": {
            "id": "learner",
            "model_path": "backend/assets/sft/sft.onnx",
        },
        "rollout_opponents": [
            {"id": "sft_cold", "model_path": "sft.onnx", "weight": 1},
            {"id": "sft_warm", "model_path": "sft.onnx", "weight": 1},
            {"id": "sft_hot", "model_path": "sft.onnx", "weight": 1},
        ],
    }

    configs = build_trajectory_configs(
        pool,
        matches=6,
        seed=11,
        max_actions=20,
        chunk_matches=2,
    )

    for config in configs:
        assert sorted(opponent["id"] for opponent in config["opponents"]) == [
            "sft_cold",
            "sft_hot",
            "sft_warm",
        ]


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


def test_policy_improvement_pipeline_uses_counterfactual_ranker_and_gates() -> None:
    script_path = Path(__file__).resolve().parent.parent / "train_policy_improvement_pipeline.py"
    script = script_path.read_text(encoding="utf-8")

    assert "--counterfactual-discards" in script
    assert "counterfactual_discards.jsonl" in script
    assert "train_value.py" in script
    assert "value_pretrained.pt" in script
    assert "--start-iteration" in script
    assert "train_discard_ranker.py" in script
    assert "ranker_best.pt" in script
    assert "bucket_report.py" in script
    assert "train_awr.py" in script
    assert "candidate_gate.py" in script
    assert "--gate-mode" in script
    assert "selection" in script and "promotion" in script


def test_policy_improvement_pipeline_updates_pool_after_promotion(tmp_path: Path) -> None:
    pipeline = load_policy_improvement_module()
    pool_path = tmp_path / "pool.json"
    pool_path.write_text(
        """{
          "learner": {"id": "learner", "model_path": "sft.onnx"},
          "rollout_opponents": [
            {"id": "sft", "model_path": "sft.onnx", "sample_actions": true, "temperature": 1.0}
          ]
        }""",
        encoding="utf-8",
    )

    pipeline.update_pool_after_promotion(pool_path, "league/iter_0/policy.onnx", 0)

    updated = pipeline.load_json(pool_path)
    assert updated["learner"]["model_path"] == "league/iter_0/policy.onnx"
    assert any(opponent["id"] == "promoted_iter_0" for opponent in updated["rollout_opponents"])


def test_policy_improvement_pipeline_adds_selected_candidate_without_replacing_learner(tmp_path: Path) -> None:
    pipeline = load_policy_improvement_module()
    pool_path = tmp_path / "pool.json"
    pool_path.write_text(
        """{
          "learner": {"id": "learner", "model_path": "sft.onnx"},
          "rollout_opponents": [
            {"id": "sft_cold", "model_path": "sft.onnx", "weight": 1}
          ]
        }""",
        encoding="utf-8",
    )

    pipeline.update_pool_after_selection(pool_path, "candidate.onnx", 2)

    updated = pipeline.load_json(pool_path)
    assert updated["learner"]["model_path"] == "sft.onnx"
    selected = [
        opponent
        for opponent in updated["rollout_opponents"]
        if opponent["id"] == "selected_iter_2"
    ]
    assert selected == [{
        "id": "selected_iter_2",
        "model_path": "candidate.onnx",
        "sample_actions": True,
        "temperature": 1.0,
        "weight": 1,
    }]
