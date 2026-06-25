from __future__ import annotations

import json
from pathlib import Path

import pytest

from dataset import (
    DISCARD_EVENT_FEATURE_COUNT,
    DISCARD_SEQUENCE_LENGTH,
    IGNORE_INDEX,
    SCALAR_FEATURE_COUNT,
    encode_row,
    load_metadata,
)


def test_encode_row_without_torch_dependency(tmp_path: Path) -> None:
    metadata_path, train_path = write_fixture(tmp_path)
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    row = json.loads(train_path.read_text(encoding="utf-8").splitlines()[0])

    encoded = encode_row(row, metadata)

    assert encoded["tile_planes"].shape == (10, 34)
    assert encoded["scalar_features"].shape == (SCALAR_FEATURE_COUNT,)
    assert encoded["discard_sequence"].shape == (
        DISCARD_SEQUENCE_LENGTH,
        DISCARD_EVENT_FEATURE_COUNT,
    )
    assert encoded["discard_mask"].shape == (34,)
    assert encoded["discard_target"].item() == 0
    assert encoded["sample_weight"].shape == (1,)
    assert encoded["sample_weight"][0].item() == 1.0


def test_encode_row_reads_sample_weight(tmp_path: Path) -> None:
    metadata_path, train_path = write_fixture(tmp_path)
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    row = json.loads(train_path.read_text(encoding="utf-8").splitlines()[0])
    row["sample_weight"] = 0.35

    encoded = encode_row(row, metadata)

    assert encoded["sample_weight"][0].item() == pytest.approx(0.35)


def test_schema_v7_encodes_opponent_targets_fan_targets_and_sample_weight(tmp_path: Path) -> None:
    metadata_path, train_path = write_fixture(tmp_path)
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    row = json.loads(train_path.read_text(encoding="utf-8").splitlines()[0])
    row["outcome"]["fan_count"] = 8
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

    encoded = encode_row(row, metadata)

    assert encoded["opponent_tenpai_target"].tolist() == [1.0, 0.0, 1.0]
    assert encoded["opponent_risk_target"].shape == (3, 34)
    assert encoded["opponent_risk_mask"].shape == (3, 34)
    assert encoded["opponent_risk_target"][2, 1].item() == 1.0
    assert not encoded["opponent_risk_mask"][1].any().item()
    assert encoded["fan_target"].shape == (1,)
    assert encoded["fan_target"][0].item() == 0.5
    assert encoded["qualifying_fan_target"].shape == (1,)
    assert encoded["qualifying_fan_target"][0].item() == 1.0
    assert encoded["sample_weight"][0].item() == 1.0


def test_qualifying_fan_target_preserves_sub_eight_fan_gradient(tmp_path: Path) -> None:
    metadata_path, train_path = write_fixture(tmp_path)
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    row = json.loads(train_path.read_text(encoding="utf-8").splitlines()[0])
    row["outcome"]["fan_count"] = 4

    encoded = encode_row(row, metadata)

    assert encoded["fan_target"][0].item() == 0.25
    assert encoded["qualifying_fan_target"][0].item() == 0.5


def test_load_metadata_rejects_old_schema_with_export_hint(tmp_path: Path) -> None:
    metadata_path, _ = write_fixture(tmp_path)
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    metadata["schema_version"] = 2
    metadata_path.write_text(json.dumps(metadata), encoding="utf-8")

    with pytest.raises(ValueError, match="Re-export the dataset"):
        load_metadata(metadata_path)


def test_sft_pipeline_forwards_auxiliary_training_flags() -> None:
    script_dir = Path(__file__).resolve().parents[1]
    pipeline = (script_dir / "run_sft_pipeline.py").read_text(encoding="utf-8")

    required_flags = [
        "--fan-loss-weight",
        "--qualifying-fan-loss-weight",
        "--risk-pos-weight",
        "--value-loss-start-weight",
        "--fan-loss-start-weight",
        "--qualifying-fan-loss-start-weight",
        "--risk-loss-start-weight",
        "--aux-loss-warmup-epochs",
        "--claim-rare-action-weight",
        "--self-kong-rare-action-weight",
        "--hu-positive-weight",
    ]

    for flag in required_flags:
        assert flag in pipeline
    assert 'parser.add_argument("--lr", type=float, default=0.0003)' in pipeline
    assert 'parser.add_argument("--lr-min", type=float, default=0.00001)' in pipeline
    assert 'parser.add_argument("--amp", dest="amp", action="store_true", default=None)' in pipeline
    assert 'parser.add_argument("--no-amp", dest="amp", action="store_false")' in pipeline
    assert 'parser.add_argument("--no-tf32", action="store_true")' in pipeline
    assert 'parser.add_argument("--value-loss-weight", type=float, default=0.75)' in pipeline
    assert 'parser.add_argument("--risk-loss-weight", type=float, default=1.0)' in pipeline
    assert "Existing dataset found" in pipeline
    assert "skip_export_dataset" in pipeline
    assert '"--bin",' in pipeline
    assert '"export_bot_dataset_v2"' in pipeline
    assert '"export_bot_dataset_v2_datasets2"' in pipeline
    assert "Transformer encoder" not in pipeline
    assert "MAHJONG_BOT_MODEL_PATH" in pipeline
    assert "bot::neural::tests::runs_local_onnx_model_when_available" in pipeline


