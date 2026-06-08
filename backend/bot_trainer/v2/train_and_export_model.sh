#!/usr/bin/env bash
set -euo pipefail

DATA_DIR="backend/bot_trainer/v2/out"
CHECKPOINT_DIR="backend/bot_trainer/v2/checkpoints"
ONNX_OUTPUT="backend/assets/sft/sft.onnx"
EPOCHS=20
BATCH_SIZE=4096
NUM_WORKERS=0
PYTHON_CMD=(python)
LEARNING_RATE=0.001
WEIGHT_DECAY=0.0001
CLAIM_LOSS_WEIGHT=1.0
SELF_KONG_LOSS_WEIGHT=1.0
HU_LOSS_WEIGHT=1.0
VALUE_LOSS_WEIGHT=0.75
RISK_LOSS_WEIGHT=1.0
GRAD_CLIP_NORM=1.0
MAX_NAN_TOLERANCE=2
EARLY_STOP_PATIENCE=0
DEVICE="cuda"
NO_AMP=0
COMPILE_MODEL=0
SKIP_TESTS=0
SKIP_ONNX_EXPORT=0

usage() {
    cat <<'EOF'
Usage: train_and_export_model.sh [options]

Options:
  --data-dir DIR            Training data directory.
  --checkpoint-dir DIR      Directory for training checkpoints.
  --onnx-output PATH        Output ONNX model path.
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
  --risk-loss-weight VALUE  Risk head loss weight.
  --grad-clip-norm VALUE    Gradient clipping norm (0 to disable).
  --max-nan-tolerance N     Max consecutive NaN epochs before stopping.
  --early-stop-patience N   Early stopping patience (0 to disable).
  --no-amp                  Do not pass --amp to train.py.
  --compile                 Pass --compile to train.py.
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
        --risk-loss-weight)
            require_value "$1" "${2:-}"
            RISK_LOSS_WEIGHT="$2"
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

cd "$REPO_ROOT"

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
echo "Grad clip:   $GRAD_CLIP_NORM"
echo "NaN tolerance: $MAX_NAN_TOLERANCE"
echo "Early stop:  $EARLY_STOP_PATIENCE"
echo "Python:      ${PYTHON_CMD[*]}"

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
    --lr "$LEARNING_RATE"
    --weight-decay "$WEIGHT_DECAY"
    --claim-loss-weight "$CLAIM_LOSS_WEIGHT"
    --self-kong-loss-weight "$SELF_KONG_LOSS_WEIGHT"
    --hu-loss-weight "$HU_LOSS_WEIGHT"
    --value-loss-weight "$VALUE_LOSS_WEIGHT"
    --risk-loss-weight "$RISK_LOSS_WEIGHT"
    --grad-clip-norm "$GRAD_CLIP_NORM"
    --max-nan-tolerance "$MAX_NAN_TOLERANCE"
    --early-stop-patience "$EARLY_STOP_PATIENCE"
)

if (( NO_AMP == 0 )); then
    train_args+=(--amp)
fi

if (( COMPILE_MODEL == 1 )); then
    train_args+=(--compile)
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
