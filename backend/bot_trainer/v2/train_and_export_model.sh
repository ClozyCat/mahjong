#!/usr/bin/env bash
set -euo pipefail

DATA_DIR="backend/bot_trainer/v2/out"
CHECKPOINT_DIR="backend/bot_trainer/v2/checkpoints"
ONNX_OUTPUT="backend/assets/sft/sft.onnx"
DATA_CACHE_DIR=""
EPOCHS=20
BATCH_SIZE=4096
NUM_WORKERS=0
PYTHON_CMD=(python)
LEARNING_RATE=0.0003
WEIGHT_DECAY=0.0001
CLAIM_LOSS_WEIGHT=1.0
SELF_KONG_LOSS_WEIGHT=1.0
HU_LOSS_WEIGHT=1.0
VALUE_LOSS_WEIGHT=0.75
FAN_LOSS_WEIGHT=0.5
QUALIFYING_FAN_LOSS_WEIGHT=0.75
RISK_LOSS_WEIGHT=1.0
RISK_POS_WEIGHT=300.0
VALUE_LOSS_START_WEIGHT=0.25
FAN_LOSS_START_WEIGHT=0.1
QUALIFYING_FAN_LOSS_START_WEIGHT=0.1
RISK_LOSS_START_WEIGHT=0.25
AUX_LOSS_WARMUP_EPOCHS=4
CLAIM_RARE_ACTION_WEIGHT=2.0
SELF_KONG_RARE_ACTION_WEIGHT=3.0
HU_POSITIVE_WEIGHT=3.0
GRAD_CLIP_NORM=1.0
MAX_NAN_TOLERANCE=2
EARLY_STOP_PATIENCE=0
DEVICE="cuda"
USE_AMP=0
USE_TF32=1
COMPILE_MODEL=0
REBUILD_DATA_CACHE=0
SKIP_TESTS=0
SKIP_ONNX_EXPORT=0
EXPECTED_METADATA_SCHEMA_VERSION=4

