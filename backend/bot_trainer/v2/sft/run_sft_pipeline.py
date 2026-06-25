from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


EXPECTED_METADATA_SCHEMA_VERSION = 6
DEFAULT_INPUT_PATH = "backend/bot_trainer/datasets/data.txt"
DEFAULT_DATASETS2_INPUT_PATH = "backend/bot_trainer/datasets2"
DEFAULT_DATA_DIR = "backend/bot_trainer/v2/sft/out"
DEFAULT_CHECKPOINT_DIR = "backend/bot_trainer/v2/sft/checkpoints"
DEFAULT_ONNX_OUTPUT = "backend/assets/sft/sft.onnx"


@dataclass(frozen=True)
class DatasetStatus:
    complete: bool
    metadata_schema_version: int | None
    missing_files: tuple[str, ...]


def repo_root() -> Path:
    return Path(__file__).resolve().parents[4]


def dataset_status(data_dir: Path) -> DatasetStatus:
    required_files = ("metadata.json", "train.jsonl", "val.jsonl", "test.jsonl")
    missing = tuple(name for name in required_files if not (data_dir / name).is_file())
    schema_version = None
    metadata_path = data_dir / "metadata.json"
    if metadata_path.is_file():
        try:
            metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
            raw_version = metadata.get("schema_version")
            if isinstance(raw_version, int):
                schema_version = raw_version
        except json.JSONDecodeError:
            schema_version = None
    return DatasetStatus(
        complete=not missing,
        metadata_schema_version=schema_version,
        missing_files=missing,
    )


def bool_prompt(prompt: str, default: bool) -> bool:
    suffix = "Y/n" if default else "y/N"
    while True:
        answer = input(f"{prompt} [{suffix}]: ").strip().lower()
        if not answer:
            return default
        if answer in {"y", "yes"}:
            return True
        if answer in {"n", "no"}:
            return False
        print("Please answer y or n.")


def choice_prompt(prompt: str, choices: tuple[str, ...], default: str) -> str:
    joined = "/".join(choice.upper() if choice == default else choice for choice in choices)
    while True:
        answer = input(f"{prompt} [{joined}]: ").strip().lower()
        if not answer:
            return default
        if answer in choices:
            return answer
        print(f"Please choose one of: {', '.join(choices)}.")


def value_prompt(prompt: str, default: str) -> str:
    answer = input(f"{prompt} [{default}]: ").strip()
    return answer or default


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run the Mahjong SFT dataset/train/export pipeline.")
    parser.add_argument("--input", default=DEFAULT_INPUT_PATH)
    parser.add_argument("--input-format", choices=("botzone", "datasets2", "both"), default="botzone")
    parser.add_argument("--datasets2-input", default=DEFAULT_DATASETS2_INPUT_PATH)
    parser.add_argument("--data-dir", default=DEFAULT_DATA_DIR)
    parser.add_argument("--checkpoint-dir", default=DEFAULT_CHECKPOINT_DIR)
    parser.add_argument("--onnx-output", default=DEFAULT_ONNX_OUTPUT)
    parser.add_argument("--progress-every", type=int, default=10000)
    parser.add_argument("--max-matches", type=int, default=0)
    parser.add_argument("--export-workers", type=int, default=0)
    parser.add_argument("--epochs", type=int, default=20)
    parser.add_argument("--batch-size", type=int, default=1024)
    parser.add_argument("--num-workers", type=int, default=0)
    parser.add_argument("--data-cache-dir", default="")
    parser.add_argument("--python-exe", default=sys.executable)
    parser.add_argument("--device", choices=("auto", "cuda", "cpu", "dml"), default="auto")
    parser.add_argument("--lr", type=float, default=0.0003)
    parser.add_argument("--lr-min", type=float, default=0.00001)
    parser.add_argument("--weight-decay", type=float, default=0.0001)
    parser.add_argument("--claim-loss-weight", type=float, default=1.0)
    parser.add_argument("--self-kong-loss-weight", type=float, default=1.0)
    parser.add_argument("--hu-loss-weight", type=float, default=1.0)
    parser.add_argument("--value-loss-weight", type=float, default=0.75)
    parser.add_argument("--fan-loss-weight", type=float, default=0.5)
    parser.add_argument("--qualifying-fan-loss-weight", type=float, default=0.75)
    parser.add_argument("--risk-loss-weight", type=float, default=1.0)
    parser.add_argument("--risk-pos-weight", type=float, default=300.0)
    parser.add_argument("--value-loss-start-weight", type=float, default=0.25)
    parser.add_argument("--fan-loss-start-weight", type=float, default=0.1)
    parser.add_argument("--qualifying-fan-loss-start-weight", type=float, default=0.1)
    parser.add_argument("--risk-loss-start-weight", type=float, default=0.25)
    parser.add_argument("--aux-loss-warmup-epochs", type=int, default=4)
    parser.add_argument("--claim-rare-action-weight", type=float, default=2.0)
    parser.add_argument("--self-kong-rare-action-weight", type=float, default=3.0)
    parser.add_argument("--hu-positive-weight", type=float, default=3.0)
    parser.add_argument("--grad-clip-norm", type=float, default=1.0)
    parser.add_argument("--max-nan-tolerance", type=int, default=2)
    parser.add_argument("--early-stop-patience", type=int, default=0)
    parser.add_argument("--rebuild-data-cache", action="store_true")
    parser.add_argument("--amp", dest="amp", action="store_true", default=None)
    parser.add_argument("--no-amp", dest="amp", action="store_false")
    parser.add_argument("--no-tf32", action="store_true")
    parser.add_argument("--compile-model", action="store_true")
    parser.add_argument("--skip-tests", action="store_true")
    parser.add_argument("--skip-onnx-export", action="store_true")
    parser.add_argument("--skip-export-dataset", action="store_true")
    parser.add_argument("--yes", action="store_true", help="Run non-interactively with defaults.")
    return parser.parse_args()


