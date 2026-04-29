#!/usr/bin/env bash
set -euo pipefail

OUTPUT_DIR="backend/bot_trainer/v2/rl_runs/latest"
BASELINE_CHECKPOINT="backend/bot_trainer/v2/checkpoints/best.pt"
BASELINE_ONNX="backend/assets/models/mahjong_policy_net.onnx"
PYTHON_CMD=(python)
CARGO_CMD=(cargo)
ARENA_JOBS=0
TRAJECTORY_MATCHES=200
TRAJECTORY_PROGRESS_EVERY=20
EVAL_MATCHES=200
SEED=20260429
MAX_ACTIONS_PER_MATCH=2400
EPOCHS=3
BATCH_SIZE=256
LEARNING_RATE=0.00001
GAMMA=0.99
GAE_LAMBDA=0.95
CLIP_EPSILON=0.2
VALUE_CLIP_EPSILON=0.2
ENTROPY_COEF=0.02
ENTROPY_END_COEF=0.005
ENTROPY_DECAY_STEPS=0
KL_COEF=0.01
KL_END_COEF=0.0
DEVICE=auto
OPPONENT_POOL="backend/bot_trainer/v2/opponent_pool.json"
LEARNER_POLICY_ID="learner"
SELFPLAY_POLICY_ID="selfplay_neural"
SELFPLAY_POLICY_MODE=neural
SKIP_TESTS=0
SKIP_TRAJECTORY_GENERATION=0
SKIP_ONNX_EXPORT=0
SKIP_EVAL=0
ENFORCE_CANDIDATE_GATE=0
RECOMPUTE_OLD_POLICY_STATS=0

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
  --python-exe PATH                Python executable override. Defaults to python.
  --cargo-exe PATH                 Cargo executable override. Defaults to cargo.
  --arena-jobs N                   Parallel arena workers. Use 0 for all available cores.
  --trajectory-matches N           Matches used to generate trajectories.
  --trajectory-progress-every N    Print trajectory arena progress every N matches. Use 0 to disable.
  --eval-matches N                 Matches used for candidate evaluation.
  --seed N                         Arena seed.
  --max-actions-per-match N        Arena action cap.
  --epochs N                       PPO epochs.
  --batch-size N                   PPO batch size.
  --lr VALUE                       PPO learning rate.
  --gamma VALUE                    Return discount.
  --gae-lambda VALUE               GAE lambda.
  --clip-epsilon VALUE             PPO clipping epsilon.
  --value-clip-epsilon VALUE       PPO value clipping epsilon.
  --entropy-coef VALUE             PPO entropy coefficient.
  --entropy-end-coef VALUE         PPO final entropy coefficient after decay.
  --entropy-decay-steps N          Linear entropy decay steps. Use 0 for full training.
  --kl-coef VALUE                  Supervised policy KL coefficient.
  --kl-end-coef VALUE              Final KL coefficient after decay.
  --device DEVICE                  auto, cpu, cuda, etc.
  --opponent-pool PATH             Opponent pool JSON for league rollout.
  --learner-policy-id ID           Policy id filtered for PPO training.
  --selfplay-policy-id ID          Policy id written to trajectory rows.
  --selfplay-policy-mode MODE      heuristic or neural.
  --skip-tests                     Skip Python tests.
  --skip-trajectory-generation     Reuse existing trajectories.jsonl in output dir.
  --skip-onnx-export               Do not export candidate.onnx.
  --skip-eval                      Do not run baseline vs candidate arena evaluation.
  --enforce-candidate-gate         Exit non-zero when candidate acceptance fails.
  --recompute-old-policy-stats     Recompute old log-probs and values from checkpoint.
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

require_file() {
    local path="$1"
    local purpose="$2"
    local advice="$3"
    if [[ ! -f "$path" ]]; then
        echo "$purpose not found: $path" >&2
        echo "$advice" >&2
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
        --arena-jobs)
            require_value "$1" "${2:-}"
            ARENA_JOBS="$2"
            shift 2
            ;;
        --trajectory-matches)
            require_value "$1" "${2:-}"
            TRAJECTORY_MATCHES="$2"
            shift 2
            ;;
        --trajectory-progress-every)
            require_value "$1" "${2:-}"
            TRAJECTORY_PROGRESS_EVERY="$2"
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
        --gae-lambda)
            require_value "$1" "${2:-}"
            GAE_LAMBDA="$2"
            shift 2
            ;;
        --clip-epsilon)
            require_value "$1" "${2:-}"
            CLIP_EPSILON="$2"
            shift 2
            ;;
        --value-clip-epsilon)
            require_value "$1" "${2:-}"
            VALUE_CLIP_EPSILON="$2"
            shift 2
            ;;
        --entropy-coef)
            require_value "$1" "${2:-}"
            ENTROPY_COEF="$2"
            shift 2
            ;;
        --entropy-end-coef)
            require_value "$1" "${2:-}"
            ENTROPY_END_COEF="$2"
            shift 2
            ;;
        --entropy-decay-steps)
            require_value "$1" "${2:-}"
            ENTROPY_DECAY_STEPS="$2"
            shift 2
            ;;
        --kl-coef)
            require_value "$1" "${2:-}"
            KL_COEF="$2"
            shift 2
            ;;
        --kl-end-coef)
            require_value "$1" "${2:-}"
            KL_END_COEF="$2"
            shift 2
            ;;
        --device)
            require_value "$1" "${2:-}"
            DEVICE="$2"
            shift 2
            ;;
        --opponent-pool)
            require_value "$1" "${2:-}"
            OPPONENT_POOL="$2"
            shift 2
            ;;
        --learner-policy-id)
            require_value "$1" "${2:-}"
            LEARNER_POLICY_ID="$2"
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
        --enforce-candidate-gate)
            ENFORCE_CANDIDATE_GATE=1
            shift
            ;;
        --recompute-old-policy-stats)
            RECOMPUTE_OLD_POLICY_STATS=1
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

