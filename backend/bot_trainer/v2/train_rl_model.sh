#!/usr/bin/env bash
set -euo pipefail

OUTPUT_DIR="backend/bot_trainer/v2/rl_runs/$(date +%Y%m%d%H%M)"
BASELINE_CHECKPOINT="backend/bot_trainer/v2/checkpoints/best.pt"
BASELINE_ONNX="backend/assets/models/mahjong_policy_net.onnx"
PYTHON_CMD=(python)
CARGO_CMD=(cargo)
ARENA_JOBS=0
ITERATIONS=5
ITERATION_MATCHES=500
TRAJECTORY_PROGRESS_EVERY=20
EVAL_MATCHES=1000
SEED=20260429
MAX_ACTIONS_PER_MATCH=2400
EPOCHS=1
BATCH_SIZE=256
LEARNING_RATE=0.000003
GAMMA=0.995
GAE_LAMBDA=0.95
CLIP_EPSILON=0.2
VALUE_CLIP_EPSILON=0.2
ENTROPY_COEF=0.02
ENTROPY_END_COEF=0.005
ENTROPY_DECAY_STEPS=0
KL_COEF=0.01
KL_END_COEF=0.0
TARGET_KL=0.03
PLAY_STYLE=balanced
PLAY_STYLES=""
TRAJECTORY_ROLLOUT_STYLE=""
DEVICE=auto
OPPONENT_POOL="backend/bot_trainer/v2/opponent_pool.json"
LEARNER_POLICY_ID="learner"
SELFPLAY_POLICY_ID="selfplay_neural"
SELFPLAY_POLICY_MODE=neural
SKIP_TESTS=0
SKIP_ONNX_EXPORT=0
SKIP_EVAL=0
ENFORCE_CANDIDATE_GATE=0
ALLOW_RL_BASELINE_CHECKPOINT=0
RECOMPUTE_OLD_POLICY_STATS=0
RECORD_HEURISTIC_COMPARISON=0
CANDIDATE_SELECTION_MODE=epoch

