from __future__ import annotations

import argparse
import json
import sys
import warnings
from datetime import UTC, datetime
from pathlib import Path

try:
    import torch
    from torch import nn
except ModuleNotFoundError as exc:  # pragma: no cover
    raise SystemExit("PyTorch is required: pip install torch") from exc

from model import ModelConfig, build_model, build_actor_critic


def onnx_output_names(model_path: Path) -> list[str] | None:
    try:
        import onnxruntime as ort
    except ModuleNotFoundError:
        return None

    try:
        session = ort.InferenceSession(str(model_path), providers=["CPUExecutionProvider"])
    except Exception as exc:
        print(f"Could not inspect existing ONNX model {model_path}: {exc}", file=sys.stderr)
        return None
    return [output.name for output in session.get_outputs()]


def quantize_onnx(fp32_path: Path) -> Path | None:
    """Create an int8 quantized copy alongside the fp32 model.

    Returns the quantized path, or None if quantization failed.
    """
    try:
        from onnxruntime.quantization import QuantType, quantize_dynamic
        from onnxruntime.quantization.shape_inference import quant_pre_process
    except ImportError:
        print("onnxruntime.quantization not available; skipping int8 quantization", file=sys.stderr)
        return None

    quant_path = fp32_path.with_name(fp32_path.stem + ".quant.onnx")
    if quant_path.exists():
        existing_outputs = onnx_output_names(quant_path)
        is_current = existing_outputs == OUTPUT_NAMES and (
            quant_path.stat().st_mtime >= fp32_path.stat().st_mtime
        )
        if is_current:
            print(f"Quantized model already exists: {quant_path}")
            return quant_path
        print(
            f"Quantized model is stale; rebuilding {quant_path}",
            file=sys.stderr,
        )
        try:
            quant_path.unlink()
        except OSError as exc:
            print(f"Could not remove stale quantized model {quant_path}: {exc}", file=sys.stderr)
            return None

    import tempfile

    try:
        with tempfile.TemporaryDirectory() as tmp:
            preprocessed = Path(tmp) / "preprocessed.onnx"
            quant_pre_process(fp32_path.as_posix(), preprocessed.as_posix())
            quantize_dynamic(
                preprocessed.as_posix(),
                quant_path.as_posix(),
                weight_type=QuantType.QInt8,
                op_types_to_quantize=["MatMul"],
            )
    except Exception as exc:
        print(f"int8 quantization failed: {exc}", file=sys.stderr)
        return None

    print(f"Exported int8 quantized model: {quant_path}")
    return quant_path


OUTPUT_NAMES = [
    "discard_logits",
    "claim_logits",
    "self_kong_logits",
    "hu_logits",
    "value",
    "fan_value",
    "qualifying_fan_value",
    "risk_logits",
]
INPUT_NAMES = ["tile_planes", "scalar_features", "discard_sequence"]


def make_exporter_logging_windows_safe() -> None:
    for stream in (sys.stdout, sys.stderr):
        reconfigure = getattr(stream, "reconfigure", None)
        if reconfigure is not None:
            reconfigure(errors="replace")


class OnnxWrapper(nn.Module):
    def __init__(self, model: nn.Module) -> None:
        super().__init__()
        self.model = model

    def forward(
        self,
        tile_planes: torch.Tensor,
        scalar_features: torch.Tensor,
        discard_sequence: torch.Tensor,
    ) -> tuple[torch.Tensor, ...]:
        outputs = self.model(tile_planes, scalar_features, discard_sequence)
        return tuple(outputs[name] for name in OUTPUT_NAMES)


def load_export_model(checkpoint_path: Path) -> tuple[nn.Module, ModelConfig, bool]:
    checkpoint = torch.load(checkpoint_path, map_location="cpu")
    model_config = ModelConfig.from_dict(checkpoint.get("model_config", {}))
    state_dict = checkpoint["model_state"]
    is_actor_critic = any(
        key.startswith("actor.") or key.startswith("critic.")
        for key in state_dict.keys()
    )

    if is_actor_critic:
        print("Detected actor-critic checkpoint, exporting local inference wrapper")
        model = build_actor_critic(model_config)
    else:
        print("Detected shared policy-value checkpoint")
        model = build_model(model_config)
    missing, _ = model.load_state_dict(state_dict, strict=False)
    if missing:
        print(
            f"ONNX export: checkpoint missing keys (new params initialized fresh): {missing}",
            file=sys.stderr,
        )
    return model, model_config, is_actor_critic