def run_command(command: list[str], cwd: Path, env: dict[str, str] | None = None) -> None:
    print()
    print("$ " + " ".join(command))
    completed = subprocess.run(command, cwd=cwd, env=env, check=False)
    if completed.returncode != 0:
        raise SystemExit(completed.returncode)


def assert_dataset_contract(data_dir: Path) -> None:
    status = dataset_status(data_dir)
    if not status.complete:
        raise SystemExit(f"Dataset is incomplete; missing: {', '.join(status.missing_files)}")
    if status.metadata_schema_version != EXPECTED_METADATA_SCHEMA_VERSION:
        raise SystemExit(
            "Unsupported dataset schema: "
            f"{status.metadata_schema_version}; expected {EXPECTED_METADATA_SCHEMA_VERSION}."
        )


def prompt_pipeline(args: argparse.Namespace, root: Path) -> None:
    if args.yes:
        return
    print("Mahjong SFT pipeline")
    args.input_format = choice_prompt(
        "Input format",
        ("botzone", "datasets2", "both"),
        args.input_format,
    )
    if args.input_format == "datasets2" and args.input == DEFAULT_INPUT_PATH:
        args.input = DEFAULT_DATASETS2_INPUT_PATH
    args.input = value_prompt("Input replay path", args.input)
    if args.input_format == "both":
        args.datasets2_input = value_prompt("Datasets2 input directory", args.datasets2_input)
    args.data_dir = value_prompt("Dataset output directory", args.data_dir)
    args.checkpoint_dir = value_prompt("Checkpoint directory", args.checkpoint_dir)
    args.onnx_output = value_prompt("ONNX output path", args.onnx_output)
    args.device = choice_prompt("Device", ("auto", "cuda", "cpu", "dml"), args.device)
    args.epochs = int(value_prompt("Training epochs", str(args.epochs)))
    args.batch_size = int(value_prompt("Batch size", str(args.batch_size)))
    args.export_workers = int(value_prompt("Dataset export workers", str(args.export_workers)))
    args.num_workers = int(value_prompt("Data loader workers", str(args.num_workers)))

    data_dir = (root / args.data_dir).resolve()
    status = dataset_status(data_dir)
    if status.complete:
        schema = status.metadata_schema_version
        print(f"Existing dataset found at {data_dir} (schema={schema}).")
        action = choice_prompt("Dataset export action", ("skip", "overwrite", "exit"), "skip")
        if action == "skip":
            args.skip_export_dataset = True
        elif action == "exit":
            raise SystemExit(0)
    else:
        if status.missing_files:
            print(f"Dataset is incomplete; missing: {', '.join(status.missing_files)}")
        args.skip_export_dataset = not bool_prompt("Run dataset export", True)

    args.skip_tests = not bool_prompt("Run Python tests before training", not args.skip_tests)
    args.skip_onnx_export = not bool_prompt("Export ONNX after training", not args.skip_onnx_export)
    args.rebuild_data_cache = bool_prompt("Rebuild tensor data cache", args.rebuild_data_cache)