usage() {
    cat <<'EOF'
Usage: train_and_export_model.sh [options]

Options:
  --data-dir DIR            Training data directory.
  --checkpoint-dir DIR      Directory for training checkpoints.
  --onnx-output PATH        Output ONNX model path.
  --data-cache-dir DIR      Tensor cache directory. Defaults to DATA_DIR/.tensor_cache.
  --epochs N                Number of training epochs.
  --batch-size N            Training batch size.
  --num-workers N           DataLoader worker count.
  --python-exe PATH         Python executable override. Defaults to python.
  --device NAME             Training device: auto, cuda, cpu, or dml. Defaults to cuda.
  --lr VALUE                Learning rate.
  --weight-decay VALUE      Weight decay.
  --claim-loss-weight VALUE Claim head loss weight.
  --self-kong-loss-weight VALUE
                            Self-kong head loss weight.
  --hu-loss-weight VALUE    Hu head loss weight.
  --value-loss-weight VALUE Value head loss weight.
  --fan-loss-weight VALUE   Fan auxiliary loss weight.
  --qualifying-fan-loss-weight VALUE
                            Qualifying fan auxiliary loss weight.
  --risk-loss-weight VALUE  Risk head loss weight.
  --risk-pos-weight VALUE   Positive class weight for masked risk BCE.
  --value-loss-start-weight VALUE
                            Initial value loss weight during warmup.
  --fan-loss-start-weight VALUE
                            Initial fan loss weight during warmup.
  --qualifying-fan-loss-start-weight VALUE
                            Initial qualifying fan loss weight during warmup.
  --risk-loss-start-weight VALUE
                            Initial risk loss weight during warmup.
  --aux-loss-warmup-epochs N
                            Epochs used to warm up auxiliary loss weights.
  --claim-rare-action-weight VALUE
                            Weight for non-pass claim labels.
  --self-kong-rare-action-weight VALUE
                            Weight for non-pass self-kong labels.
  --hu-positive-weight VALUE
                            Weight for positive hu labels.
  --grad-clip-norm VALUE    Gradient clipping norm (0 to disable).
  --max-nan-tolerance N     Max consecutive NaN epochs before stopping.
  --early-stop-patience N   Early stopping patience (0 to disable).
  --amp                     Enable mixed precision training.
  --no-tf32                 Disable CUDA TF32 acceleration for float32 training.
  --compile                 Pass --compile to train.py.
  --rebuild-data-cache      Rebuild tensor cache before training.
  --skip-tests              Skip pytest before training.
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

while [[ $# -gt 0 ]]; do
    case "$1" in
        --data-dir)
            require_value "$1" "${2:-}"
            DATA_DIR="$2"
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
        --data-cache-dir)
            require_value "$1" "${2:-}"
            DATA_CACHE_DIR="$2"
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
            PYTHON_CMD=("$2")
            shift 2
            ;;
        --device)
            require_value "$1" "${2:-}"
            DEVICE="$2"
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
        --fan-loss-weight)
            require_value "$1" "${2:-}"
            FAN_LOSS_WEIGHT="$2"
            shift 2
            ;;
        --qualifying-fan-loss-weight)
            require_value "$1" "${2:-}"
            QUALIFYING_FAN_LOSS_WEIGHT="$2"
            shift 2
            ;;
        --risk-loss-weight)
            require_value "$1" "${2:-}"
            RISK_LOSS_WEIGHT="$2"
            shift 2
            ;;
        --risk-pos-weight)
            require_value "$1" "${2:-}"
            RISK_POS_WEIGHT="$2"
            shift 2
            ;;
        --value-loss-start-weight)
            require_value "$1" "${2:-}"
            VALUE_LOSS_START_WEIGHT="$2"
            shift 2
            ;;
        --fan-loss-start-weight)
            require_value "$1" "${2:-}"
            FAN_LOSS_START_WEIGHT="$2"
            shift 2
            ;;
        --qualifying-fan-loss-start-weight)
            require_value "$1" "${2:-}"
            QUALIFYING_FAN_LOSS_START_WEIGHT="$2"
            shift 2
            ;;
        --risk-loss-start-weight)
            require_value "$1" "${2:-}"
            RISK_LOSS_START_WEIGHT="$2"
            shift 2
            ;;
        --aux-loss-warmup-epochs)
            require_value "$1" "${2:-}"
            AUX_LOSS_WARMUP_EPOCHS="$2"
            shift 2
            ;;
        --claim-rare-action-weight)
            require_value "$1" "${2:-}"
            CLAIM_RARE_ACTION_WEIGHT="$2"
            shift 2
            ;;
        --self-kong-rare-action-weight)
            require_value "$1" "${2:-}"
            SELF_KONG_RARE_ACTION_WEIGHT="$2"
            shift 2
            ;;
        --hu-positive-weight)
            require_value "$1" "${2:-}"
            HU_POSITIVE_WEIGHT="$2"
            shift 2
            ;;
        --grad-clip-norm)
            require_value "$1" "${2:-}"
            GRAD_CLIP_NORM="$2"
            shift 2
            ;;
        --max-nan-tolerance)
            require_value "$1" "${2:-}"
            MAX_NAN_TOLERANCE="$2"
            shift 2
            ;;
        --early-stop-patience)
            require_value "$1" "${2:-}"
            EARLY_STOP_PATIENCE="$2"
            shift 2
            ;;
        --amp)
            USE_AMP=1
            shift
            ;;
        --no-tf32)
            USE_TF32=0
            shift
            ;;
        --compile)
            COMPILE_MODEL=1
            shift
            ;;
        --rebuild-data-cache)
            REBUILD_DATA_CACHE=1
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

cd "$REPO_ROOT"

if [[ -n "$DATA_CACHE_DIR" ]]; then
    RESOLVED_DATA_CACHE_DIR="$DATA_CACHE_DIR"
else
    RESOLVED_DATA_CACHE_DIR="$DATA_DIR/.tensor_cache"
fi

export PYTHONUTF8=1
export PYTHONIOENCODING=utf-8
TEMP_DIR="$REPO_ROOT/.tmp/bot-trainer-v2-sft"
mkdir -p "$TEMP_DIR"
export TMPDIR="$TEMP_DIR"
export PYTEST_DEBUG_TEMPROOT="$TMPDIR"

require_cuda_gpu() {
    if ! command -v nvidia-smi >/dev/null 2>&1; then
        echo "CUDA GPU is required, but nvidia-smi was not found." >&2
        exit 3
    fi
    if ! nvidia-smi --query-gpu=name --format=csv,noheader >/dev/null; then
        echo "CUDA GPU is required, but nvidia-smi could not detect an NVIDIA GPU." >&2
        exit 3
    fi
}

