from __future__ import annotations

import argparse
import json
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

import torch


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--checkpoint", type=Path, required=True)
    parser.add_argument("--onnx", type=Path, default=None)
    parser.add_argument("--allow-rl-checkpoint", action="store_true")
    return parser.parse_args()


def load_checkpoint_payload(path: Path) -> dict[str, Any]:
    if not path.exists():
        raise FileNotFoundError(f"checkpoint not found: {path}")
    payload = torch.load(path, map_location="cpu")
    if not isinstance(payload, dict):
        raise ValueError(f"checkpoint payload must be a dict: {path}")
    return payload


def checkpoint_training_source(payload_or_path: dict[str, Any] | Path) -> str:
    payload = (
        load_checkpoint_payload(payload_or_path)
        if isinstance(payload_or_path, Path)
        else payload_or_path
    )
    source = payload.get("training_source")
    if source:
        return str(source)
    if "rl_metrics" in payload:
        return "rl"
    if "metrics" in payload:
        return "sft"
    return "unknown"


def checkpoint_manifest(path: Path) -> dict[str, Any]:
    payload = load_checkpoint_payload(path)
    return {
        "path": path.as_posix(),
        "training_source": checkpoint_training_source(payload),
        "created_at_utc": payload.get("created_at_utc"),
        "model_config": payload.get("model_config", {}),
        "has_model_state": "model_state" in payload,
    }


def validate_baseline_checkpoint(
    path: Path,
    allow_rl_checkpoint: bool = False,
) -> dict[str, Any]:
    manifest = checkpoint_manifest(path)
    if manifest["training_source"] == "rl" and not allow_rl_checkpoint:
        raise ValueError(
            f"RL checkpoint cannot be used as an SFT baseline: {path}. "
            "Pass the explicit continuation flag only when intentionally continuing RL."
        )
    if not manifest["has_model_state"]:
        raise ValueError(f"checkpoint has no model_state: {path}")
    return manifest


def onnx_manifest(path: Path) -> dict[str, Any]:
    if not path.exists():
        raise FileNotFoundError(f"ONNX model not found: {path}")
    sidecar = onnx_sidecar_manifest(path)
    try:
        import onnxruntime as ort
    except ModuleNotFoundError:
        return {
            "path": path.as_posix(),
            "available": True,
            "input_shapes": None,
            "checked_with_onnxruntime": False,
            "sidecar": sidecar,
        }
    session = ort.InferenceSession(str(path), providers=["CPUExecutionProvider"])
    return {
        "path": path.as_posix(),
        "available": True,
        "input_shapes": {input_.name: list(input_.shape) for input_ in session.get_inputs()},
        "checked_with_onnxruntime": True,
        "sidecar": sidecar,
    }


def onnx_sidecar_manifest(path: Path) -> dict[str, Any] | None:
    manifest_path = path.with_suffix(path.suffix + ".manifest.json")
    if not manifest_path.exists():
        return None
    return json.loads(manifest_path.read_text(encoding="utf-8"))


def validate_checkpoint_onnx_pair(
    checkpoint: Path,
    onnx: Path,
    allow_rl_checkpoint: bool = False,
) -> dict[str, Any]:
    checkpoint_info = validate_baseline_checkpoint(checkpoint, allow_rl_checkpoint)
    onnx_info = onnx_manifest(onnx)
    input_shapes = onnx_info.get("input_shapes")
    model_config = checkpoint_info.get("model_config") or {}
    if input_shapes:
        tile_shape = input_shapes.get("tile_planes") or []
        scalar_shape = input_shapes.get("scalar_features") or []
        tile_plane_count = model_config.get("tile_plane_count")
        scalar_feature_count = model_config.get("scalar_feature_count")
        if tile_plane_count is not None and len(tile_shape) >= 2:
            if int(tile_shape[1]) != int(tile_plane_count):
                raise ValueError("checkpoint/ONNX tile_plane_count mismatch")
        if scalar_feature_count is not None and len(scalar_shape) >= 2:
            if int(scalar_shape[1]) != int(scalar_feature_count):
                raise ValueError("checkpoint/ONNX scalar_feature_count mismatch")
    sidecar = onnx_info.get("sidecar")
    if sidecar:
        if sidecar.get("training_source") != checkpoint_info.get("training_source"):
            raise ValueError("checkpoint/ONNX training_source mismatch")
        if sidecar.get("model_config") != checkpoint_info.get("model_config"):
            raise ValueError("checkpoint/ONNX model_config mismatch")
    return {
        "checked_at_utc": datetime.now(UTC).isoformat(),
        "checkpoint": checkpoint_info,
        "onnx": onnx_info,
    }


def main() -> None:
    args = parse_args()
    try:
        if args.onnx is None:
            manifest = validate_baseline_checkpoint(args.checkpoint, args.allow_rl_checkpoint)
        else:
            manifest = validate_checkpoint_onnx_pair(
                args.checkpoint,
                args.onnx,
                args.allow_rl_checkpoint,
            )
    except (FileNotFoundError, ValueError) as exc:
        raise SystemExit(str(exc)) from exc
    print(json.dumps(manifest, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
