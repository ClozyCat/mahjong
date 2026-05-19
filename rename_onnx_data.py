"""Rename external ONNX data file reference in model files.

Usage:
    python rename_onnx_data.py  # preview (dry-run)
    python rename_onnx_data.py --apply  # actually rename

Renames data file references inside .onnx protos (aggroessive.onnx -> weights.data),
then renames the actual .data files on disk to match.

"""

import argparse
import os
import sys

import onnx

MODEL_DIRS = [
    "backend/assets/model_d",
    "backend/assets/model_a",
    "backend/assets/model_b",
    "backend/assets/model_s",
]

NEW_NAME = "weights.data"

parser = argparse.ArgumentParser()
parser.add_argument("--apply", action="store_true", help="Apply changes (default is dry-run)")


def main() -> None:
    args = parser.parse_args()
    repo_root = os.path.dirname(os.path.abspath(__file__))
    changes = {}

    for rel_dir in MODEL_DIRS:
        model_dir = os.path.join(repo_root, rel_dir)
        if not os.path.isdir(model_dir):
            continue
        for fname in os.listdir(model_dir):
            if not fname.endswith(".onnx"):
                continue
            onnx_path = os.path.join(model_dir, fname)
            model = onnx.load(onnx_path, load_external_data=False)
            old_name = None
            for t in model.graph.initializer:
                if t.HasField("data_location") and t.data_location == onnx.TensorProto.EXTERNAL:
                    for entry in t.external_data:
                        if entry.key == "location":
                            old_name = entry.value
                            entry.value = NEW_NAME
                            break
                    break

            if old_name is None:
                print(f"  {fname}: no external data found")
                continue
            changes[onnx_path] = (old_name, os.path.join(model_dir, old_name))
            print(f"  {fname}: {old_name} -> {NEW_NAME}")

            if args.apply:
                onnx.save(model, onnx_path)

    if not changes:
        print("Nothing to do.")
        return

    if args.apply:
        print()
        print("Renaming .data files on disk...")
        for onnx_path, (old_name, data_path) in changes.items():
            if os.path.exists(data_path):
                new_path = os.path.join(os.path.dirname(data_path), NEW_NAME)
                os.rename(data_path, new_path)
                print(f"  {os.path.basename(onnx_path)}: renamed {old_name} -> {NEW_NAME}")
            else:
                print(f"  {os.path.basename(onnx_path)}: {old_name} not found, skipping")
        print()
        print("Done.")
    else:
        print()
        print("Dry-run complete. Run with --apply to apply changes.")


if __name__ == "__main__":
    main()
