"""Normalize external ONNX data file references.

Usage:
    python rename_onnx_data.py
    python rename_onnx_data.py --apply
    python rename_onnx_data.py --model-dir backend/assets/ppo --apply

The script rewrites every external-data ``location`` entry in each .onnx file to
``weights.data`` and, when applying changes, copies the referenced .data file to
that name if needed. A dry run prints the planned changes without touching files.
"""

from __future__ import annotations

import argparse
import shutil
from pathlib import Path

import onnx

DEFAULT_MODEL_DIRS = (
    "backend/assets/trainning",
)
DEFAULT_DATA_NAME = "weights.data"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--apply", action="store_true", help="Apply changes")
    parser.add_argument(
        "--model-dir",
        action="append",
        dest="model_dirs",
        help="Directory containing .onnx files. Can be passed multiple times.",
    )
    parser.add_argument(
        "--new-name",
        default=DEFAULT_DATA_NAME,
        help=f"Target external data filename. Default: {DEFAULT_DATA_NAME}",
    )
    return parser.parse_args()


def rewrite_external_data_locations(
    model: onnx.ModelProto,
    new_name: str,
) -> set[str]:
    old_locations: set[str] = set()
    for tensor in model.graph.initializer:
        if tensor.data_location != onnx.TensorProto.EXTERNAL:
            continue
        for entry in tensor.external_data:
            if entry.key != "location":
                continue
            old_locations.add(entry.value)
            entry.value = new_name
    return old_locations


def onnx_files(model_dir: Path) -> list[Path]:
    if not model_dir.is_dir():
        return []
    return sorted(path for path in model_dir.iterdir() if path.suffix == ".onnx")


def pick_source_data_file(model_dir: Path, old_locations: set[str], new_name: str) -> Path | None:
    candidates = [model_dir / location for location in sorted(old_locations)]
    existing = [path for path in candidates if path.exists()]
    if existing:
        return existing[0]
    target = model_dir / new_name
    if target.exists():
        return target
    return None


def process_model(onnx_path: Path, new_name: str, apply: bool) -> bool:
    model = onnx.load(onnx_path, load_external_data=False)
    old_locations = rewrite_external_data_locations(model, new_name)
    if not old_locations:
        print(f"  {onnx_path.name}: no external data found")
        return False

    model_dir = onnx_path.parent
    missing = sorted(
        location for location in old_locations if not (model_dir / location).exists()
    )
    old_list = ", ".join(sorted(old_locations))
    print(f"  {onnx_path.name}: {old_list} -> {new_name}")
    if missing:
        print(f"    missing before rewrite: {', '.join(missing)}")

    if not apply:
        return True

    source = pick_source_data_file(model_dir, old_locations, new_name)
    if source is None:
        raise FileNotFoundError(
            f"{onnx_path}: no referenced .data file exists; expected one of "
            f"{sorted(old_locations)} or {new_name}"
        )

    target = model_dir / new_name
    if source.resolve() != target.resolve():
        if target.exists():
            raise FileExistsError(
                f"{target} already exists; remove it or choose --new-name explicitly"
            )
        shutil.copy2(source, target)
        print(f"    copied {source.name} -> {target.name}")

    onnx.save(model, onnx_path)
    return True


def main() -> None:
    args = parse_args()
    repo_root = Path(__file__).resolve().parent
    model_dirs = args.model_dirs or list(DEFAULT_MODEL_DIRS)

    changed = False
    for rel_dir in model_dirs:
        model_dir = (repo_root / rel_dir).resolve()
        if not model_dir.is_dir():
            print(f"{rel_dir}: directory not found, skipping")
            continue
        print(f"{model_dir}:")
        for path in onnx_files(model_dir):
            changed = process_model(path, args.new_name, args.apply) or changed

    if not changed:
        print("Nothing to do.")
    elif args.apply:
        print("Done.")
    else:
        print("Dry-run complete. Run with --apply to apply changes.")


if __name__ == "__main__":
    main()