def main() -> None:
    make_exporter_logging_windows_safe()
    args = parse_args()

    if args.quantize_only is not None:
        quantize_onnx(args.quantize_only)
        return

    model, model_config, is_actor_critic = load_export_model(args.checkpoint)
    checkpoint = torch.load(args.checkpoint, map_location="cpu")

    model.eval()

    args.output.parent.mkdir(parents=True, exist_ok=True)
    tile_plane_count = model.tile_plane_count
    scalar_count = model.scalar_feature_count
    discard_sequence_length = model.discard_sequence_length
    discard_event_feature_count = model.discard_event_feature_count
    dummy_tile_planes = torch.zeros((1, tile_plane_count, 34), dtype=torch.float32)
    dummy_scalar_features = torch.zeros((1, scalar_count), dtype=torch.float32)
    dummy_discard_sequence = torch.zeros(
        (1, discard_sequence_length, discard_event_feature_count),
        dtype=torch.float32,
    )
    export_model = OnnxWrapper(model).eval()
    export_inputs: tuple[torch.Tensor, ...] = (
        dummy_tile_planes,
        dummy_scalar_features,
        dummy_discard_sequence,
    )

    with warnings.catch_warnings():
        warnings.filterwarnings(
            "ignore",
            message=r"The tensor attributes .* were assigned during export.*",
            category=UserWarning,
        )
        warnings.filterwarnings(
            "ignore",
            message=r"`isinstance\(treespec, LeafSpec\)` is deprecated.*",
            category=FutureWarning,
        )
        torch.onnx.export(
            export_model,
            export_inputs,
            args.output,
            input_names=INPUT_NAMES,
            output_names=OUTPUT_NAMES,
            opset_version=args.opset,
            dynamo=True,
        )
    smoke_onnxruntime(
        args.output,
        dummy_tile_planes,
        dummy_scalar_features,
        dummy_discard_sequence,
    )
    write_export_manifest(args.output, args.checkpoint, checkpoint, model_config, is_actor_critic)
    print(f"exported {args.output}")

    if args.quantize:
        quant_path = quantize_onnx(args.output)
        if quant_path is not None:
            smoke_onnxruntime(
                quant_path,
                dummy_tile_planes,
                dummy_scalar_features,
                dummy_discard_sequence,
            )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--checkpoint", type=Path, default=None)
    parser.add_argument("--output", type=Path, default=None)
    parser.add_argument("--opset", type=int, default=18)
    parser.add_argument("--quantize", action=argparse.BooleanOptionalAction, default=True)
    parser.add_argument("--quantize-only", type=Path, default=None,
                        help="Quantize an existing ONNX file without exporting from PyTorch")
    args = parser.parse_args()
    if args.quantize_only is None:
        if args.checkpoint is None or args.output is None:
            parser.error("--checkpoint and --output are required when not using --quantize-only")
    return args


def smoke_onnxruntime(
    model_path: Path,
    tile_planes: torch.Tensor,
    scalar_features: torch.Tensor,
    discard_sequence: torch.Tensor,
) -> None:
    try:
        import onnxruntime as ort
    except ModuleNotFoundError:
        print("onnxruntime is not installed; skipped ONNX runtime smoke check", file=sys.stderr)
        return
    session = ort.InferenceSession(str(model_path), providers=["CPUExecutionProvider"])
    inputs = {
        "tile_planes": tile_planes.numpy(),
        "scalar_features": scalar_features.numpy(),
        "discard_sequence": discard_sequence.numpy(),
    }
    outputs = session.run(OUTPUT_NAMES, inputs)
    expected_shapes = [(1, 34), (1, 7), (1, 3), (1, 2), (1, 1), (1, 1), (1, 1), (1, 34)]
    for name, output, expected_shape in zip(OUTPUT_NAMES, outputs, expected_shapes, strict=True):
        if tuple(output.shape) != expected_shape:
            raise RuntimeError(f"{name} shape {tuple(output.shape)} != {expected_shape}")


def write_export_manifest(
    output: Path,
    checkpoint_path: Path,
    checkpoint: dict[str, object],
    model_config: ModelConfig,
    is_actor_critic: bool = False,
) -> None:
    manifest = {
        "created_at_utc": datetime.now(UTC).isoformat(),
        "onnx": output.as_posix(),
        "checkpoint": checkpoint_path.as_posix(),
        "training_source": checkpoint.get("training_source", "unknown"),
        "checkpoint_created_at_utc": checkpoint.get("created_at_utc"),
        "model_config": model_config.to_dict(),
        "outputs": OUTPUT_NAMES,
        "is_actor_critic": is_actor_critic,
        "exported_component": (
            "actor_critic_local_inference_wrapper"
            if is_actor_critic
            else "full_model"
        ),
    }
    manifest_path = output.with_suffix(output.suffix + ".manifest.json")
    manifest_path.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
