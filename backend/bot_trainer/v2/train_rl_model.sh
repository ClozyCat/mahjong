#!/usr/bin/env bash
set -euo pipefail

OUTPUT_DIR="backend/bot_trainer/v2/rl_runs/latest"
BASELINE_CHECKPOINT="backend/bot_trainer/v2/checkpoints/best.pt"
BASELINE_ONNX="backend/assets/models/mahjong_policy_net.onnx"
PYTHON_CMD=(python3)
CARGO_CMD=(cargo)
TRAJECTORY_MATCHES=200
EVAL_MATCHES=200
SEED=20260429
MAX_ACTIONS_PER_MATCH=2400
EPOCHS=3
BATCH_SIZE=256
LEARNING_RATE=0.00001
GAMMA=0.99
CLIP_EPSILON=0.2
DEVICE=auto
SELFPLAY_POLICY_ID="selfplay_hybrid30"
SELFPLAY_POLICY_MODE=hybrid
SELFPLAY_NEURAL_WEIGHT=30
SKIP_TESTS=0
SKIP_TRAJECTORY_GENERATION=0
SKIP_ONNX_EXPORT=0
SKIP_EVAL=0

usage() {
    cat <<'EOF'
Usage: train_rl_model.sh [options]

Runs the full RL pipeline:
  1. generate arena trajectories
  2. train PPO checkpoint
  3. export candidate ONNX
  4. evaluate baseline vs candidate in the arena

Options:
  --output-dir DIR                 Directory for RL run artifacts.
  --baseline-checkpoint PATH       Supervised checkpoint to initialize PPO.
  --baseline-onnx PATH             Baseline ONNX used for self-play and evaluation.
  --python-exe PATH                Python executable override. Defaults to python3.
  --cargo-exe PATH                 Cargo executable override. Defaults to cargo.
  --trajectory-matches N           Matches used to generate trajectories.
  --eval-matches N                 Matches used for candidate evaluation.
  --seed N                         Arena seed.
  --max-actions-per-match N        Arena action cap.
  --epochs N                       PPO epochs.
  --batch-size N                   PPO batch size.
  --lr VALUE                       PPO learning rate.
  --gamma VALUE                    Return discount.
  --clip-epsilon VALUE             PPO clipping epsilon.
  --device DEVICE                  auto, cpu, cuda, etc.
  --selfplay-policy-id ID          Policy id written to trajectory rows.
  --selfplay-policy-mode MODE      heuristic, hybrid, or neural.
  --selfplay-neural-weight N       Hybrid neural prior weight.
  --skip-tests                     Skip Python tests.
  --skip-trajectory-generation     Reuse existing trajectories.jsonl in output dir.
  --skip-onnx-export               Do not export candidate.onnx.
  --skip-eval                      Do not run baseline vs candidate arena evaluation.
  -h, --help                       Show this help.
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
        --output-dir)
            require_value "$1" "${2:-}"
            OUTPUT_DIR="$2"
            shift 2
            ;;
        --baseline-checkpoint)
            require_value "$1" "${2:-}"
            BASELINE_CHECKPOINT="$2"
            shift 2
            ;;
        --baseline-onnx)
            require_value "$1" "${2:-}"
            BASELINE_ONNX="$2"
            shift 2
            ;;
        --python-exe)
            require_value "$1" "${2:-}"
            PYTHON_CMD=("$2")
            shift 2
            ;;
        --cargo-exe)
            require_value "$1" "${2:-}"
            CARGO_CMD=("$2")
            shift 2
            ;;
        --trajectory-matches)
            require_value "$1" "${2:-}"
            TRAJECTORY_MATCHES="$2"
            shift 2
            ;;
        --eval-matches)
            require_value "$1" "${2:-}"
            EVAL_MATCHES="$2"
            shift 2
            ;;
        --seed)
            require_value "$1" "${2:-}"
            SEED="$2"
            shift 2
            ;;
        --max-actions-per-match)
            require_value "$1" "${2:-}"
            MAX_ACTIONS_PER_MATCH="$2"
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
        --lr)
            require_value "$1" "${2:-}"
            LEARNING_RATE="$2"
            shift 2
            ;;
        --gamma)
            require_value "$1" "${2:-}"
            GAMMA="$2"
            shift 2
            ;;
        --clip-epsilon)
            require_value "$1" "${2:-}"
            CLIP_EPSILON="$2"
            shift 2
            ;;
        --device)
            require_value "$1" "${2:-}"
            DEVICE="$2"
            shift 2
            ;;
        --selfplay-policy-id)
            require_value "$1" "${2:-}"
            SELFPLAY_POLICY_ID="$2"
            shift 2
            ;;
        --selfplay-policy-mode)
            require_value "$1" "${2:-}"
            SELFPLAY_POLICY_MODE="$2"
            shift 2
            ;;
        --selfplay-neural-weight)
            require_value "$1" "${2:-}"
            SELFPLAY_NEURAL_WEIGHT="$2"
            shift 2
            ;;
        --skip-tests)
            SKIP_TESTS=1
            shift
            ;;
        --skip-trajectory-generation)
            SKIP_TRAJECTORY_GENERATION=1
            shift
            ;;
        --skip-onnx-export)
            SKIP_ONNX_EXPORT=1
            shift
            ;;
        --skip-eval)
            SKIP_EVAL=1
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