def test_sft_pipeline_supports_datasets2_export_source() -> None:
    script_dir = Path(__file__).resolve().parents[1]
    pipeline = (script_dir / "run_sft_pipeline.py").read_text(encoding="utf-8")

    assert 'parser.add_argument("--input-format", choices=("botzone", "datasets2", "both"), default="botzone")' in pipeline
    assert 'DEFAULT_DATASETS2_INPUT_PATH = "backend/bot_trainer/datasets2"' in pipeline
    assert 'parser.add_argument("--datasets2-input", default=DEFAULT_DATASETS2_INPUT_PATH)' in pipeline
    assert 'parser.add_argument("--export-workers", type=int, default=0)' in pipeline
    assert '"export_bot_dataset_v2_datasets2"' in pipeline
    assert '"--workers"' in pipeline
    assert 'if args.input_format == "datasets2" and args.input == DEFAULT_INPUT_PATH:' in pipeline
    assert 'if args.input_format == "both":' in pipeline
    assert "merge_exported_datasets" in pipeline


def test_sft_training_defaults_to_bf16_amp_without_grad_scaling(monkeypatch: pytest.MonkeyPatch) -> None:
    import sys

    torch = pytest.importorskip("torch")
    import train

    monkeypatch.setattr(
        sys,
        "argv",
        [
            "train.py",
            "--data",
            "backend/bot_trainer/v2/sft/out",
            "--output",
            "backend/bot_trainer/v2/sft/checkpoints",
        ],
    )

    args = train.parse_args()

    assert args.amp is True

    monkeypatch.setattr(torch.cuda, "is_bf16_supported", lambda: True, raising=False)
    amp_config = train.resolve_amp_config(torch.device("cuda"), args.amp)

    assert amp_config.enabled is True
    assert amp_config.dtype == torch.bfloat16
    assert amp_config.scaler_enabled is False

    monkeypatch.setattr(
        sys,
        "argv",
        [
            "train.py",
            "--data",
            "backend/bot_trainer/v2/sft/out",
            "--output",
            "backend/bot_trainer/v2/sft/checkpoints",
            "--no-amp",
        ],
    )

    assert train.parse_args().amp is False



def test_discard_sequence_encodes_order_source_and_latest_marker(tmp_path: Path) -> None:
    metadata_path, train_path = write_fixture(tmp_path)
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    row = json.loads(train_path.read_text(encoding="utf-8").splitlines()[0])
    row["context"]["seat_index"] = 0
    row["context"]["discard_history"] = [
        {"seat_index": 1, "tile_key": "w3"},
        {"seat_index": 2, "tile_key": "t5"},
    ]

    encoded = encode_row(row, metadata)
    sequence = encoded["discard_sequence"]
    previous = sequence[-2]
    latest = sequence[-1]

    assert previous[2].item() == 1.0
    assert previous[34:38].tolist() == [0.0, 1.0, 0.0, 0.0]
    assert previous[39].item() == 0.0
    assert latest[13].item() == 1.0
    assert latest[34:38].tolist() == [0.0, 0.0, 1.0, 0.0]
    assert latest[39].item() == 1.0


def test_scalar_features_use_standard_seat_wind_for_runtime_context(tmp_path: Path) -> None:
    metadata_path, train_path = write_fixture(tmp_path)
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    row = json.loads(train_path.read_text(encoding="utf-8").splitlines()[0])
    row["context"]["seat_index"] = 1
    row["context"]["dealer_seat"] = 0
    row["context"]["round_wind"] = "south"

    encoded = encode_row(row, metadata)

    assert abs(encoded["scalar_features"][10].item() - (1.0 / 3.0)) < 1e-6
    assert encoded["scalar_features"][11].item() == 1.0