CUDA_DEVICE=""
if [[ "$DEVICE" == "cuda" ]]; then
    require_cuda_gpu
    probe_output="$(
        "${PYTHON_CMD[@]}" - <<'PY' 2>&1
import sys

try:
    import torch
except ModuleNotFoundError as exc:
    print('PyTorch is required: pip install torch', file=sys.stderr)
    raise SystemExit(2) from exc

if getattr(torch.version, 'hip', None):
    print('CUDA GPU is required, but this PyTorch build is ROCm/HIP.', file=sys.stderr)
    raise SystemExit(3)

if not getattr(torch.version, 'cuda', None):
    print('CUDA GPU is required, but this PyTorch build has no CUDA runtime.', file=sys.stderr)
    raise SystemExit(3)

if not torch.cuda.is_available():
    print('CUDA GPU is required, but torch.cuda.is_available() is False.', file=sys.stderr)
    raise SystemExit(3)

print('CUDA_DEVICE=' + torch.cuda.get_device_name(0))
PY
    )" || {
        status=$?
        printf '%s\n' "$probe_output" >&2
        echo "CUDA preflight failed before training." >&2
        exit "$status"
    }

    CUDA_DEVICE="$(printf '%s\n' "$probe_output" | awk -F= '/^CUDA_DEVICE=/{value=$2} END{print value}')"
    if [[ -z "$CUDA_DEVICE" ]]; then
        printf '%s\n' "$probe_output" >&2
        echo "Failed to verify CUDA device from Python probe." >&2
        exit 3
    fi
fi

check_dataset_contract() {
    "${PYTHON_CMD[@]}" - "$DATA_DIR" "$RESOLVED_DATA_CACHE_DIR" "$EXPECTED_METADATA_SCHEMA_VERSION" <<'PY'
import json
import sys
from pathlib import Path

data_dir = Path(sys.argv[1])
cache_dir = Path(sys.argv[2])
expected = int(sys.argv[3])
metadata_path = data_dir / "metadata.json"

if not metadata_path.is_file():
    print(f"Dataset metadata not found: {metadata_path}", file=sys.stderr)
    print(
        f"Run: ./backend/bot_trainer/v2/export_full_dataset.sh --output {data_dir}",
        file=sys.stderr,
    )
    raise SystemExit(2)

try:
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
except json.JSONDecodeError as exc:
    print(f"Dataset metadata is not valid JSON: {metadata_path}", file=sys.stderr)
    print(str(exc), file=sys.stderr)
    raise SystemExit(2) from exc

schema_version = metadata.get("schema_version")
if schema_version != expected:
    print(
        f"Unsupported dataset schema: {schema_version}; expected {expected}.",
        file=sys.stderr,
    )
    print(
        f"Re-export data before training: "
        f"./backend/bot_trainer/v2/export_full_dataset.sh --output {data_dir}",
        file=sys.stderr,
    )
    print(f"Then remove the tensor cache: rm -rf {cache_dir}", file=sys.stderr)
    raise SystemExit(2)

missing = [
    str(data_dir / name)
    for name in ("train.jsonl", "val.jsonl", "test.jsonl")
    if not (data_dir / name).is_file()
]
if missing:
    print("Dataset split files are missing:", file=sys.stderr)
    for path in missing:
        print(f"  {path}", file=sys.stderr)
    print(
        f"Run: ./backend/bot_trainer/v2/export_full_dataset.sh --output {data_dir}",
        file=sys.stderr,
    )
    raise SystemExit(2)
PY
}

echo "Training Mahjong bot v2 model"
echo "Data:        $DATA_DIR"
echo "Checkpoints: $CHECKPOINT_DIR"
echo "Device:      $DEVICE"
if [[ "$DEVICE" == "cuda" ]]; then
    echo "CUDA GPU:    $CUDA_DEVICE"
