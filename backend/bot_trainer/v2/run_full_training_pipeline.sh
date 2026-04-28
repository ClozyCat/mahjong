#!/usr/bin/env bash
set -euo pipefail

DATASETS_ZIP="backend/bot_trainer/datasets.zip"
DATASETS_DIR="backend/bot_trainer/datasets"
INPUT_PATH="backend/bot_trainer/datasets/data.txt"
EXPORT_OUTPUT="backend/bot_trainer/v2/out"
CHECKPOINT_DIR="backend/bot_trainer/v2/checkpoints"
ONNX_OUTPUT="backend/assets/models/mahjong_policy_net.onnx"
PROGRESS_EVERY=1000
MAX_MATCHES=0
EPOCHS=20
BATCH_SIZE=4096
NUM_WORKERS=0
PYTHON_EXE=""
LEARNING_RATE=0.001
WEIGHT_DECAY=0.0001
CLAIM_LOSS_WEIGHT=1.0
SELF_KONG_LOSS_WEIGHT=1.0
HU_LOSS_WEIGHT=1.0
VALUE_LOSS_WEIGHT=0.25
RISK_LOSS_WEIGHT=0.25
FORCE_UNZIP=0
SKIP_UNZIP=0
NO_AMP=0
COMPILE_MODEL=0
SKIP_TESTS=0
SKIP_ONNX_EXPORT=0

usage() {
    cat <<'EOF'
Usage: run_full_training_pipeline.sh [options]

Ubuntu/Debian pipeline:
  1. Unzip datasets.zip into backend/bot_trainer/datasets.
  2. Export v2 JSONL samples.
  3. Train and export the ONNX model with CUDA only.

Options:
  --datasets-zip PATH       Source zip. Defaults to backend/bot_trainer/datasets.zip.
  --datasets-dir DIR        Directory to unzip into. Defaults to backend/bot_trainer/datasets.
  --input PATH              Exporter input file. Defaults to backend/bot_trainer/datasets/data.txt.
  --export-output DIR       Exported JSONL output directory.
  --checkpoint-dir DIR      Training checkpoint directory.
  --onnx-output PATH        Output ONNX model path.
  --progress-every N        Export progress interval.
  --max-matches N           Export at most N matches; 0 exports all.
  --epochs N                Training epochs.
  --batch-size N            Training batch size.
  --num-workers N           PyTorch DataLoader worker count.
  --python-exe PATH         Python executable passed to train_and_export_model.sh.
  --lr VALUE                Training learning rate.
  --weight-decay VALUE      Training weight decay.
  --claim-loss-weight VALUE Claim head loss weight.
  --self-kong-loss-weight VALUE
                            Self-kong head loss weight.
  --hu-loss-weight VALUE    Hu head loss weight.
  --value-loss-weight VALUE Value head loss weight.
  --risk-loss-weight VALUE  Risk head loss weight.
  --force-unzip             Unzip even when the input file already exists.
  --skip-unzip              Do not unzip datasets.zip.
  --no-amp                  Disable AMP in training.
  --compile                 Enable torch.compile in training.
  --skip-tests              Skip Python tests before training.
  --skip-onnx-export        Skip ONNX export and Rust ONNX smoke test.
  -h, --help                Show this help.
EOF
}

require_value() {
    local option="$1"
    local value="${2:-}"
    if [[ -z "$value" || "$value" == --* ]]; then
        echo "Missing value for $option" >&2
        exit 2
    fi
}

require_command() {
    local command_name="$1"
    local install_hint="$2"
    if ! command -v "$command_name" >/dev/null 2>&1; then
        echo "Missing required command: $command_name" >&2
        echo "$install_hint" >&2
        exit 3
    fi
}

require_ubuntu_or_debian() {
    if [[ ! -r /etc/os-release ]]; then
        echo "This script supports Ubuntu/Debian systems and requires /etc/os-release." >&2
        exit 3
    fi

    # shellcheck disable=SC1091
    source /etc/os-release
    local os_id="${ID:-}"
    local os_like="${ID_LIKE:-}"
    if [[ "$os_id" != "ubuntu" && "$os_id" != "debian" && "$os_like" != *"debian"* ]]; then
        echo "This script supports Ubuntu/Debian systems only. Detected ID=$os_id ID_LIKE=$os_like" >&2
        exit 3
    fi
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --datasets-zip)
            require_value "$1" "${2:-}"
            DATASETS_ZIP="$2"
            shift 2
            ;;
        --datasets-dir)
            require_value "$1" "${2:-}"
            DATASETS_DIR="$2"
            shift 2
            ;;
        --input)
            require_value "$1" "${2:-}"
            INPUT_PATH="$2"
            shift 2
            ;;
        --export-output)
            require_value "$1" "${2:-}"
            EXPORT_OUTPUT="$2"
            shift 2
            ;;
        --checkpoint-dir)
            require_value "$1" "${2:-}"
            CHECKPOINT_DIR="$2"
            shift 2
            ;;
        --onnx-output)
            require_value "$1" "${2:-}"
            ONNX_OUTPUT="$2"
            shift 2
            ;;
        --progress-every)
            require_value "$1" "${2:-}"
            PROGRESS_EVERY="$2"
            shift 2
            ;;
        --max-matches)
            require_value "$1" "${2:-}"
            MAX_MATCHES="$2"
            shift 2
            ;;
        --epochs)
            require_value "$1" "${2:-}"
            EPOCHS="$2"
            shift 2
            ;;
        --batch-size)
            require_value "$1" "${2:-}"
            BATCH_SIZE="$2"
            shift 2
            ;;
        --num-workers)
            require_value "$1" "${2:-}"
            NUM_WORKERS="$2"
            shift 2
            ;;
        --python-exe)
            require_value "$1" "${2:-}"
            PYTHON_EXE="$2"
            shift 2
            ;;
        --lr)
            require_value "$1" "${2:-}"
            LEARNING_RATE="$2"
            shift 2
            ;;
        --weight-decay)
            require_value "$1" "${2:-}"
            WEIGHT_DECAY="$2"
            shift 2
            ;;
        --claim-loss-weight)
            require_value "$1" "${2:-}"
            CLAIM_LOSS_WEIGHT="$2"
            shift 2
            ;;
        --self-kong-loss-weight)
            require_value "$1" "${2:-}"
            SELF_KONG_LOSS_WEIGHT="$2"
            shift 2
            ;;
        --hu-loss-weight)
            require_value "$1" "${2:-}"
            HU_LOSS_WEIGHT="$2"
            shift 2
            ;;
        --value-loss-weight)
            require_value "$1" "${2:-}"
            VALUE_LOSS_WEIGHT="$2"
            shift 2
            ;;
        --risk-loss-weight)
            require_value "$1" "${2:-}"
            RISK_LOSS_WEIGHT="$2"
            shift 2
            ;;
        --force-unzip)
            FORCE_UNZIP=1
            shift
            ;;
        --skip-unzip)
            SKIP_UNZIP=1
            shift
            ;;
        --no-amp)
            NO_AMP=1
            shift
            ;;
        --compile)
            COMPILE_MODEL=1
            shift
            ;;
        --skip-tests)
            SKIP_TESTS=1
            shift
            ;;
        --skip-onnx-export)
            SKIP_ONNX_EXPORT=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../../.." && pwd)"