def export_dataset(args: argparse.Namespace, root: Path) -> None:
    if args.input_format == "both":
        export_combined_dataset(args, root)
        return

    run_export_command(
        args=args,
        root=root,
        input_format=args.input_format,
        input_path=args.input,
        output_dir=Path(args.data_dir),
    )


def export_combined_dataset(args: argparse.Namespace, root: Path) -> None:
    temp_root = root / ".tmp" / "bot-trainer-v2-sft" / "combined-export"
    if temp_root.exists():
        shutil.rmtree(temp_root)
    botzone_dir = temp_root / "botzone"
    datasets2_dir = temp_root / "datasets2"

    run_export_command(
        args=args,
        root=root,
        input_format="botzone",
        input_path=args.input,
        output_dir=botzone_dir,
    )
    run_export_command(
        args=args,
        root=root,
        input_format="datasets2",
        input_path=args.datasets2_input,
        output_dir=datasets2_dir,
    )
    merge_exported_datasets((botzone_dir, datasets2_dir), resolve_output_path(root, args.data_dir))


def run_export_command(
    args: argparse.Namespace,
    root: Path,
    input_format: str,
    input_path: str,
    output_dir: Path,
) -> None:
    binary_name = (
        "export_bot_dataset_v2_datasets2"
        if input_format == "datasets2"
        else "export_bot_dataset_v2"
    )
    command = [
        "cargo",
        "run",
        "--release",
        "--manifest-path",
        "backend/Cargo.toml",
        "--bin",
        binary_name,
        "--",
        "--input",
        str(input_path),
        "--output",
        str(output_dir),
        "--progress-every",
        str(args.progress_every),
    ]
    if args.max_matches > 0:
        command.extend(["--max-matches", str(args.max_matches)])
    if args.export_workers > 0:
        command.extend(["--workers", str(args.export_workers)])
    run_command(command, root)