fi
echo "Epochs:      $EPOCHS"
echo "Batch size:  $BATCH_SIZE"
echo "Workers:     $NUM_WORKERS"
echo "Data cache:  $RESOLVED_DATA_CACHE_DIR"
echo "Aux weights: value=$VALUE_LOSS_WEIGHT fan=$FAN_LOSS_WEIGHT qualifying_fan=$QUALIFYING_FAN_LOSS_WEIGHT risk=$RISK_LOSS_WEIGHT risk_pos=$RISK_POS_WEIGHT"
echo "Rare weights: claim=$CLAIM_RARE_ACTION_WEIGHT self_kong=$SELF_KONG_RARE_ACTION_WEIGHT hu=$HU_POSITIVE_WEIGHT"
echo "Grad clip:   $GRAD_CLIP_NORM"
echo "NaN tolerance: $MAX_NAN_TOLERANCE"
echo "Early stop:  $EARLY_STOP_PATIENCE"
echo "Python:      ${PYTHON_CMD[*]}"

check_dataset_contract

if (( SKIP_TESTS == 0 )); then
    if "${PYTHON_CMD[@]}" -c "import importlib.util, sys; sys.exit(0 if importlib.util.find_spec('pytest') else 2)"; then
        "${PYTHON_CMD[@]}" -m pytest backend/bot_trainer/v2 -q --basetemp "$TEMP_DIR/pytest"
    else
        echo "pytest is not installed for this Python; skipping Python tests. Use --skip-tests to silence this check."
    fi
fi

train_args=(
    backend/bot_trainer/v2/train.py
    --data "$DATA_DIR"
    --epochs "$EPOCHS"
    --batch-size "$BATCH_SIZE"
    --output "$CHECKPOINT_DIR"
    --device "$DEVICE"
    --num-workers "$NUM_WORKERS"
    --data-cache-dir "$RESOLVED_DATA_CACHE_DIR"
    --lr "$LEARNING_RATE"
    --weight-decay "$WEIGHT_DECAY"
    --claim-loss-weight "$CLAIM_LOSS_WEIGHT"
    --self-kong-loss-weight "$SELF_KONG_LOSS_WEIGHT"
    --hu-loss-weight "$HU_LOSS_WEIGHT"
    --value-loss-weight "$VALUE_LOSS_WEIGHT"
    --fan-loss-weight "$FAN_LOSS_WEIGHT"
    --qualifying-fan-loss-weight "$QUALIFYING_FAN_LOSS_WEIGHT"
    --risk-loss-weight "$RISK_LOSS_WEIGHT"
    --risk-pos-weight "$RISK_POS_WEIGHT"
    --value-loss-start-weight "$VALUE_LOSS_START_WEIGHT"
    --fan-loss-start-weight "$FAN_LOSS_START_WEIGHT"
    --qualifying-fan-loss-start-weight "$QUALIFYING_FAN_LOSS_START_WEIGHT"
    --risk-loss-start-weight "$RISK_LOSS_START_WEIGHT"
    --aux-loss-warmup-epochs "$AUX_LOSS_WARMUP_EPOCHS"
    --claim-rare-action-weight "$CLAIM_RARE_ACTION_WEIGHT"
    --self-kong-rare-action-weight "$SELF_KONG_RARE_ACTION_WEIGHT"
    --hu-positive-weight "$HU_POSITIVE_WEIGHT"
    --grad-clip-norm "$GRAD_CLIP_NORM"
    --max-nan-tolerance "$MAX_NAN_TOLERANCE"
    --early-stop-patience "$EARLY_STOP_PATIENCE"
)

if (( USE_AMP == 1 )); then
    train_args+=(--amp)
fi
if (( USE_TF32 == 0 )); then
    train_args+=(--no-tf32)
fi

if (( COMPILE_MODEL == 1 )); then
    train_args+=(--compile)
fi

if (( REBUILD_DATA_CACHE == 1 )); then
    train_args+=(--rebuild-data-cache)
fi

"${PYTHON_CMD[@]}" "${train_args[@]}"

if (( SKIP_ONNX_EXPORT == 0 )); then
    "${PYTHON_CMD[@]}" backend/bot_trainer/v2/export_onnx.py \
        --checkpoint "$CHECKPOINT_DIR/best.pt" \
        --output "$ONNX_OUTPUT"

    cargo test \
        --manifest-path backend/Cargo.toml \
        bot::neural::tests::runs_local_onnx_model_when_available \
        -- --nocapture
fi