def test_scalar_features_use_standard_north_wind_index(tmp_path: Path) -> None:
    metadata_path, train_path = write_fixture(tmp_path)
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    row = json.loads(train_path.read_text(encoding="utf-8").splitlines()[0])
    row["context"]["seat_index"] = 3
    row["context"]["dealer_seat"] = 0
    row["context"]["seat_wind"] = "north"
    row["context"]["round_wind"] = "north"

    encoded = encode_row(row, metadata)

    assert encoded["scalar_features"][10].item() == 1.0
    assert encoded["scalar_features"][11].item() == 1.0


def test_dataset_reads_batches_from_disk_cache(tmp_path: Path) -> None:
    import pytest

    torch = pytest.importorskip("torch")
    from dataset import MahjongDecisionDataset
    from train import build_loader

    metadata_path, train_path = write_fixture(tmp_path)
    dataset = MahjongDecisionDataset(train_path, metadata_path, cache_dir=tmp_path / "cache")

    batch = dataset.get_batch([0])
    loader_batch = next(iter(build_loader(dataset, 1, False, 0, torch.device("cpu"))))

    assert len(dataset) == 1
    assert (tmp_path / "cache" / "train" / "tile_planes.npy").exists()
    assert batch["tile_planes"].shape == (1, 10, 34)
    assert batch["scalar_features"].shape == (1, SCALAR_FEATURE_COUNT)
    assert batch["discard_sequence"].shape == (
        1,
        DISCARD_SEQUENCE_LENGTH,
        DISCARD_EVENT_FEATURE_COUNT,
    )
    assert batch["discard_target"].tolist() == [0]
    assert batch["discard_mask"].dtype == torch.bool
    assert loader_batch["discard_target"].tolist() == [0]


def test_chow_claim_target_uses_discard_position(tmp_path: Path) -> None:
    metadata_path, train_path = write_fixture(tmp_path)
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    base_row = json.loads(train_path.read_text(encoding="utf-8").splitlines()[0])
    expected_by_discard = {"w3": 4, "w4": 5, "w5": 6}

    for last_discard, expected_target in expected_by_discard.items():
        row = claim_row(base_row, last_discard, "w4")
        encoded = encode_row(row, metadata)
        assert encoded["claim_target"].item() == expected_target
        assert encoded["claim_mask"][expected_target].item()


def test_self_kong_pass_trains_self_kong_head_only(tmp_path: Path) -> None:
    metadata_path, train_path = write_fixture(tmp_path)
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    row = json.loads(train_path.read_text(encoding="utf-8").splitlines()[0])
    row["decision_kind"] = "active_turn"
    row["context"]["self_kong_candidates"] = [
        {
            "kind": "concealed_kong",
            "tile_ids": ["w1#0", "w1#1", "w1#2", "w1#3"],
            "tile_key": "w1",
            "meld_index": None,
        }
    ]
    row["legal_actions"] = ["pass", "self_kong:concealed_kong:w1"]
    row["label"] = {"type": "pass"}

    encoded = encode_row(row, metadata)

    assert encoded["claim_target"].item() == IGNORE_INDEX
    assert encoded["self_kong_target"].item() == 0
    assert encoded["self_kong_mask"][0].item()
    assert encoded["self_kong_mask"][1].item()


def test_auxiliary_loss_weights_can_disable_value_and_risk() -> None:
    import torch
    from train import compute_losses

    outputs = {
        "discard_logits": torch.tensor([[3.0, 0.0] + [-100.0] * 32]),
        "claim_logits": torch.zeros((1, 7)),
        "self_kong_logits": torch.zeros((1, 3)),
        "hu_logits": torch.zeros((1, 2)),
        "value": torch.tensor([[999.0]]),
        "fan_value": torch.tensor([[999.0]]),
        "qualifying_fan_value": torch.tensor([[999.0]]),
        "opponent_tenpai_logits": torch.zeros((1, 3)),
        "opponent_risk_logits": torch.full((1, 3, 34), 999.0),
    }
    batch = {
        "discard_mask": torch.tensor([[True, True] + [False] * 32]),
        "discard_target": torch.tensor([0]),
        "claim_mask": torch.zeros((1, 7), dtype=torch.bool),
        "claim_target": torch.tensor([-100]),
        "self_kong_mask": torch.zeros((1, 3), dtype=torch.bool),
        "self_kong_target": torch.tensor([-100]),
        "hu_mask": torch.tensor([[True, False]]),
        "hu_target": torch.tensor([-100]),
        "value_target": torch.tensor([[0.0]]),
        "fan_target": torch.tensor([[0.0]]),
        "qualifying_fan_target": torch.tensor([[0.0]]),
        "opponent_tenpai_target": torch.zeros((1, 3)),
        "opponent_risk_target": torch.zeros((1, 3, 34)),
        "opponent_risk_mask": torch.zeros((1, 3, 34), dtype=torch.bool),
        "sample_weight": torch.ones((1, 1)),
    }

    losses = compute_losses(
        outputs,
        batch,
        value_weight=0.0,
        fan_weight=0.0,
        qualifying_fan_weight=0.0,
        risk_weight=0.0,
        hu_weight=1.0,
    )

    # value_loss 被裁剪到 max=100.0 以防止数值爆炸
    assert losses["value_loss"].item() == 100.0
    assert losses["fan_loss"].item() == 100.0
    # risk_loss现在是opponent modeling loss，可能为0（无opponent targets）
    assert losses["risk_loss"].item() >= 0.0
    assert losses["loss"].item() < 0.1