if [[ "$SELFPLAY_POLICY_MODE" != "heuristic" && "$SELFPLAY_POLICY_MODE" != "neural" ]]; then
    echo "--selfplay-policy-mode must be heuristic or neural." >&2
    exit 2
fi

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../../.." && pwd)"
cd "$REPO_ROOT"

export PYTHONUTF8=1
export PYTHONIOENCODING=utf-8

TRAJECTORY_CONFIG_DIR="$OUTPUT_DIR/trajectory_configs"
TRAJECTORY_JSONL="$OUTPUT_DIR/trajectories.jsonl"
ARENA_REPORT_JSONL="$OUTPUT_DIR/trajectory_arena_report.jsonl"
CHECKPOINT_DIR="$OUTPUT_DIR/checkpoints"
CANDIDATE_ONNX="$OUTPUT_DIR/candidate.onnx"
EVAL_CONFIG="$OUTPUT_DIR/candidate_eval_config.json"
EVAL_JSONL="$OUTPUT_DIR/candidate_eval.jsonl"
EVAL_SUMMARY="$OUTPUT_DIR/candidate_eval_summary.json"
GATE_OUTPUT="$OUTPUT_DIR/candidate_gate.json"

mkdir -p "$OUTPUT_DIR"
TEMP_DIR="$OUTPUT_DIR/tmp"
PYTEST_SITE_DIR="$TEMP_DIR/pytest_site"
mkdir -p "$TEMP_DIR" "$PYTEST_SITE_DIR"
export TMPDIR="$TEMP_DIR"
export PYTEST_DEBUG_TEMPROOT="$TMPDIR"

cat > "$PYTEST_SITE_DIR/sitecustomize.py" <<'PY'
import os
import pathlib

if os.name == "nt":
    _original_mkdir = pathlib.Path.mkdir

    def _mkdir_with_accessible_mode(self, mode=0o777, parents=False, exist_ok=False):
        if mode == 0o700:
            mode = 0o777
        return _original_mkdir(self, mode=mode, parents=parents, exist_ok=exist_ok)

    pathlib.Path.mkdir = _mkdir_with_accessible_mode
PY

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
echo "Trajectory progress: every $TRAJECTORY_PROGRESS_EVERY match(es)"
echo "Opponent pool:       $OPPONENT_POOL"
echo "Learner policy id:   $LEARNER_POLICY_ID"
echo "Eval matches:        $EVAL_MATCHES"
echo "Device:              $DEVICE"
echo "Python:              ${PYTHON_CMD[*]}"
echo "Cargo:               ${CARGO_CMD[*]}"
if (( ARENA_JOBS == 0 )); then
    echo "Arena jobs:          auto"
else
    echo "Arena jobs:          $ARENA_JOBS"
fi

require_file \
    "$BASELINE_CHECKPOINT" \
    "Baseline checkpoint" \
    "Run supervised training first with backend/bot_trainer/v2/train_and_export_model.sh, or pass --baseline-checkpoint <existing .pt file>."
require_file \
    "$BASELINE_ONNX" \
    "Baseline ONNX model" \
    "Export the supervised model first, or pass --baseline-onnx <existing .onnx file>."

if (( SKIP_TRAJECTORY_GENERATION == 1 )); then
    require_file \
        "$TRAJECTORY_JSONL" \
        "Trajectory JSONL" \
        "Remove --skip-trajectory-generation, or place an existing trajectories.jsonl at $TRAJECTORY_JSONL."
fi

if (( SKIP_TESTS == 0 )); then
    PYTHONPATH="$PYTEST_SITE_DIR${PYTHONPATH:+:$PYTHONPATH}" "${PYTHON_CMD[@]}" -m pytest \
        backend/bot_trainer/v2/test_rl_dataset.py \
        backend/bot_trainer/v2/test_model.py \
        backend/bot_trainer/v2/test_dataset.py \
        -q \
        -p no:cacheprovider \
        --basetemp "$TEMP_DIR/pytest"
fi

