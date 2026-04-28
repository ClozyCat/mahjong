#!/usr/bin/env bash
set -euo pipefail

INPUT_PATH="backend/bot_trainer/datasets/data.txt"
OUTPUT_DIR="backend/bot_trainer/v2/out"
PROGRESS_EVERY=1000
MAX_MATCHES=0

usage() {
    cat <<'EOF'
Usage: export_full_dataset.sh [options]

Options:
  --input PATH              Dataset input file.
  --output DIR              Output directory for exported v2 samples.
  --progress-every N        Print progress every N matches.
  --max-matches N           Stop after N matches; 0 exports all matches.
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

while [[ $# -gt 0 ]]; do
    case "$1" in
        --input)
            require_value "$1" "${2:-}"
            INPUT_PATH="$2"
            shift 2
            ;;
        --output)
            require_value "$1" "${2:-}"
            OUTPUT_DIR="$2"
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
require_cuda_gpu

args=(
    run
    --release
    --manifest-path backend/Cargo.toml
    --bin export_bot_dataset_v2
    --
    --input "$INPUT_PATH"
    --output "$OUTPUT_DIR"
    --progress-every "$PROGRESS_EVERY"
)

if (( MAX_MATCHES > 0 )); then
    args+=(--max-matches "$MAX_MATCHES")
fi

echo "Running exporter from $REPO_ROOT"
echo "Input:  $INPUT_PATH"
echo "Output: $OUTPUT_DIR"
echo "Progress interval: every $PROGRESS_EVERY matches"

exec cargo "${args[@]}"