def test_sample_weight_scales_selection_loss() -> None:
    import torch
    from train import compute_losses

    outputs = {
        "discard_logits": torch.tensor([[0.0, 0.0] + [-100.0] * 32]),
        "claim_logits": torch.zeros((1, 7)),
        "self_kong_logits": torch.zeros((1, 3)),
        "hu_logits": torch.zeros((1, 2)),
        "value": torch.zeros((1, 1)),
        "fan_value": torch.zeros((1, 1)),
        "qualifying_fan_value": torch.zeros((1, 1)),
        "opponent_tenpai_logits": torch.zeros((1, 3)),
        "opponent_risk_logits": torch.zeros((1, 3, 34)),
    }
    batch = {
        "discard_mask": torch.tensor([[True, True] + [False] * 32]),
        "discard_target": torch.tensor([0]),
        "claim_mask": torch.zeros((1, 7), dtype=torch.bool),
        "claim_target": torch.tensor([-100]),
        "self_kong_mask": torch.zeros((1, 3), dtype=torch.bool),
        "self_kong_target": torch.tensor([-100]),
        "hu_mask": torch.zeros((1, 2), dtype=torch.bool),
        "hu_target": torch.tensor([-100]),
        "value_target": torch.zeros((1, 1)),
        "fan_target": torch.zeros((1, 1)),
        "qualifying_fan_target": torch.zeros((1, 1)),
        "opponent_tenpai_target": torch.zeros((1, 3)),
        "opponent_risk_target": torch.zeros((1, 3, 34)),
        "opponent_risk_mask": torch.zeros((1, 3, 34), dtype=torch.bool),
        "sample_weight": torch.tensor([[0.25]]),
    }

    losses = compute_losses(
        outputs,
        batch,
        value_weight=0.0,
        fan_weight=0.0,
        qualifying_fan_weight=0.0,
        risk_weight=0.0,
    )

    assert losses["discard_loss"].item() == pytest.approx(0.25 * 0.693147, rel=1e-5)


def test_gradients_are_finite_rejects_nan_and_inf() -> None:
    import torch
    from train import gradients_are_finite

    parameter = torch.nn.Parameter(torch.tensor([1.0]))
    parameter.grad = torch.tensor([0.5])
    assert gradients_are_finite([parameter])

    parameter.grad = torch.tensor([float("inf")])
    assert not gradients_are_finite([parameter])

    parameter.grad = torch.tensor([float("nan")])
    assert not gradients_are_finite([parameter])


def test_configure_cuda_math_enables_tf32_for_cuda_device() -> None:
    import torch
    from train import configure_cuda_math

    previous_matmul = torch.backends.cuda.matmul.allow_tf32
    previous_cudnn = torch.backends.cudnn.allow_tf32
    previous_precision = torch.get_float32_matmul_precision()
    try:
        mode = configure_cuda_math(torch.device("cuda"), allow_tf32=True)

        assert mode == "tf32"
        assert torch.backends.cuda.matmul.allow_tf32
        assert torch.backends.cudnn.allow_tf32
        assert torch.get_float32_matmul_precision() == "high"
    finally:
        torch.backends.cuda.matmul.allow_tf32 = previous_matmul
        torch.backends.cudnn.allow_tf32 = previous_cudnn
        torch.set_float32_matmul_precision(previous_precision)


