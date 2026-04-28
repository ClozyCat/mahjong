#!/usr/bin/env bash
set -euo pipefail

DATA_DIR="backend/bot_trainer/v2/out"
CHECKPOINT_DIR="backend/bot_trainer/v2/checkpoints"
ONNX_OUTPUT="backend/assets/models/mahjong_policy_net.onnx"
EPOCHS=20
BATCH_SIZE=4096
DEVICE="rocm"
NUM_WORKERS=0
UV_EXE="${UV:-uv}"
PYTHON_CMD=("$UV_EXE" run python)
LEARNING_RATE=0.001
WEIGHT_DECAY=0.0001
ROCM_GFX_OVERRIDE="${HSA_OVERRIDE_GFX_VERSION:-10.3.0}"
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
  --device NAME             Training device: rocm, amd, hip, auto, cuda.
  --num-workers N           DataLoader worker count.
  --python-exe PATH         Python executable override. Defaults to "$UV run python".
  --lr VALUE                Learning rate.
  --weight-decay VALUE      Weight decay.
  --rocm-gfx-override VALUE HSA_OVERRIDE_GFX_VERSION for ROCm. Defaults to 10.3.0 for RX 6800.
  --no-rocm-gfx-override    Do not set HSA_OVERRIDE_GFX_VERSION.
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
        --device)
            require_value "$1" "${2:-}"
            DEVICE="$2"
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
        --rocm-gfx-override)
            require_value "$1" "${2:-}"
            ROCM_GFX_OVERRIDE="$2"
            shift 2
            ;;
        --no-rocm-gfx-override)
            ROCM_GFX_OVERRIDE=""
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

cd "$REPO_ROOT"

export PYTHONUTF8=1
export PYTHONIOENCODING=utf-8
export PYTHONFAULTHANDLER=1
export AMD_LOG_LEVEL="${AMD_LOG_LEVEL:-0}"

case "${DEVICE,,}" in
    auto|gpu|rocm|hip|amd)
        if [[ -n "$ROCM_GFX_OVERRIDE" && -z "${HSA_OVERRIDE_GFX_VERSION:-}" ]]; then
            export HSA_OVERRIDE_GFX_VERSION="$ROCM_GFX_OVERRIDE"
        fi
        ;;
esac

probe_output="$(
    "${PYTHON_CMD[@]}" - <<'PY' "$DEVICE" 2>&1
import sys

try:
    import torch
except ModuleNotFoundError as exc:
    print('PyTorch is required: pip install torch', file=sys.stderr)
    raise SystemExit(2) from exc

requested = sys.argv[1].strip().lower()

def fail(message: str, code: int = 3) -> None:
    print(message, file=sys.stderr)
    raise SystemExit(code)

def cuda_available() -> bool:
    return bool(torch.cuda.is_available())

def rocm_available() -> bool:
    return cuda_available() and bool(getattr(torch.version, 'hip', None))

def nvidia_cuda_available() -> bool:
    return cuda_available() and bool(getattr(torch.version, 'cuda', None)) and not bool(getattr(torch.version, 'hip', None))

def rocm_failure_message() -> str:
    return (
        'Requested ROCm/HIP, but this Python environment does not expose a ROCm PyTorch backend. '
        'If torch.cuda.is_available() crashes, the failure is in ROCm/HIP runtime initialization. '
        'HIP SDK alone is not enough; use a ROCm-enabled PyTorch build and a supported GPU/OS combination.'
    )

def resolve_backend_and_device() -> tuple[str, str]:
    if requested in ('auto', 'gpu'):
        if rocm_available():
            return 'rocm', 'cuda'
        if nvidia_cuda_available():
            return 'cuda', 'cuda'
        fail(
            'No supported GPU backend is available. CPU fallback is disabled. '
            'For AMD ROCm, install a ROCm-enabled PyTorch build; for NVIDIA CUDA, install a CUDA-enabled PyTorch build.'
        )

    if requested in ('rocm', 'hip', 'amd'):
        if rocm_available():
            return 'rocm', 'cuda'
        fail(rocm_failure_message())

    if requested in ('cuda', 'cu', 'nvidia'):
        if nvidia_cuda_available():
            return 'cuda', 'cuda'
        if rocm_available():
            fail('Requested NVIDIA CUDA, but this PyTorch build is ROCm/HIP. Use --device rocm or --device auto.')
        fail('Requested NVIDIA CUDA, but torch.cuda.is_available() is False or torch.version.cuda is empty.')

    if requested in ('dml', 'directml'):
        fail('DirectML is disabled for this script. Use --device rocm with a ROCm-enabled PyTorch build.')

    if requested == 'cpu':
        fail('CPU training is disabled for this script. Install a supported GPU backend instead.')

    try:
        device = torch.device(requested)
    except Exception as exc:
        fail('Unsupported device ' + repr(requested) + ': ' + str(exc), 2)

    if device.type == 'cpu':
        fail('Training device resolved to CPU. CPU fallback is disabled.')
    if device.type == 'cuda':
        if rocm_available():
            return 'rocm', 'cuda'
        if nvidia_cuda_available():
            return 'cuda', 'cuda'
        fail('Requested cuda device, but no CUDA/ROCm backend is available.')
    return device.type, requested

backend, device = resolve_backend_and_device()
print('RESOLVED_BACKEND=' + backend)
print('RESOLVED_DEVICE=' + device)
PY
)" || {
    status=$?
    printf '%s\n' "$probe_output" >&2
    echo "GPU preflight failed before training. If this shows a Windows fatal exception or access violation, ROCm/PyTorch crashed while initializing HIP." >&2
    exit "$status"
}

RESOLVED_BACKEND="$(printf '%s\n' "$probe_output" | awk -F= '/^RESOLVED_BACKEND=/{value=$2} END{print value}')"
RESOLVED_DEVICE="$(printf '%s\n' "$probe_output" | awk -F= '/^RESOLVED_DEVICE=/{value=$2} END{print value}')"
if [[ -z "$RESOLVED_BACKEND" || -z "$RESOLVED_DEVICE" ]]; then
    printf '%s\n' "$probe_output" >&2
    echo "Failed to resolve training device from Python probe." >&2
    exit 3
fi

echo "Training Mahjong bot v2 model"
echo "Data:        $DATA_DIR"
echo "Checkpoints: $CHECKPOINT_DIR"
echo "Backend:     $RESOLVED_BACKEND"
echo "Device:      $RESOLVED_DEVICE (requested: $DEVICE)"
echo "Epochs:      $EPOCHS"
echo "Batch size:  $BATCH_SIZE"
echo "Workers:     $NUM_WORKERS"
echo "Python:      ${PYTHON_CMD[*]}"
if [[ "$RESOLVED_BACKEND" == "rocm" ]]; then
    echo "ROCm GFX:    ${HSA_OVERRIDE_GFX_VERSION:-}"
fi

if (( SKIP_TESTS == 0 )); then
    if "${PYTHON_CMD[@]}" -c "import importlib.util, sys; sys.exit(0 if importlib.util.find_spec('pytest') else 2)"; then
        "${PYTHON_CMD[@]}" -m pytest backend/bot_trainer/v2 -q
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
    --device "$RESOLVED_DEVICE"
    --num-workers "$NUM_WORKERS"
    --lr "$LEARNING_RATE"
    --weight-decay "$WEIGHT_DECAY"
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