def merge_exported_datasets(input_dirs: tuple[Path, ...], output_dir: Path) -> None:
    if output_dir.exists():
        shutil.rmtree(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    first_metadata = input_dirs[0] / "metadata.json"
    shutil.copyfile(first_metadata, output_dir / "metadata.json")
    expected_metadata = first_metadata.read_text(encoding="utf-8")
    for input_dir in input_dirs[1:]:
        metadata = (input_dir / "metadata.json").read_text(encoding="utf-8")
        if metadata != expected_metadata:
            raise SystemExit(f"Dataset metadata mismatch: {input_dir / 'metadata.json'}")

    for split_name in ("train.jsonl", "val.jsonl", "test.jsonl"):
        with (output_dir / split_name).open("w", encoding="utf-8", newline="\n") as target:
            for input_dir in input_dirs:
                source_path = input_dir / split_name
                with source_path.open("r", encoding="utf-8") as source:
                    shutil.copyfileobj(source, target)


def resolve_output_path(root: Path, path: str) -> Path:
    output = Path(path)
    return output if output.is_absolute() else root / output


def run_tests(args: argparse.Namespace, root: Path, env: dict[str, str]) -> None:
    if args.skip_tests:
        return
    if shutil.which(args.python_exe) is None and not Path(args.python_exe).exists():
        raise SystemExit(f"Python executable not found: {args.python_exe}")
    probe = "import importlib.util, sys; sys.exit(0 if importlib.util.find_spec('pytest') else 2)"
    completed = subprocess.run([args.python_exe, "-c", probe], cwd=root, env=env, check=False)
    if completed.returncode != 0:
        print("pytest is not installed for this Python; skipping Python tests.")
        return
    run_command(
        [
            args.python_exe,
            "-m",
            "pytest",
            "backend/bot_trainer/v2/sft/tests",
            "-q",
            "--basetemp",
            str(root / ".tmp" / "bot-trainer-v2-sft" / "pytest"),
        ],
        root,
        env,
    )


def train_model(args: argparse.Namespace, root: Path, env: dict[str, str]) -> None:
    data_cache_dir = args.data_cache_dir or str(Path(args.data_dir) / ".tensor_cache")
    command = [
        args.python_exe,
        "backend/bot_trainer/v2/sft/train.py",
        "--data",
        args.data_dir,
        "--epochs",
        str(args.epochs),
        "--batch-size",
        str(args.batch_size),
        "--output",
        args.checkpoint_dir,
        "--device",
        args.device,
        "--num-workers",
        str(args.num_workers),
        "--data-cache-dir",
        data_cache_dir,
        "--lr",
        str(args.lr),
        "--lr-min",
        str(args.lr_min),
        "--weight-decay",
        str(args.weight_decay),
        "--claim-loss-weight",
        str(args.claim_loss_weight),
        "--self-kong-loss-weight",
        str(args.self_kong_loss_weight),
        "--hu-loss-weight",
        str(args.hu_loss_weight),
        "--value-loss-weight",
        str(args.value_loss_weight),
        "--fan-loss-weight",
        str(args.fan_loss_weight),
        "--qualifying-fan-loss-weight",
        str(args.qualifying_fan_loss_weight),
        "--risk-loss-weight",
        str(args.risk_loss_weight),
        "--risk-pos-weight",
        str(args.risk_pos_weight),
        "--value-loss-start-weight",
        str(args.value_loss_start_weight),
        "--fan-loss-start-weight",
        str(args.fan_loss_start_weight),
        "--qualifying-fan-loss-start-weight",
        str(args.qualifying_fan_loss_start_weight),
        "--risk-loss-start-weight",
        str(args.risk_loss_start_weight),
        "--aux-loss-warmup-epochs",
        str(args.aux_loss_warmup_epochs),
        "--claim-rare-action-weight",
        str(args.claim_rare_action_weight),
        "--self-kong-rare-action-weight",
        str(args.self_kong_rare_action_weight),
        "--hu-positive-weight",
        str(args.hu_positive_weight),
        "--grad-clip-norm",
        str(args.grad_clip_norm),
        "--max-nan-tolerance",
        str(args.max_nan_tolerance),
        "--early-stop-patience",
        str(args.early_stop_patience),
    ]
    if args.amp is False:
        command.append("--no-amp")
    elif args.amp is True:
        command.append("--amp")
    if args.no_tf32:
        command.append("--no-tf32")
    if args.compile_model:
        command.append("--compile")
    if args.rebuild_data_cache:
        command.append("--rebuild-data-cache")
    run_command(command, root, env)


def export_onnx(args: argparse.Namespace, root: Path, env: dict[str, str]) -> None:
    if args.skip_onnx_export:
        return
    run_command(
        [
            args.python_exe,
            "backend/bot_trainer/v2/sft/export_onnx.py",
            "--checkpoint",
            str(Path(args.checkpoint_dir) / "best.pt"),
            "--output",
            args.onnx_output,
        ],
        root,
        env,
    )
    smoke_env = env.copy()
    smoke_env["MAHJONG_BOT_MODEL_PATH"] = str((root / args.onnx_output).resolve())
    run_command(
        [
            "cargo",
            "test",
            "--manifest-path",
            "backend/Cargo.toml",
            "bot::neural::tests::runs_local_onnx_model_when_available",
            "--",
            "--nocapture",
        ],
        root,
        smoke_env,
    )


def main() -> None:
    args = parse_args()
    if args.input_format == "datasets2" and args.input == DEFAULT_INPUT_PATH:
        args.input = DEFAULT_DATASETS2_INPUT_PATH
    root = repo_root()
    prompt_pipeline(args, root)

    env = os.environ.copy()
    env["PYTHONUTF8"] = "1"
    env["PYTHONIOENCODING"] = "utf-8"
    temp_dir = root / ".tmp" / "bot-trainer-v2-sft"
    temp_dir.mkdir(parents=True, exist_ok=True)
    env["TEMP"] = str(temp_dir)
    env["TMP"] = str(temp_dir)
    env["PYTEST_DEBUG_TEMPROOT"] = str(temp_dir)

    data_dir = (root / args.data_dir).resolve()
    if args.yes and dataset_status(data_dir).complete:
        print(f"Existing dataset found at {data_dir}; skipping export.")
        args.skip_export_dataset = True

    if not args.skip_export_dataset:
        export_dataset(args, root)
    assert_dataset_contract(data_dir)
    run_tests(args, root, env)
    train_model(args, root, env)
    export_onnx(args, root, env)


if __name__ == "__main__":
    main()