usage() {
    cat <<'EOF'
Usage: train_rl_model.sh [options]

Runs the iterative self-play RL pipeline:
  For each iteration:
    1. generate arena trajectories using current best model
    2. train PPO checkpoint
    3. export candidate ONNX
    4. evaluate candidate vs original baseline
    5. update rollout model if improved

Options:
  --output-dir DIR                 Directory for RL run artifacts.
  --baseline-checkpoint PATH       Supervised checkpoint to initialize PPO.
  --baseline-onnx PATH             Baseline ONNX used for evaluation reference.
  --python-exe PATH                Python executable override. Defaults to python.
  --cargo-exe PATH                 Cargo executable override. Defaults to cargo.
  --arena-jobs N                   Parallel arena workers. Use 0 for all available cores.
  --iterations N                   Number of self-play iterations. Default 5.
  --iteration-matches N            Matches per iteration for trajectory generation. Default 500.
  --trajectory-progress-every N    Print trajectory arena progress every N matches. Use 0 to disable.
  --eval-matches N                 Matches used for candidate evaluation.
  --seed N                         Arena seed.
  --max-actions-per-match N        Arena action cap.
  --epochs N                       PPO epochs per iteration. Default 2.
  --batch-size N                   PPO batch size.
  --lr VALUE                       PPO learning rate.
  --gamma VALUE                    Return discount. Default 0.97.
  --gae-lambda VALUE               GAE lambda.
  --clip-epsilon VALUE             PPO clipping epsilon.
  --value-clip-epsilon VALUE       PPO value clipping epsilon.
  --entropy-coef VALUE             PPO entropy coefficient.
  --entropy-end-coef VALUE         PPO final entropy coefficient after decay.
  --entropy-decay-steps N          Linear entropy decay steps. Use 0 for full training.
  --kl-coef VALUE                  Supervised policy KL coefficient. Default 0.01.
  --kl-end-coef VALUE              Final KL coefficient after decay.
  --target-kl VALUE                Stop PPO epoch loop when approximate KL exceeds this value.
  --play-style STYLE               Play style: aggressive, balanced, or defensive. Default balanced.
  --play-styles LIST               Comma-separated play styles trained in parallel on shared trajectories.
  --trajectory-rollout-style STYLE Deprecated; ignored because trajectories are generated per play style.
  --device DEVICE                  auto, cpu, cuda, etc.
  --opponent-pool PATH             Opponent pool JSON for league rollout.
  --learner-policy-id ID           Policy id filtered for PPO training.
  --selfplay-policy-id ID          Policy id written to trajectory rows.
  --selfplay-policy-mode MODE      heuristic or neural.
  --skip-tests                     Skip Python tests.
  --skip-onnx-export               Do not export candidate.onnx.
  --skip-eval                      Do not run baseline vs candidate arena evaluation.
  --enforce-candidate-gate         Exit non-zero when no iteration passes candidate gate.
  --allow-rl-baseline-checkpoint   Allow intentionally continuing from an RL checkpoint.
  --recompute-old-policy-stats     Recompute old log-probs and values from checkpoint.
  --record-heuristic-comparison    Record same-as-heuristic telemetry during arena runs.
  --candidate-selection-mode MODE  epoch or final. Default epoch.
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

copy_required_file() {
    local source_path="$1"
    local target_path="$2"
    if [[ ! -f "$source_path" ]]; then
        echo "Required file was not found: $source_path" >&2
        exit 2
    fi
    cp -f "$source_path" "$target_path"
}

is_valid_play_style() {
    [[ "$1" == "aggressive" || "$1" == "balanced" || "$1" == "defensive" ]]
}

contains_style() {
    local needle="$1"
    shift
    local style
    for style in "$@"; do
        [[ "$style" == "$needle" ]] && return 0
    done
    return 1
}

set_style_paths() {
    local iter_dir="$1"
    local style="$2"
    if (( MULTI_STYLE_TRAINING == 1 )); then
        STYLE_DIR="$iter_dir/styles/$style"
    else
        STYLE_DIR="$iter_dir"
    fi
    STYLE_CHECKPOINT_DIR="$STYLE_DIR/checkpoints"
    STYLE_CANDIDATE_ONNX="$STYLE_DIR/candidate.onnx"
    STYLE_EVAL_DIR="$STYLE_DIR/eval"
}

set_final_style_paths() {
    local style="$1"
    if (( MULTI_STYLE_TRAINING == 1 )); then
        FINAL_STYLE_DIR="$OUTPUT_DIR/styles/$style"
    else
        FINAL_STYLE_DIR="$OUTPUT_DIR"
    fi
    FINAL_STYLE_CANDIDATE_ONNX="$FINAL_STYLE_DIR/candidate.onnx"
    FINAL_STYLE_CHECKPOINT_DIR="$FINAL_STYLE_DIR/checkpoints"
    FINAL_STYLE_EVAL_SUMMARY="$FINAL_STYLE_DIR/candidate_eval_summary.json"
    FINAL_STYLE_GATE_OUTPUT="$FINAL_STYLE_DIR/candidate_gate.json"
    FINAL_STYLE_HISTORY="$FINAL_STYLE_DIR/iteration_history.json"
}

run_candidate_eval() {
    local candidate_model="$1"
    local eval_dir="$2"
    local eval_baseline_onnx="$3"
    mkdir -p "$eval_dir"
    eval_config_args=(
        backend/bot_trainer/v2/league_config.py
        --pool "$OPPONENT_POOL"
        --output-dir "$eval_dir"
        --matches "$EVAL_MATCHES"
        --seed "$SEED"
        --max-actions "$MAX_ACTIONS_PER_MATCH"
        --mode eval
        --candidate-onnx "$candidate_model"
        --baseline-onnx "$eval_baseline_onnx"
    )
    if (( RECORD_HEURISTIC_COMPARISON == 1 )); then
        eval_config_args+=(--record-heuristic-comparison)
    fi
    "${PYTHON_CMD[@]}" "${eval_config_args[@]}"

    RUN_EVAL_CONFIG="$eval_dir/candidate_eval_config.json"
    RUN_EVAL_JSONL="$eval_dir/candidate_eval.jsonl"
    RUN_EVAL_SUMMARY="$eval_dir/candidate_eval_summary.json"
    RUN_EVAL_GATE="$eval_dir/candidate_gate.json"

    "${CARGO_CMD[@]}" run --manifest-path backend/Cargo.toml --release --bin bot_arena -- \
        --config "$RUN_EVAL_CONFIG" \
        --output "$RUN_EVAL_JSONL" \
        --jobs "$ARENA_JOBS"

    "${PYTHON_CMD[@]}" backend/bot_trainer/v2/arena_summary.py \
        --input "$RUN_EVAL_JSONL" \
        --output "$RUN_EVAL_SUMMARY"

    set +e
    "${PYTHON_CMD[@]}" backend/bot_trainer/v2/candidate_gate.py \
        --summary "$RUN_EVAL_SUMMARY" \
        --baseline-policy baseline_neural \
        --candidate-policy rl_candidate_neural \
        --output "$RUN_EVAL_GATE"
    RUN_EVAL_GATE_EXIT=$?
    set -e
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
        --iterations)
            require_value "$1" "${2:-}"
            ITERATIONS="$2"
            shift 2
            ;;
        --iteration-matches)
            require_value "$1" "${2:-}"
            ITERATION_MATCHES="$2"
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
        --target-kl)
            require_value "$1" "${2:-}"
            TARGET_KL="$2"
            shift 2
            ;;
        --play-style)
            require_value "$1" "${2:-}"
            PLAY_STYLE="$2"
            shift 2
            ;;
        --play-styles)
            require_value "$1" "${2:-}"
            PLAY_STYLES="$2"
            shift 2
            ;;
        --trajectory-rollout-style)
            require_value "$1" "${2:-}"
            TRAJECTORY_ROLLOUT_STYLE="$2"
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
        --allow-rl-baseline-checkpoint)
            ALLOW_RL_BASELINE_CHECKPOINT=1
            shift
            ;;
        --recompute-old-policy-stats)
            RECOMPUTE_OLD_POLICY_STATS=1
            shift
            ;;
        --record-heuristic-comparison)
            RECORD_HEURISTIC_COMPARISON=1
            shift
            ;;
        --candidate-selection-mode)
            require_value "$1" "${2:-}"
            CANDIDATE_SELECTION_MODE="$2"
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