EXPORT_SCRIPT="backend/bot_trainer/v2/export_full_dataset.sh"
TRAIN_SCRIPT="backend/bot_trainer/v2/train_and_export_model.sh"

cd "$REPO_ROOT"

require_ubuntu_or_debian
require_command unzip "Install it with: sudo apt-get update && sudo apt-get install -y unzip"
require_command cargo "Install Rust/Cargo first: https://rustup.rs/"
require_command nvidia-smi "Install the NVIDIA driver and CUDA runtime so nvidia-smi is available."

if [[ ! -x "$EXPORT_SCRIPT" ]]; then
    chmod +x "$EXPORT_SCRIPT"
fi
if [[ ! -x "$TRAIN_SCRIPT" ]]; then
    chmod +x "$TRAIN_SCRIPT"
fi

echo "Mahjong bot v2 full training pipeline"
echo "Repo:          $REPO_ROOT"
echo "Datasets zip:  $DATASETS_ZIP"
echo "Datasets dir:  $DATASETS_DIR"
echo "Input:         $INPUT_PATH"
echo "Export output: $EXPORT_OUTPUT"
echo "Checkpoints:   $CHECKPOINT_DIR"
echo "ONNX output:   $ONNX_OUTPUT"
echo "Epochs:        $EPOCHS"
echo "Batch size:    $BATCH_SIZE"

if (( SKIP_UNZIP == 0 )); then
    if [[ ! -f "$DATASETS_ZIP" ]]; then
        echo "datasets.zip was not found: $DATASETS_ZIP" >&2
        exit 3
    fi

    mkdir -p "$DATASETS_DIR"
    if [[ ! -f "$INPUT_PATH" || "$FORCE_UNZIP" -eq 1 ]]; then
        echo "Unzipping datasets.zip into $DATASETS_DIR"
        unzip -o "$DATASETS_ZIP" -d "$DATASETS_DIR"
    else
        echo "Dataset input already exists; skipping unzip. Use --force-unzip to overwrite."
    fi
fi

if [[ ! -f "$INPUT_PATH" ]]; then
    echo "Dataset input file was not found after unzip: $INPUT_PATH" >&2
    exit 3
fi

export_args=(
    --input "$INPUT_PATH"
    --output "$EXPORT_OUTPUT"
    --progress-every "$PROGRESS_EVERY"
    --max-matches "$MAX_MATCHES"
)

train_args=(
    --data-dir "$EXPORT_OUTPUT"
    --checkpoint-dir "$CHECKPOINT_DIR"
    --onnx-output "$ONNX_OUTPUT"
    --epochs "$EPOCHS"
    --batch-size "$BATCH_SIZE"
    --num-workers "$NUM_WORKERS"
    --lr "$LEARNING_RATE"
    --weight-decay "$WEIGHT_DECAY"
    --claim-loss-weight "$CLAIM_LOSS_WEIGHT"
    --self-kong-loss-weight "$SELF_KONG_LOSS_WEIGHT"
    --hu-loss-weight "$HU_LOSS_WEIGHT"
    --value-loss-weight "$VALUE_LOSS_WEIGHT"
    --risk-loss-weight "$RISK_LOSS_WEIGHT"
)

if [[ -n "$PYTHON_EXE" ]]; then
    train_args+=(--python-exe "$PYTHON_EXE")
fi
if (( NO_AMP == 1 )); then
    train_args+=(--no-amp)
fi
if (( COMPILE_MODEL == 1 )); then
    train_args+=(--compile)
fi
if (( SKIP_TESTS == 1 )); then
    train_args+=(--skip-tests)
fi
if (( SKIP_ONNX_EXPORT == 1 )); then
    train_args+=(--skip-onnx-export)
fi

echo "Exporting JSONL dataset"
"$EXPORT_SCRIPT" "${export_args[@]}"

echo "Training and exporting model"
"$TRAIN_SCRIPT" "${train_args[@]}"