def test_risk_loss_ignores_unmasked_tiles() -> None:
    import torch
    from train import compute_losses

    outputs = {
        "discard_logits": torch.zeros((1, 34)),
        "claim_logits": torch.zeros((1, 7)),
        "self_kong_logits": torch.zeros((1, 3)),
        "hu_logits": torch.zeros((1, 2)),
        "value": torch.zeros((1, 1)),
        "fan_value": torch.zeros((1, 1)),
        "qualifying_fan_value": torch.zeros((1, 1)),
        "opponent_tenpai_logits": torch.zeros((1, 3)),
        "opponent_risk_logits": torch.full((1, 3, 34), -10.0),
    }
    batch = {
        "discard_mask": torch.zeros((1, 34), dtype=torch.bool),
        "discard_target": torch.tensor([-100]),
        "claim_mask": torch.zeros((1, 7), dtype=torch.bool),
        "claim_target": torch.tensor([-100]),
        "self_kong_mask": torch.zeros((1, 3), dtype=torch.bool),
        "self_kong_target": torch.tensor([-100]),
        "hu_mask": torch.zeros((1, 2), dtype=torch.bool),
        "hu_target": torch.tensor([-100]),
        "value_target": torch.zeros((1, 1)),
        "fan_target": torch.zeros((1, 1)),
        "qualifying_fan_target": torch.zeros((1, 1)),
        "opponent_tenpai_target": torch.zeros((1, 3)),
        "opponent_risk_target": torch.zeros((1, 3, 34)),
        "opponent_risk_mask": torch.ones((1, 3, 34), dtype=torch.bool),
    }

    losses = compute_losses(
        outputs,
        batch,
        value_weight=0.0,
        fan_weight=0.0,
        qualifying_fan_weight=0.0,
        risk_weight=1.0,
    )

    expected_tenpai_loss = torch.nn.functional.binary_cross_entropy_with_logits(
        outputs["opponent_tenpai_logits"],
        batch["opponent_tenpai_target"],
    )
    assert losses["risk_loss"].item() == pytest.approx(expected_tenpai_loss.item())


def test_auxiliary_losses_use_float32_for_half_precision_outputs() -> None:
    import torch
    from train import compute_losses

    outputs = {
        "discard_logits": torch.zeros((1, 34), dtype=torch.float16),
        "claim_logits": torch.zeros((1, 7), dtype=torch.float16),
        "self_kong_logits": torch.zeros((1, 3), dtype=torch.float16),
        "hu_logits": torch.zeros((1, 2), dtype=torch.float16),
        "value": torch.tensor([[0.25]], dtype=torch.float16),
        "fan_value": torch.tensor([[0.25]], dtype=torch.float16),
        "qualifying_fan_value": torch.tensor([[0.25]], dtype=torch.float16),
        "opponent_tenpai_logits": torch.zeros((1, 3)),
        "opponent_risk_logits": torch.zeros((1, 3, 34), dtype=torch.float16),
    }
    batch = {
        "discard_mask": torch.zeros((1, 34), dtype=torch.bool),
        "discard_target": torch.tensor([-100]),
        "claim_mask": torch.zeros((1, 7), dtype=torch.bool),
        "claim_target": torch.tensor([-100]),
        "self_kong_mask": torch.zeros((1, 3), dtype=torch.bool),
        "self_kong_target": torch.tensor([-100]),
        "hu_mask": torch.zeros((1, 2), dtype=torch.bool),
        "hu_target": torch.tensor([-100]),
        "value_target": torch.ones((1, 1)),
        "fan_target": torch.ones((1, 1)),
        "qualifying_fan_target": torch.ones((1, 1)),
        "opponent_tenpai_target": torch.ones((1, 3)),
        "opponent_risk_target": torch.ones((1, 3, 34)),
        "opponent_risk_mask": torch.ones((1, 3, 34), dtype=torch.bool),
    }

    losses = compute_losses(outputs, batch)

    assert losses["discard_loss"].dtype == torch.float32
    assert losses["claim_loss"].dtype == torch.float32
    assert losses["self_kong_loss"].dtype == torch.float32
    assert losses["hu_loss"].dtype == torch.float32
    assert losses["value_loss"].dtype == torch.float32
    assert losses["fan_loss"].dtype == torch.float32
    assert losses["qualifying_fan_loss"].dtype == torch.float32
    assert losses["risk_loss"].dtype == torch.float32


