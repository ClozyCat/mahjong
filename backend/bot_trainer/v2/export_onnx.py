from __future__ import annotations

import argparse
import sys
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


class OnnxWrapper(nn.Module):
    def __init__(self, model: nn.Module) -> None:
        super().__init__()
        self.model = model

    def forward(
        self,
        tile_planes: torch.Tensor,
        scalar_features: torch.Tensor,
        discard_sequence: torch.Tensor | None = None,
    ) -> tuple[torch.Tensor, ...]:
        config = getattr(self.model, "config", None)
        if getattr(config, "use_discard_sequence", False):
            outputs = self.model(tile_planes, scalar_features, discard_sequence)
        else:
            outputs = self.model(tile_planes, scalar_features)
        return tuple(outputs[name] for name in OUTPUT_NAMES)


def main() -> None:
    args = parse_args()
    checkpoint = torch.load(args.checkpoint, map_location="cpu")
    model_config = ModelConfig.from_dict(checkpoint.get("model_config", {}))
    model = build_model(model_config)
    model.load_state_dict(checkpoint["model_state"])
    model.eval()

    args.output.parent.mkdir(parents=True, exist_ok=True)
    tile_plane_count = model.tile_plane_count
    scalar_count = model.scalar_feature_count
    dummy_tile_planes = torch.zeros((1, tile_plane_count, 34), dtype=torch.float32)
    dummy_scalar_features = torch.zeros((1, scalar_count), dtype=torch.float32)
    dummy_discard_sequence = torch.zeros((1, 64, 38), dtype=torch.float32)
    export_inputs: tuple[torch.Tensor, ...] = (
        (dummy_tile_planes, dummy_scalar_features, dummy_discard_sequence)
        if model_config.use_discard_sequence
        else (dummy_tile_planes, dummy_scalar_features)
    )
    input_names = ["tile_planes", "scalar_features"]
    if model_config.use_discard_sequence:
        input_names.append("discard_sequence")

    torch.onnx.export(
        OnnxWrapper(model),
        export_inputs,
        args.output,
        input_names=input_names,
        output_names=OUTPUT_NAMES,
        dynamic_axes={
            "tile_planes": {0: "batch"},
            "scalar_features": {0: "batch"},
            **({"discard_sequence": {0: "batch"}} if model_config.use_discard_sequence else {}),
            **{name: {0: "batch"} for name in OUTPUT_NAMES},
        },
        opset_version=args.opset,
        dynamo=False,
    )
    smoke_onnxruntime(
        args.output,
        dummy_tile_planes,
        dummy_scalar_features,
        dummy_discard_sequence if model_config.use_discard_sequence else None,
    )
    print(f"exported {args.output}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--checkpoint", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--opset", type=int, default=17)
    return parser.parse_args()


def smoke_onnxruntime(
    model_path: Path,
    tile_planes: torch.Tensor,
    scalar_features: torch.Tensor,
    discard_sequence: torch.Tensor | None = None,
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
    }
    if discard_sequence is not None:
        inputs["discard_sequence"] = discard_sequence.numpy()
    outputs = session.run(OUTPUT_NAMES, inputs)
    expected_shapes = [(1, 34), (1, 7), (1, 3), (1, 2), (1, 1), (1, 34)]
    for name, output, expected_shape in zip(OUTPUT_NAMES, outputs, expected_shapes, strict=True):
        if tuple(output.shape) != expected_shape:
            raise RuntimeError(f"{name} shape {tuple(output.shape)} != {expected_shape}")


if __name__ == "__main__":
    main()
