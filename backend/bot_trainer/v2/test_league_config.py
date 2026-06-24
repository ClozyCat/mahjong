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
    assert "build_counterfactual_teacher.py" in script
    assert "counterfactual_teacher.jsonl" in script
    assert "--ranker-risk-penalty-weight" in script
    assert "bucket_report.py" in script
    assert "train_awr.py" in script
    assert "candidate_gate.py" in script
    assert "--gate-mode" in script
    assert "--promotion-matrix-matches" in script
    assert "promotion_matrix_config" in script
    assert "promotion_result.json" in script
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


def test_policy_improvement_pipeline_runs_large_promotion_matrix_only_after_selection(
    tmp_path: Path,
    monkeypatch,
) -> None:
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
    output_dir = tmp_path / "out"
    calls = []

    monkeypatch.chdir(Path(__file__).resolve().parents[3])
    monkeypatch.setattr(
        pipeline,
        "parse_args",
        lambda: type("Args", (), {
            "iterations": 1,
            "start_iteration": 0,
            "trajectory_matches": 1,
            "trajectory_chunk_matches": 1,
            "matrix_matches": 80,
            "promotion_matrix_matches": 400,
            "seed": "20260624",
            "sft_onnx": "sft.onnx",
            "sft_checkpoint": "sft.pt",
            "pool": str(pool_path),
            "output_dir": str(output_dir),
            "jobs": 1,
            "ranker_epochs": 1,
            "value_epochs": 1,
            "value_lr": 1e-4,
            "ranker_lr": 1e-5,
            "ranker_temperature": 1.5,
            "ranker_top1_weight": 0.1,
            "ranker_risk_penalty_weight": 0.1,
            "teacher_prior_weight": 0.25,
            "teacher_safety_weight": 0.10,
            "teacher_logit_weight": 1.0,
            "teacher_min_count": 5,
            "teacher_max_score_delta": 0.35,
            "awr_epochs": 1,
            "awr_lr": 1e-5,
            "awr_temperature": 1.0,
            "awr_weight_clip": 6.0,
            "awr_kl_coef": 0.08,
        })(),
    )
    monkeypatch.setattr(pipeline, "generate_trajectory_configs", lambda *args, **kwargs: None)
    monkeypatch.setattr(
        pipeline,
        "collect_rollouts",
        lambda _config_dir, iter_dir, _jobs: (iter_dir / "trajectories.jsonl", iter_dir / "cf.jsonl"),
    )
    monkeypatch.setattr(pipeline, "run", lambda _cmd: None)
    monkeypatch.setattr(pipeline, "build_counterfactual_teacher", lambda _cf, _traj, iter_dir, _args: iter_dir / "counterfactual_teacher.jsonl")
    monkeypatch.setattr(pipeline, "train_value_head", lambda _base, _traj, iter_dir, _args: iter_dir / "value.pt")
    monkeypatch.setattr(pipeline, "train_ranker", lambda _base, _cf, iter_dir, _args: iter_dir / "ranker.pt")
    monkeypatch.setattr(pipeline, "run_awr", lambda _ranker, _traj, iter_dir, _args: iter_dir / "awr.pt")
    monkeypatch.setattr(pipeline, "export_candidate", lambda _checkpoint, output: output.write_text("onnx"))
    monkeypatch.setattr(pipeline, "copy_onnx_bundle", lambda _src, dst: dst.parent.mkdir(parents=True, exist_ok=True) or dst.write_text("onnx"))

    def fake_matrix_eval(_pool, _candidate, _iter_dir, _args, _seed, *, matches, label, config_dir_name):
        calls.append((label, matches, config_dir_name))
        return [tmp_path / f"{label}.json"]

    monkeypatch.setattr(pipeline, "matrix_eval", fake_matrix_eval)
    monkeypatch.setattr(pipeline, "run_gate", lambda mode, *_args: mode == "selection")

    pipeline.main()

    assert calls == [
        ("selection", 80, "selection_matrix_config"),
        ("promotion", 400, "promotion_matrix_config"),
    ]


def test_policy_improvement_pipeline_skips_promotion_matrix_when_selection_fails(
    tmp_path: Path,
    monkeypatch,
) -> None:
    pipeline = load_policy_improvement_module()
    pool_path = tmp_path / "pool.json"
    pool_path.write_text(
        """{
          "learner": {"id": "learner", "model_path": "sft.onnx"},
          "rollout_opponents": [
            {"id": "sft", "model_path": "sft.onnx"}
          ]
        }""",
        encoding="utf-8",
    )
    output_dir = tmp_path / "out"
    calls = []

    monkeypatch.chdir(Path(__file__).resolve().parents[3])
    monkeypatch.setattr(
        pipeline,
        "parse_args",
        lambda: type("Args", (), {
            "iterations": 1,
            "start_iteration": 0,
            "trajectory_matches": 1,
            "trajectory_chunk_matches": 1,
            "matrix_matches": 80,
            "promotion_matrix_matches": 400,
            "seed": "20260624",
            "sft_onnx": "sft.onnx",
            "sft_checkpoint": "sft.pt",
            "pool": str(pool_path),
            "output_dir": str(output_dir),
            "jobs": 1,
            "ranker_epochs": 1,
            "value_epochs": 1,
            "value_lr": 1e-4,
            "ranker_lr": 1e-5,
            "ranker_temperature": 1.5,
            "ranker_top1_weight": 0.1,
            "ranker_risk_penalty_weight": 0.1,
            "teacher_prior_weight": 0.25,
            "teacher_safety_weight": 0.10,
            "teacher_logit_weight": 1.0,
            "teacher_min_count": 5,
            "teacher_max_score_delta": 0.35,
            "awr_epochs": 1,
            "awr_lr": 1e-5,
            "awr_temperature": 1.0,
            "awr_weight_clip": 6.0,
            "awr_kl_coef": 0.08,
        })(),
    )
    monkeypatch.setattr(pipeline, "generate_trajectory_configs", lambda *args, **kwargs: None)
    monkeypatch.setattr(
        pipeline,
        "collect_rollouts",
        lambda _config_dir, iter_dir, _jobs: (iter_dir / "trajectories.jsonl", iter_dir / "cf.jsonl"),
    )
    monkeypatch.setattr(pipeline, "run", lambda _cmd: None)
    monkeypatch.setattr(pipeline, "build_counterfactual_teacher", lambda _cf, _traj, iter_dir, _args: iter_dir / "counterfactual_teacher.jsonl")
    monkeypatch.setattr(pipeline, "train_value_head", lambda _base, _traj, iter_dir, _args: iter_dir / "value.pt")
    monkeypatch.setattr(pipeline, "train_ranker", lambda _base, _cf, iter_dir, _args: iter_dir / "ranker.pt")
    monkeypatch.setattr(pipeline, "run_awr", lambda _ranker, _traj, iter_dir, _args: iter_dir / "awr.pt")
    monkeypatch.setattr(pipeline, "export_candidate", lambda _checkpoint, output: output.write_text("onnx"))

    def fake_matrix_eval(_pool, _candidate, _iter_dir, _args, _seed, *, matches, label, config_dir_name):
        calls.append((label, matches, config_dir_name))
        return [tmp_path / f"{label}.json"]

    monkeypatch.setattr(pipeline, "matrix_eval", fake_matrix_eval)
    monkeypatch.setattr(pipeline, "run_gate", lambda _mode, *_args: False)

    pipeline.main()

    assert calls == [("selection", 80, "selection_matrix_config")]