def test_opponent_targets_supervise_opponent_outputs() -> None:
    import torch
    from train import compute_losses

    outputs = {
        "discard_logits": torch.zeros((1, 34)),
        "claim_logits": torch.zeros((1, 7)),
        "self_kong_logits": torch.zeros((1, 3)),
        "hu_logits": torch.zeros((1, 2)),
        "value": torch.zeros((1, 1)),
        "fan_value": torch.zeros((1, 1)),
        "qualifying_fan_value": torch.zeros((1, 1)),
        "opponent_tenpai_logits": torch.zeros((1, 3)),
        "opponent_risk_logits": torch.zeros((1, 3, 34)),
    }
    batch = {
        "discard_mask": torch.zeros((1, 34), dtype=torch.bool),
        "discard_target": torch.tensor([-100]),
        "claim_mask": torch.zeros((1, 7), dtype=torch.bool),
        "claim_target": torch.tensor([-100]),
        "self_kong_mask": torch.zeros((1, 3), dtype=torch.bool),
        "self_kong_target": torch.tensor([-100]),
        "hu_mask": torch.zeros((1, 2), dtype=torch.bool),
        "hu_target": torch.tensor([-100]),
        "value_target": torch.zeros((1, 1)),
        "fan_target": torch.zeros((1, 1)),
        "qualifying_fan_target": torch.zeros((1, 1)),
        "opponent_tenpai_target": torch.tensor([[1.0, 0.0, 0.0]]),
        "opponent_risk_target": torch.tensor(
            [[[1.0, 0.0] + [0.0] * 32, [0.0] * 34, [0.0] * 34]]
        ),
        "opponent_risk_mask": torch.tensor(
            [[[True, True] + [False] * 32, [False] * 34, [False] * 34]]
        ),
    }

    losses = compute_losses(
        outputs,
        batch,
        value_weight=0.0,
        fan_weight=0.0,
        qualifying_fan_weight=0.0,
        risk_weight=1.0,
    )

    assert losses["risk_loss"].item() > 0.0
    assert losses["loss"].item() == losses["risk_loss"].item()


def test_losses_sanitize_nonfinite_model_outputs() -> None:
    import torch
    from train import compute_losses

    outputs = {
        "discard_logits": torch.tensor([[float("nan"), float("inf")] + [0.0] * 32]),
        "claim_logits": torch.tensor([[0.0, float("nan")] + [0.0] * 5]),
        "self_kong_logits": torch.tensor([[0.0, float("-inf"), 1.0]]),
        "hu_logits": torch.tensor([[float("inf"), 0.0]]),
        "value": torch.tensor([[float("inf")]]),
        "fan_value": torch.tensor([[float("nan")]]),
        "qualifying_fan_value": torch.tensor([[float("-inf")]]),
        "opponent_tenpai_logits": torch.zeros((1, 3)),
        "opponent_risk_logits": torch.zeros((1, 3, 34)),
    }
    batch = {
        "discard_mask": torch.tensor([[True, True] + [False] * 32]),
        "discard_target": torch.tensor([0]),
        "claim_mask": torch.tensor([[True, True] + [False] * 5]),
        "claim_target": torch.tensor([1]),
        "self_kong_mask": torch.tensor([[True, True, True]]),
        "self_kong_target": torch.tensor([2]),
        "hu_mask": torch.tensor([[True, True]]),
        "hu_target": torch.tensor([1]),
        "value_target": torch.zeros((1, 1)),
        "fan_target": torch.zeros((1, 1)),
        "qualifying_fan_target": torch.zeros((1, 1)),
        "opponent_tenpai_target": torch.zeros((1, 3)),
        "opponent_risk_target": torch.zeros((1, 3, 34)),
        "opponent_risk_mask": torch.ones((1, 3, 34), dtype=torch.bool),
    }

    losses = compute_losses(outputs, batch)

    assert all(torch.isfinite(loss) for loss in losses.values())


def test_rare_hu_positive_weight_increases_hu_loss() -> None:
    import torch
    from train import compute_losses

    outputs = {
        "discard_logits": torch.zeros((1, 34)),
        "claim_logits": torch.zeros((1, 7)),
        "self_kong_logits": torch.zeros((1, 3)),
        "hu_logits": torch.zeros((1, 2)),
        "value": torch.zeros((1, 1)),
        "fan_value": torch.zeros((1, 1)),
        "qualifying_fan_value": torch.zeros((1, 1)),
        "opponent_tenpai_logits": torch.zeros((1, 3)),
        "opponent_risk_logits": torch.zeros((1, 3, 34)),
    }
    batch = {
        "discard_mask": torch.zeros((1, 34), dtype=torch.bool),
        "discard_target": torch.tensor([-100]),
        "claim_mask": torch.zeros((1, 7), dtype=torch.bool),
        "claim_target": torch.tensor([-100]),
        "self_kong_mask": torch.zeros((1, 3), dtype=torch.bool),
        "self_kong_target": torch.tensor([-100]),
        "hu_mask": torch.tensor([[True, True]]),
        "hu_target": torch.tensor([1]),
        "value_target": torch.zeros((1, 1)),
        "fan_target": torch.zeros((1, 1)),
        "qualifying_fan_target": torch.zeros((1, 1)),
        "opponent_tenpai_target": torch.zeros((1, 3)),
        "opponent_risk_target": torch.zeros((1, 3, 34)),
        "opponent_risk_mask": torch.zeros((1, 3, 34), dtype=torch.bool),
    }

    base = compute_losses(
        outputs,
        batch,
        value_weight=0.0,
        fan_weight=0.0,
        qualifying_fan_weight=0.0,
        risk_weight=0.0,
        hu_positive_weight=1.0,
    )
    weighted = compute_losses(
        outputs,
        batch,
        value_weight=0.0,
        fan_weight=0.0,
        qualifying_fan_weight=0.0,
        risk_weight=0.0,
        hu_positive_weight=3.0,
    )

    assert weighted["hu_loss"].item() == base["hu_loss"].item() * 3.0


