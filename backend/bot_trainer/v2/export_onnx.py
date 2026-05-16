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

from model import ModelConfig, build_model


OUTPUT_NAMES = [
    "discard_logits",
    "claim_logits",
    "self_kong_logits",
    "hu_logits",
    "value",
    "risk_logits",
]
INPUT_NAMES = ["tile_planes", "scalar_features", "discard_sequence"]


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


def main() -> None:
    args = parse_args()
    checkpoint = torch.load(args.checkpoint, map_location="cpu")
    model_config = ModelConfig.from_dict(checkpoint.get("model_config", {}))
    model = build_model(model_config)
    missing, _ = model.load_state_dict(checkpoint["model_state"], strict=False)
    if missing:
        print(f"ONNX export: checkpoint missing keys (new params initialized fresh): {missing}", file=sys.stderr)
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
    write_export_manifest(args.output, args.checkpoint, checkpoint, model_config)
    print(f"exported {args.output}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--checkpoint", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--opset", type=int, default=18)
    return parser.parse_args()


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
    expected_shapes = [(1, 34), (1, 7), (1, 3), (1, 2), (1, 1), (1, 34)]
    for name, output, expected_shape in zip(OUTPUT_NAMES, outputs, expected_shapes, strict=True):
        if tuple(output.shape) != expected_shape:
            raise RuntimeError(f"{name} shape {tuple(output.shape)} != {expected_shape}")


def write_export_manifest(
    output: Path,
    checkpoint_path: Path,
    checkpoint: dict[str, object],
    model_config: ModelConfig,
) -> None:
    manifest = {
        "created_at_utc": datetime.now(UTC).isoformat(),
        "onnx": output.as_posix(),
        "checkpoint": checkpoint_path.as_posix(),
        "training_source": checkpoint.get("training_source", "unknown"),
        "checkpoint_created_at_utc": checkpoint.get("created_at_utc"),
        "model_config": model_config.to_dict(),
        "outputs": OUTPUT_NAMES,
    }
    manifest_path = output.with_suffix(output.suffix + ".manifest.json")
    manifest_path.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