if [[ "$SELFPLAY_POLICY_MODE" != "heuristic" && "$SELFPLAY_POLICY_MODE" != "neural" ]]; then
    echo "--selfplay-policy-mode must be heuristic or neural." >&2
    exit 2
fi
if [[ "$CANDIDATE_SELECTION_MODE" != "epoch" && "$CANDIDATE_SELECTION_MODE" != "final" ]]; then
    echo "--candidate-selection-mode must be epoch or final." >&2
    exit 2
fi
if ! is_valid_play_style "$PLAY_STYLE"; then
    echo "--play-style must be aggressive, balanced, or defensive." >&2
    exit 2
fi
if [[ -n "$TRAJECTORY_ROLLOUT_STYLE" ]]; then
    echo "--trajectory-rollout-style is ignored because each play style now generates its own trajectories." >&2
fi

ACTIVE_PLAY_STYLES=()
if [[ -n "$PLAY_STYLES" ]]; then
    IFS=',' read -ra requested_styles <<< "$PLAY_STYLES"
else
    requested_styles=("$PLAY_STYLE")
fi
for requested_style in "${requested_styles[@]}"; do
    requested_style="${requested_style//[[:space:]]/}"
    [[ -n "$requested_style" ]] || continue
    if ! is_valid_play_style "$requested_style"; then
        echo "--play-styles contains invalid style: $requested_style" >&2
        exit 2
    fi
    if ! contains_style "$requested_style" "${ACTIVE_PLAY_STYLES[@]}"; then
        ACTIVE_PLAY_STYLES+=("$requested_style")
    fi