def test_fan_loss_contributes_when_weighted() -> None:
    import torch
    from train import compute_losses

    outputs = {
        "discard_logits": torch.zeros((1, 34)),
        "claim_logits": torch.zeros((1, 7)),
        "self_kong_logits": torch.zeros((1, 3)),
        "hu_logits": torch.zeros((1, 2)),
        "value": torch.zeros((1, 1)),
        "fan_value": torch.tensor([[2.0]]),
        "qualifying_fan_value": torch.zeros((1, 1)),
        "opponent_tenpai_logits": torch.zeros((1, 3)),
        "opponent_risk_logits": torch.zeros((1, 3, 34)),
    }
    batch = {
        "discard_mask": torch.zeros((1, 34), dtype=torch.bool),
        "discard_target": torch.tensor([-100]),
        "claim_mask": torch.zeros((1, 7), dtype=torch.bool),
        "claim_target": torch.tensor([-100]),
        "self_kong_mask": torch.zeros((1, 3), dtype=torch.bool),
        "self_kong_target": torch.tensor([-100]),
        "hu_mask": torch.zeros((1, 2), dtype=torch.bool),
        "hu_target": torch.tensor([-100]),
        "value_target": torch.zeros((1, 1)),
        "fan_target": torch.zeros((1, 1)),
        "qualifying_fan_target": torch.zeros((1, 1)),
        "opponent_tenpai_target": torch.zeros((1, 3)),
        "opponent_risk_target": torch.zeros((1, 3, 34)),
        "opponent_risk_mask": torch.zeros((1, 3, 34), dtype=torch.bool),
    }

    losses = compute_losses(
        outputs,
        batch,
        value_weight=0.0,
        fan_weight=0.5,
        qualifying_fan_weight=0.0,
        risk_weight=0.0,
    )

    assert losses["fan_loss"].item() == 4.0
    assert losses["loss"].item() == 2.0


def test_qualifying_fan_loss_contributes_when_weighted() -> None:
    import torch
    from train import compute_losses

    outputs = {
        "discard_logits": torch.zeros((1, 34)),
        "claim_logits": torch.zeros((1, 7)),
        "self_kong_logits": torch.zeros((1, 3)),
        "hu_logits": torch.zeros((1, 2)),
        "value": torch.zeros((1, 1)),
        "fan_value": torch.zeros((1, 1)),
        "qualifying_fan_value": torch.tensor([[0.25]]),
        "opponent_tenpai_logits": torch.zeros((1, 3)),
        "opponent_risk_logits": torch.zeros((1, 3, 34)),
    }
    batch = {
        "discard_mask": torch.zeros((1, 34), dtype=torch.bool),
        "discard_target": torch.tensor([-100]),
        "claim_mask": torch.zeros((1, 7), dtype=torch.bool),
        "claim_target": torch.tensor([-100]),
        "self_kong_mask": torch.zeros((1, 3), dtype=torch.bool),
        "self_kong_target": torch.tensor([-100]),
        "hu_mask": torch.zeros((1, 2), dtype=torch.bool),
        "hu_target": torch.tensor([-100]),
        "value_target": torch.zeros((1, 1)),
        "fan_target": torch.zeros((1, 1)),
        "qualifying_fan_target": torch.tensor([[1.0]]),
        "opponent_tenpai_target": torch.zeros((1, 3)),
        "opponent_risk_target": torch.zeros((1, 3, 34)),
        "opponent_risk_mask": torch.zeros((1, 3, 34), dtype=torch.bool),
    }

    losses = compute_losses(
        outputs,
        batch,
        value_weight=0.0,
        fan_weight=0.0,
        qualifying_fan_weight=2.0,
        risk_weight=0.0,
    )

    assert losses["qualifying_fan_loss"].item() == 0.5625
    assert losses["loss"].item() == 1.125