if (( SKIP_TRAJECTORY_GENERATION == 0 )); then
    mkdir -p "$TRAJECTORY_CONFIG_DIR"
    "${PYTHON_CMD[@]}" backend/bot_trainer/v2/league_config.py \
        --pool "$OPPONENT_POOL" \
        --output-dir "$TRAJECTORY_CONFIG_DIR" \
        --matches "$TRAJECTORY_MATCHES" \
        --seed "$SEED" \
        --max-actions "$MAX_ACTIONS_PER_MATCH" \
        --mode trajectory \
        --rollout-onnx "$BASELINE_ONNX"

    trajectory_files=()
    for config_path in "$TRAJECTORY_CONFIG_DIR"/trajectory_config_*.json; do
        [[ -e "$config_path" ]] || continue
        config_name="$(basename "$config_path" .json)"
        index="${config_name#trajectory_config_}"
        partial_report="$OUTPUT_DIR/trajectory_arena_report_$index.jsonl"
        partial_trajectory="$OUTPUT_DIR/trajectories_$index.jsonl"
        trajectory_files+=("$partial_trajectory")
        arena_args=(
            run --manifest-path backend/Cargo.toml --release --bin bot_arena --
            --config "$config_path"
            --output "$partial_report"
            --trajectories "$partial_trajectory"
            --jobs "$ARENA_JOBS"
        )
        if (( TRAJECTORY_PROGRESS_EVERY > 0 )); then
            arena_args+=(--progress-every "$TRAJECTORY_PROGRESS_EVERY")
        fi
        "${CARGO_CMD[@]}" "${arena_args[@]}"
    done
    if (( ${#trajectory_files[@]} == 0 )); then
        echo "No trajectory configs generated in $TRAJECTORY_CONFIG_DIR" >&2
        exit 2
    fi
    cat "${trajectory_files[@]}" > "$TRAJECTORY_JSONL"
fi

rl_train_args=(
    backend/bot_trainer/v2/rl_train.py
    --trajectories "$TRAJECTORY_JSONL"
    --checkpoint "$BASELINE_CHECKPOINT"
    --epochs "$EPOCHS"
    --batch-size "$BATCH_SIZE"
    --lr "$LEARNING_RATE"
    --gamma "$GAMMA"
    --gae-lambda "$GAE_LAMBDA"
    --policy-id "$LEARNER_POLICY_ID"
    --clip-epsilon "$CLIP_EPSILON"
    --value-clip-epsilon "$VALUE_CLIP_EPSILON"
    --entropy-coef "$ENTROPY_COEF"
    --entropy-end-coef "$ENTROPY_END_COEF"
    --kl-coef "$KL_COEF"
    --kl-end-coef "$KL_END_COEF"
    --output "$CHECKPOINT_DIR"
    --device "$DEVICE"
)
if (( ENTROPY_DECAY_STEPS > 0 )); then
    rl_train_args+=(--entropy-decay-steps "$ENTROPY_DECAY_STEPS")
fi
if (( RECOMPUTE_OLD_POLICY_STATS == 1 )); then
    rl_train_args+=(--recompute-old-policy-stats)
fi
"${PYTHON_CMD[@]}" "${rl_train_args[@]}"

if (( SKIP_ONNX_EXPORT == 0 )); then
    "${PYTHON_CMD[@]}" backend/bot_trainer/v2/export_onnx.py \
        --checkpoint "$CHECKPOINT_DIR/best.pt" \
        --output "$CANDIDATE_ONNX"
fi

if (( SKIP_EVAL == 0 )); then
    "${PYTHON_CMD[@]}" backend/bot_trainer/v2/league_config.py \
        --pool "$OPPONENT_POOL" \
        --output-dir "$OUTPUT_DIR" \
        --matches "$EVAL_MATCHES" \
        --seed "$SEED" \
        --max-actions "$MAX_ACTIONS_PER_MATCH" \
        --mode eval \
        --candidate-onnx "$CANDIDATE_ONNX" \
        --baseline-onnx "$BASELINE_ONNX"

    "${CARGO_CMD[@]}" run --manifest-path backend/Cargo.toml --release --bin bot_arena -- \
        --config "$EVAL_CONFIG" \
        --output "$EVAL_JSONL" \
        --jobs "$ARENA_JOBS"

    "${PYTHON_CMD[@]}" backend/bot_trainer/v2/arena_summary.py \
        --input "$EVAL_JSONL" \
        --output "$EVAL_SUMMARY"

    set +e
    "${PYTHON_CMD[@]}" backend/bot_trainer/v2/candidate_gate.py \
        --summary "$EVAL_SUMMARY" \
        --baseline-policy baseline_neural \
        --candidate-policy rl_candidate_neural \
        --output "$GATE_OUTPUT"
    gate_exit=$?
    set -e
    if (( ENFORCE_CANDIDATE_GATE == 1 && gate_exit != 0 )); then
        exit "$gate_exit"
    fi
    if (( ENFORCE_CANDIDATE_GATE != 1 && gate_exit != 0 )); then
        echo "Candidate gate rejected this model. See $GATE_OUTPUT" >&2
    fi
fi

echo "RL training pipeline finished."
echo "Checkpoint:  $CHECKPOINT_DIR/best.pt"
echo "Candidate:   $CANDIDATE_ONNX"
echo "Evaluation:  $EVAL_JSONL"
echo "Summary:     $EVAL_SUMMARY"