if [[ "$SELFPLAY_POLICY_MODE" != "heuristic" && "$SELFPLAY_POLICY_MODE" != "hybrid" && "$SELFPLAY_POLICY_MODE" != "neural" ]]; then
    echo "--selfplay-policy-mode must be heuristic, hybrid, or neural." >&2
    exit 2
fi

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../../.." && pwd)"
cd "$REPO_ROOT"

export PYTHONUTF8=1
export PYTHONIOENCODING=utf-8

TRAJECTORY_CONFIG="$OUTPUT_DIR/trajectory_config.json"
TRAJECTORY_JSONL="$OUTPUT_DIR/trajectories.jsonl"
ARENA_REPORT_JSONL="$OUTPUT_DIR/trajectory_arena_report.jsonl"
CHECKPOINT_DIR="$OUTPUT_DIR/checkpoints"
CANDIDATE_ONNX="$OUTPUT_DIR/candidate.onnx"
EVAL_CONFIG="$OUTPUT_DIR/candidate_eval_config.json"
EVAL_JSONL="$OUTPUT_DIR/candidate_eval.jsonl"

mkdir -p "$OUTPUT_DIR"

"${PYTHON_CMD[@]}" - <<'PY'
import importlib.util
import sys

missing = [name for name in ("torch", "onnxruntime") if importlib.util.find_spec(name) is None]
if missing:
    print("Missing required Python modules: " + ", ".join(missing), file=sys.stderr)
    raise SystemExit(2)
PY

echo "Mahjong RL training"
echo "Output:              $OUTPUT_DIR"
echo "Baseline checkpoint: $BASELINE_CHECKPOINT"
echo "Baseline ONNX:       $BASELINE_ONNX"
echo "Trajectory matches:  $TRAJECTORY_MATCHES"
echo "Eval matches:        $EVAL_MATCHES"
echo "Device:              $DEVICE"
echo "Python:              ${PYTHON_CMD[*]}"
echo "Cargo:               ${CARGO_CMD[*]}"

if (( SKIP_TESTS == 0 )); then
    "${PYTHON_CMD[@]}" -m pytest \
        backend/bot_trainer/v2/test_rl_dataset.py \
        backend/bot_trainer/v2/test_dataset.py \
        -q
fi

if (( SKIP_TRAJECTORY_GENERATION == 0 )); then
    cat > "$TRAJECTORY_CONFIG" <<JSON
{
  "matches": $TRAJECTORY_MATCHES,
  "seed": $SEED,
  "max_actions_per_match": $MAX_ACTIONS_PER_MATCH,
  "report_trajectories": true,
  "policies": [
    {
      "id": "$SELFPLAY_POLICY_ID",
      "mode": "$SELFPLAY_POLICY_MODE",
      "neural_weight": $SELFPLAY_NEURAL_WEIGHT,
      "model_path": "$BASELINE_ONNX"
    }
  ]
}
JSON

    "${CARGO_CMD[@]}" run --manifest-path backend/Cargo.toml --release --bin bot_arena -- \
        --config "$TRAJECTORY_CONFIG" \
        --output "$ARENA_REPORT_JSONL" \
        --trajectories "$TRAJECTORY_JSONL"
fi

"${PYTHON_CMD[@]}" backend/bot_trainer/v2/rl_train.py \
    --trajectories "$TRAJECTORY_JSONL" \
    --checkpoint "$BASELINE_CHECKPOINT" \
    --epochs "$EPOCHS" \
    --batch-size "$BATCH_SIZE" \
    --lr "$LEARNING_RATE" \
    --gamma "$GAMMA" \
    --clip-epsilon "$CLIP_EPSILON" \
    --output "$CHECKPOINT_DIR" \
    --device "$DEVICE"

if (( SKIP_ONNX_EXPORT == 0 )); then
    "${PYTHON_CMD[@]}" backend/bot_trainer/v2/export_onnx.py \
        --checkpoint "$CHECKPOINT_DIR/best.pt" \
        --output "$CANDIDATE_ONNX"
fi

if (( SKIP_EVAL == 0 )); then
    cat > "$EVAL_CONFIG" <<JSON
{
  "matches": $EVAL_MATCHES,
  "seed": $SEED,
  "max_actions_per_match": $MAX_ACTIONS_PER_MATCH,
  "report_trajectories": false,
  "policies": [
    {
      "id": "baseline_hybrid30",
      "mode": "hybrid",
      "neural_weight": 30,
      "model_path": "$BASELINE_ONNX"
    },
    {
      "id": "rl_candidate_hybrid30",
      "mode": "hybrid",
      "neural_weight": 30,
      "model_path": "$CANDIDATE_ONNX"
    }
  ]
}
JSON

    "${CARGO_CMD[@]}" run --manifest-path backend/Cargo.toml --release --bin bot_arena -- \
        --config "$EVAL_CONFIG" \
        --output "$EVAL_JSONL"
fi

echo "RL training pipeline finished."
echo "Checkpoint:  $CHECKPOINT_DIR/best.pt"
echo "Candidate:   $CANDIDATE_ONNX"
echo "Evaluation:  $EVAL_JSONL"