def test_auxiliary_loss_weights_warm_up_to_targets() -> None:
    from train import loss_weights_for_epoch

    first = loss_weights_for_epoch(
        epoch=1,
        warmup_epochs=4,
        claim_weight=1.0,
        self_kong_weight=1.0,
        hu_weight=1.0,
        value_start=0.25,
        value_target=0.75,
        fan_start=0.1,
        fan_target=0.5,
        qualifying_fan_start=0.1,
        qualifying_fan_target=0.75,
        risk_start=0.25,
        risk_target=1.0,
    )
    final = loss_weights_for_epoch(
        epoch=4,
        warmup_epochs=4,
        claim_weight=1.0,
        self_kong_weight=1.0,
        hu_weight=1.0,
        value_start=0.25,
        value_target=0.75,
        fan_start=0.1,
        fan_target=0.5,
        qualifying_fan_start=0.1,
        qualifying_fan_target=0.75,
        risk_start=0.25,
        risk_target=1.0,
    )

    assert first["value_weight"] == 0.25
    assert first["fan_weight"] == 0.1
    assert first["qualifying_fan_weight"] == 0.1
    assert first["risk_weight"] == 0.25
    assert final["value_weight"] == 0.75
    assert final["fan_weight"] == 0.5
    assert final["qualifying_fan_weight"] == 0.75
    assert final["risk_weight"] == 1.0


def test_validation_selection_loss_weights_use_final_targets() -> None:
    from train import selection_loss_weights

    weights = selection_loss_weights(
        claim_weight=1.0,
        self_kong_weight=1.0,
        hu_weight=1.0,
        value_target=0.75,
        fan_target=0.5,
        qualifying_fan_target=0.75,
        risk_target=1.0,
    )

    assert weights["claim_weight"] == 1.0
    assert weights["self_kong_weight"] == 1.0
    assert weights["hu_weight"] == 1.0
    assert weights["value_weight"] == 0.75
    assert weights["fan_weight"] == 0.5
    assert weights["qualifying_fan_weight"] == 0.75
    assert weights["risk_weight"] == 1.0


def claim_row(base_row: dict, last_discard: str, middle_tile_key: str) -> dict:
    row = json.loads(json.dumps(base_row))
    row["decision_kind"] = "claim_window"
    row["context"]["last_discard_tile_key"] = last_discard
    row["legal_actions"] = ["pass", f"claim:chow:{middle_tile_key}"]
    row["label"] = {"type": "claim_chow", "middle_tile_key": middle_tile_key}
    return row


def write_fixture(tmp_path: Path) -> tuple[Path, Path]:
    metadata = {
        "schema_version": 7,
        "tile_keys": [
            "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9",
            "t1", "t2", "t3", "t4", "t5", "t6", "t7", "t8", "t9",
            "b1", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9",
            "east", "south", "west", "north", "red", "green", "white",
        ],
        "claim_actions": ["pass", "hu", "pung", "kong", "chow_left", "chow_mid", "chow_right"],
        "self_kong_actions": ["pass", "concealed_kong", "add_kong"],
    }
    row = {
        "schema_version": 7,
        "sample_weight": 1.0,
        "match_id": "fixture",
        "decision_index": 0,
        "seat_index": 0,
        "decision_kind": "active_turn",
        "context": {
            "seat_index": 0,
            "seat_count": 4,
            "dealer_seat": 0,
            "round_wind": "east",
            "cumulative_scores": [0, 0, 0, 0],
            "wall_tiles_remaining": 70,
            "visible_tile_keys": [],
            "opponent_discards_by_seat": [[], [], [], []],
            "opponent_melds_by_seat": [[], [], [], []],
            "player": {
                "concealed_tiles": [
                    {"tile_id": "w1#0", "tile_key": "w1", "is_flower": False},
                    {"tile_id": "t1#0", "tile_key": "t1", "is_flower": False},
                ],
                "concealed_tile_counts": [0] * 34,
                "meld_tile_key_groups": [],
                "flower_count": 0,
            },
            "restricted_discard_tile_key": None,
            "drawn_tile_id": "t1#0",
            "self_kong_candidates": [],
            "claim_options": [],
            "last_discard_tile_key": None,
            "add_kong_risk_tiles": [],
        },
        "legal_actions": ["discard:w1", "discard:t1"],
        "label": {"type": "discard", "tile_key": "w1"},
        "outcome": {
            "score_delta": 8,
            "fan_count": 0,
            "won": True,
            "dealt_in": False,
            "round_drawn": False,
        },
        "opponent_tenpai_target": [0.0, 0.0, 0.0],
        "opponent_risk_target": [[0.0] * 34 for _ in range(3)],
        "opponent_risk_mask": [[0.0] * 34 for _ in range(3)],
    }
    metadata_path = tmp_path / "metadata.json"
    train_path = tmp_path / "train.jsonl"
    metadata_path.write_text(json.dumps(metadata), encoding="utf-8")
    train_path.write_text(json.dumps(row) + "\n", encoding="utf-8")
    return metadata_path, train_path