done
if (( ${#ACTIVE_PLAY_STYLES[@]} == 0 )); then
    echo "No play styles selected." >&2
    exit 2
fi
MULTI_STYLE_TRAINING=0
if (( ${#ACTIVE_PLAY_STYLES[@]} > 1 )); then
    MULTI_STYLE_TRAINING=1
fi

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../../.." && pwd)"
cd "$REPO_ROOT"

export PYTHONUTF8=1
export PYTHONIOENCODING=utf-8

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

echo "Mahjong RL training (iterative self-play)"
echo "Output:              $OUTPUT_DIR"
echo "Baseline checkpoint: $BASELINE_CHECKPOINT"
echo "Baseline ONNX:       $BASELINE_ONNX"
echo "Iterations:          $ITERATIONS"
echo "Matches/iteration:   $ITERATION_MATCHES"
echo "PPO epochs/iter:     $EPOCHS"
echo "Gamma:               $GAMMA"
echo "KL coef:             $KL_COEF"
echo "Opponent pool:       $OPPONENT_POOL"
echo "Learner policy id:   $LEARNER_POLICY_ID"
echo "Eval matches:        $EVAL_MATCHES"
echo "Device:              $DEVICE"
echo "Play styles:         ${ACTIVE_PLAY_STYLES[*]}"
echo "Python:              ${PYTHON_CMD[*]}"
echo "Cargo:               ${CARGO_CMD[*]}"
if (( ARENA_JOBS == 0 )); then
    echo "Arena jobs:          auto"
else
    echo "Arena jobs:          $ARENA_JOBS"
fi
if (( RECORD_HEURISTIC_COMPARISON == 1 )); then
    echo "Heuristic compare:   true"
else
    echo "Heuristic compare:   false"
fi

require_file \
    "$BASELINE_CHECKPOINT" \
    "Baseline checkpoint" \
    "Run supervised training first with backend/bot_trainer/v2/train_and_export_model.sh, or pass --baseline-checkpoint <existing .pt file>."
require_file \
    "$BASELINE_ONNX" \
    "Baseline ONNX model" \
    "Export the supervised model first, or pass --baseline-onnx <existing .onnx file>."

baseline_guard_args=(
    backend/bot_trainer/v2/baseline_guard.py
    --checkpoint "$BASELINE_CHECKPOINT"
    --onnx "$BASELINE_ONNX"
)
if (( ALLOW_RL_BASELINE_CHECKPOINT == 1 )); then
    baseline_guard_args+=(--allow-rl-checkpoint)
fi
"${PYTHON_CMD[@]}" "${baseline_guard_args[@]}"

if (( SKIP_TESTS == 0 )); then
    PYTHONPATH="$PYTEST_SITE_DIR${PYTHONPATH:+:$PYTHONPATH}" "${PYTHON_CMD[@]}" -m pytest \
        backend/bot_trainer/v2/test_rl_dataset.py \
        backend/bot_trainer/v2/test_model.py \
        backend/bot_trainer/v2/test_dataset.py \
        -q \
        -p no:cacheprovider \
        --basetemp "$TEMP_DIR/pytest"
fi

# ── Iterative Self-Play Loop ──────────────────────────────────────────────
declare -A current_onnx_by_style=()
declare -A current_checkpoint_by_style=()
declare -A best_onnx_by_style=()
declare -A best_checkpoint_by_style=()
declare -A best_score_margin_by_style=()
declare -A best_iter_by_style=()
declare -A history_file_by_style=()

for style in "${ACTIVE_PLAY_STYLES[@]}"; do
    current_onnx_by_style["$style"]="$BASELINE_ONNX"
    current_checkpoint_by_style["$style"]="$BASELINE_CHECKPOINT"
    best_onnx_by_style["$style"]="$BASELINE_ONNX"
    best_checkpoint_by_style["$style"]="$BASELINE_CHECKPOINT"
    best_score_margin_by_style["$style"]="0.0"
    best_iter_by_style["$style"]=0
    history_file_by_style["$style"]="$OUTPUT_DIR/${style}_iteration_results.jsonl"
    : > "${history_file_by_style[$style]}"
done

for (( iter = 1; iter <= ITERATIONS; iter++ )); do
    printf -v iter_tag "iter_%03d" "$iter"
    iter_dir="$OUTPUT_DIR/$iter_tag"
    iter_seed=$(( SEED + (iter - 1) * 1000000 ))

    echo ""
    echo "═══════════════════════════════════════════════════════════════"
    echo "  Iteration $iter / $ITERATIONS"
    echo "═══════════════════════════════════════════════════════════════"

    # ── Step 1/2/3/4: Generate trajectories, train, export, and evaluate each style serially ─
    for style in "${ACTIVE_PLAY_STYLES[@]}"; do
        set_style_paths "$iter_dir" "$style"
        mkdir -p "$STYLE_CHECKPOINT_DIR" "$STYLE_DIR"
        iter_trajectory_config_dir="$STYLE_DIR/trajectory_configs"
        iter_trajectory_jsonl="$STYLE_DIR/trajectories.jsonl"
        rollout_onnx="${current_onnx_by_style[$style]}"
        echo "  Generating trajectories: style=$style rollout=$(basename "$rollout_onnx")"
        mkdir -p "$iter_trajectory_config_dir"
        trajectory_config_args=(
            backend/bot_trainer/v2/league_config.py
            --pool "$OPPONENT_POOL"
            --output-dir "$iter_trajectory_config_dir"
            --matches "$ITERATION_MATCHES"
            --seed "$iter_seed"
            --max-actions "$MAX_ACTIONS_PER_MATCH"
            --mode trajectory
            --rollout-onnx "$rollout_onnx"
        )
        if (( RECORD_HEURISTIC_COMPARISON == 1 )); then
            trajectory_config_args+=(--record-heuristic-comparison)
        fi
        "${PYTHON_CMD[@]}" "${trajectory_config_args[@]}"

        trajectory_files=()
        for config_path in "$iter_trajectory_config_dir"/trajectory_config_*.json; do
            [[ -e "$config_path" ]] || continue
            config_name="$(basename "$config_path" .json)"
            index="${config_name#trajectory_config_}"
            partial_report="$STYLE_DIR/trajectory_arena_report_$index.jsonl"
            partial_trajectory="$STYLE_DIR/trajectories_$index.jsonl"
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
            echo "No trajectory configs generated in $iter_trajectory_config_dir" >&2
            exit 2
        fi
        cat "${trajectory_files[@]}" > "$iter_trajectory_jsonl"

        rl_train_args=(
            backend/bot_trainer/v2/rl_train.py
            --trajectories "$iter_trajectory_jsonl"
            --checkpoint "${current_checkpoint_by_style[$style]}"
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
            --target-kl "$TARGET_KL"
            --play-style "$style"
            --output "$STYLE_CHECKPOINT_DIR"
            --device "$DEVICE"
        )
        if (( ENTROPY_DECAY_STEPS > 0 )); then
            rl_train_args+=(--entropy-decay-steps "$ENTROPY_DECAY_STEPS")
        fi
        if (( RECOMPUTE_OLD_POLICY_STATS == 1 )); then
            rl_train_args+=(--recompute-old-policy-stats)
        fi
        echo "  Starting PPO training: style=$style"
        "${PYTHON_CMD[@]}" "${rl_train_args[@]}"
        echo "  PPO training finished: style=$style"

        iter_best_pt="$STYLE_CHECKPOINT_DIR/best.pt"
        selected_checkpoint="$iter_best_pt"
        selected_onnx="$STYLE_CANDIDATE_ONNX"
        if (( SKIP_ONNX_EXPORT == 0 )); then
            "${PYTHON_CMD[@]}" backend/bot_trainer/v2/export_onnx.py \
                --checkpoint "$iter_best_pt" \
                --output "$STYLE_CANDIDATE_ONNX"
        fi

        if [[ "$CANDIDATE_SELECTION_MODE" == "epoch" && $SKIP_ONNX_EXPORT == 0 && $SKIP_EVAL == 0 ]]; then
            candidate_entries_jsonl="$STYLE_DIR/candidate_entries.jsonl"
            candidate_manifest="$STYLE_DIR/candidate_manifest.json"
            candidate_selection="$STYLE_DIR/candidate_selection.json"
            : > "$candidate_entries_jsonl"
            for epoch_pt in "$STYLE_CHECKPOINT_DIR"/epoch_*.pt; do
                [[ -e "$epoch_pt" ]] || continue
                epoch_name="$(basename "$epoch_pt" .pt)"
                epoch_number="${epoch_name#epoch_}"
                epoch_onnx="$STYLE_DIR/$epoch_name.onnx"
                epoch_eval_dir="$STYLE_EVAL_DIR/$epoch_name"
                "${PYTHON_CMD[@]}" backend/bot_trainer/v2/export_onnx.py \
                    --checkpoint "$epoch_pt" \
                    --output "$epoch_onnx"
                run_candidate_eval "$epoch_onnx" "$epoch_eval_dir" "$BASELINE_ONNX"
                "${PYTHON_CMD[@]}" - "$candidate_entries_jsonl" "$style" "$epoch_number" "$epoch_pt" "$epoch_onnx" "$RUN_EVAL_SUMMARY" "$RUN_EVAL_GATE" <<'PY'
import json
import sys
from pathlib import Path

output = Path(sys.argv[1])
entry = {
    "play_style": sys.argv[2],
    "epoch": int(sys.argv[3]),
    "checkpoint": sys.argv[4],
    "onnx": sys.argv[5],
    "summary": sys.argv[6],
    "gate_path": sys.argv[7],
}
with output.open("a", encoding="utf-8") as handle:
    handle.write(json.dumps(entry, ensure_ascii=False) + "\n")
PY
            done
            "${PYTHON_CMD[@]}" - "$candidate_entries_jsonl" "$candidate_manifest" <<'PY'
import json
import sys
from pathlib import Path

rows = [
    json.loads(line)
    for line in Path(sys.argv[1]).read_text(encoding="utf-8").splitlines()
    if line.strip()
]
Path(sys.argv[2]).write_text(
    json.dumps({"candidates": rows}, indent=2, ensure_ascii=False) + "\n",
    encoding="utf-8",
)
PY
            "${PYTHON_CMD[@]}" backend/bot_trainer/v2/candidate_selector.py \
                --manifest "$candidate_manifest" \
                --output "$candidate_selection"
            readarray -t selection_fields < <("${PYTHON_CMD[@]}" - "$candidate_selection" <<'PY'
import json
import sys
from pathlib import Path

selection = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
selected = selection["selected"]
print(selection["checkpoint"])
print(selection["onnx"])
print(selected.get("gate") or "")
print(selected.get("summary") or "")
PY
)
            selected_checkpoint="${selection_fields[0]}"
            selected_onnx="${selection_fields[1]}"
            selected_gate="${selection_fields[2]}"
            selected_summary="${selection_fields[3]}"
            copy_required_file "$selected_onnx" "$STYLE_CANDIDATE_ONNX"
            if [[ -n "$selected_gate" ]]; then
                copy_required_file "$selected_gate" "$STYLE_EVAL_DIR/candidate_gate.json"
            fi
            if [[ -n "$selected_summary" ]]; then
                copy_required_file "$selected_summary" "$STYLE_EVAL_DIR/candidate_eval_summary.json"
            fi
        fi

        iter_score_margin="0.0"
        iter_accepted=0
        if (( SKIP_ONNX_EXPORT == 0 && SKIP_EVAL == 0 )); then
            if [[ "$CANDIDATE_SELECTION_MODE" == "final" ]]; then
                run_candidate_eval "$STYLE_CANDIDATE_ONNX" "$STYLE_EVAL_DIR" "$BASELINE_ONNX"
            fi

            iter_score_margin="$("${PYTHON_CMD[@]}" - "$STYLE_EVAL_DIR/candidate_gate.json" <<'PY'
import json
import sys
from pathlib import Path

gate = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
margin = gate["candidate"]["avg_score_delta"] - gate["baseline"]["avg_score_delta"]
print(f"{margin:.4f}")
PY
)"
            iter_accepted="$("${PYTHON_CMD[@]}" - "$STYLE_EVAL_DIR/candidate_gate.json" <<'PY'
import json
import sys
from pathlib import Path

gate = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
print("1" if gate.get("accepted") else "0")
PY
)"

            echo "  Iteration $iter style=$style: score_margin=$iter_score_margin accepted=$iter_accepted"

            if (( iter_accepted == 1 )) || "${PYTHON_CMD[@]}" -c "exit(0 if float('$iter_score_margin') > float('${best_score_margin_by_style[$style]}') else 1)"; then
                best_score_margin_by_style["$style"]="$iter_score_margin"
                best_checkpoint_by_style["$style"]="$selected_checkpoint"
                best_onnx_by_style["$style"]="$STYLE_CANDIDATE_ONNX"
                best_iter_by_style["$style"]="$iter"
                current_checkpoint_by_style["$style"]="$selected_checkpoint"
                current_onnx_by_style["$style"]="$STYLE_CANDIDATE_ONNX"
                echo "  Style $style advanced (score_margin=$iter_score_margin, accepted=$iter_accepted)"
            else
                echo "  Style $style kept current best (best_score_margin=${best_score_margin_by_style[$style]})"
            fi
        fi

        "${PYTHON_CMD[@]}" - "${history_file_by_style[$style]}" "$iter" "$style" "$selected_checkpoint" "$STYLE_CANDIDATE_ONNX" "$iter_score_margin" "$iter_accepted" <<'PY'
import json
import sys
from pathlib import Path

output = Path(sys.argv[1])
entry = {
    "iteration": int(sys.argv[2]),
    "play_style": sys.argv[3],
    "checkpoint": sys.argv[4],
    "onnx": sys.argv[5],
    "score_margin": float(sys.argv[6]),
    "accepted": sys.argv[7] == "1",
}
with output.open("a", encoding="utf-8") as handle:
    handle.write(json.dumps(entry, ensure_ascii=False) + "\n")
PY
    done
done

# ── Finalize: copy each play style's own best result ─────────────────────
any_accepted=0
for style in "${ACTIVE_PLAY_STYLES[@]}"; do
    set_final_style_paths "$style"
    mkdir -p "$FINAL_STYLE_CHECKPOINT_DIR"
    copy_required_file "${best_checkpoint_by_style[$style]}" "$FINAL_STYLE_CHECKPOINT_DIR/best.pt"
    if (( SKIP_ONNX_EXPORT == 0 )); then
        copy_required_file "${best_onnx_by_style[$style]}" "$FINAL_STYLE_CANDIDATE_ONNX"
    fi

    best_iter="${best_iter_by_style[$style]}"
    if (( best_iter > 0 )); then
        printf -v best_iter_tag "iter_%03d" "$best_iter"
        best_iter_dir="$OUTPUT_DIR/$best_iter_tag"
        set_style_paths "$best_iter_dir" "$style"
        best_iter_eval_dir="$STYLE_EVAL_DIR"
        if [[ -f "$best_iter_eval_dir/candidate_eval_summary.json" ]]; then
            copy_required_file "$best_iter_eval_dir/candidate_eval_summary.json" "$FINAL_STYLE_EVAL_SUMMARY"
        fi
        if [[ -f "$best_iter_eval_dir/candidate_gate.json" ]]; then
            copy_required_file "$best_iter_eval_dir/candidate_gate.json" "$FINAL_STYLE_GATE_OUTPUT"
        fi
    else
        best_iter_tag="baseline"
        last_iter="$("${PYTHON_CMD[@]}" - "${history_file_by_style[$style]}" <<'PY'
import json
import sys
from pathlib import Path

rows = [json.loads(line) for line in Path(sys.argv[1]).read_text(encoding="utf-8").splitlines() if line.strip()]
print(rows[-1]["iteration"] if rows else "")
PY
)"
        if [[ -n "$last_iter" ]]; then
            printf -v last_iter_tag "iter_%03d" "$last_iter"
            last_iter_dir="$OUTPUT_DIR/$last_iter_tag"
            set_style_paths "$last_iter_dir" "$style"
            last_iter_eval_dir="$STYLE_EVAL_DIR"
            if [[ -f "$last_iter_eval_dir/candidate_eval_summary.json" ]]; then
                copy_required_file "$last_iter_eval_dir/candidate_eval_summary.json" "$FINAL_STYLE_EVAL_SUMMARY"
            fi
            if [[ -f "$last_iter_eval_dir/candidate_gate.json" ]]; then
                copy_required_file "$last_iter_eval_dir/candidate_gate.json" "$FINAL_STYLE_GATE_OUTPUT"
            fi
        fi
    fi

    "${PYTHON_CMD[@]}" - "${history_file_by_style[$style]}" "$FINAL_STYLE_HISTORY" <<'PY'
import json
import sys
from pathlib import Path

rows = [json.loads(line) for line in Path(sys.argv[1]).read_text(encoding="utf-8").splitlines() if line.strip()]
Path(sys.argv[2]).write_text(json.dumps(rows, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
PY

    style_accepted="$("${PYTHON_CMD[@]}" - "$FINAL_STYLE_HISTORY" <<'PY'
import json
import sys
from pathlib import Path

rows = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
print("1" if any(row.get("accepted") for row in rows) else "0")
PY
)"
    if (( style_accepted == 1 )); then
        any_accepted=1
    elif (( ENFORCE_CANDIDATE_GATE == 0 )); then
        echo "No iteration passed the candidate gate for play_style=$style. Best score_margin=${best_score_margin_by_style[$style]}. See $FINAL_STYLE_GATE_OUTPUT" >&2
    fi
done

if (( MULTI_STYLE_TRAINING == 1 )); then
    history_args=("$OUTPUT_DIR/iteration_history.json")
    for style in "${ACTIVE_PLAY_STYLES[@]}"; do
        set_final_style_paths "$style"
        history_args+=("$style" "$FINAL_STYLE_HISTORY")
    done
    "${PYTHON_CMD[@]}" - "${history_args[@]}" <<'PY'
import json
import sys
from pathlib import Path

output = Path(sys.argv[1])
pairs = sys.argv[2:]
styles = {}
for index in range(0, len(pairs), 2):
    style = pairs[index]
    history_path = Path(pairs[index + 1])
    styles[style] = json.loads(history_path.read_text(encoding="utf-8"))
output.write_text(
    json.dumps({"trajectory_scope": "per_play_style", "styles": styles}, indent=2, ensure_ascii=False) + "\n",
    encoding="utf-8",
)
PY
fi

if (( ENFORCE_CANDIDATE_GATE == 1 && any_accepted == 0 )); then
    echo "No iteration passed the candidate gate." >&2
    exit 1
fi

echo ""
echo "RL iterative self-play pipeline finished."
echo "Iterations:     $ITERATIONS"
for style in "${ACTIVE_PLAY_STYLES[@]}"; do
    set_final_style_paths "$style"
    best_iter="${best_iter_by_style[$style]}"
    if (( best_iter > 0 )); then
        printf -v best_iter_tag "iter_%03d" "$best_iter"
    else
        best_iter_tag="baseline"
    fi
    echo "[$style] Best iteration: $best_iter_tag (score_margin=${best_score_margin_by_style[$style]})"
    echo "[$style] Checkpoint:     $FINAL_STYLE_CHECKPOINT_DIR/best.pt"
    echo "[$style] Candidate:      $FINAL_STYLE_CANDIDATE_ONNX"
done
echo "History:        $OUTPUT_DIR/iteration_history.json"
