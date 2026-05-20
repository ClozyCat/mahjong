#!/usr/bin/env bash
set -euo pipefail

OUTPUT_DIR="backend/bot_trainer/v2/rl_runs/$(date +%Y%m%d%H%M)"
BASELINE_CHECKPOINT="backend/bot_trainer/v2/checkpoints/best.pt"
BASELINE_ONNX="backend/assets/sft/sft.onnx"
PYTHON_CMD=(python)
CARGO_CMD=(cargo)
ARENA_JOBS=1
EPOCH_EVAL_JOBS=1
ITERATIONS=5
ITERATION_MATCHES=500
EVAL_MATCHES=1000
SEED=20260429
MAX_ACTIONS_PER_MATCH=2400
EPOCHS=1
BATCH_SIZE=256
LEARNING_RATE=0.000003
GAMMA=0.97
GAE_LAMBDA=0.95
CLIP_EPSILON=0.2
VALUE_CLIP_EPSILON=0.2
ENTROPY_COEF=0.03
ENTROPY_END_COEF=0.015
ENTROPY_DECAY_STEPS=0
KL_COEF=0.01
KL_END_COEF=0.005
TARGET_KL=0.03
POLICY=ppo
POLICIES=""
USE_ACTOR_CRITIC=0
CRITIC_LR_MULTIPLIER=2.0
DEVICE=auto
OPPONENT_POOL="backend/bot_trainer/v2/opponent_pool.json"
LEARNER_POLICY_ID="learner"
SKIP_TESTS=0
SKIP_ONNX_EXPORT=0
SKIP_EVAL=0
ENFORCE_CANDIDATE_GATE=0
ALLOW_RL_BASELINE_CHECKPOINT=0
RECOMPUTE_OLD_POLICY_STATS=0
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
  --arena-jobs N                   Parallel jobs inside bot_arena. Use 0 for all available cores.
  --epoch-eval-jobs N              Parallel epoch candidate evaluations. Default 1.
  --iterations N                   Number of self-play iterations. Default 5.
  --iteration-matches N            Matches per iteration for trajectory generation. Default 500.
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
  --entropy-coef VALUE             PPO entropy coefficient. Default 0.03.
  --entropy-end-coef VALUE         PPO final entropy coefficient after decay. Default 0.015.
  --entropy-decay-steps N          Linear entropy decay steps. Use 0 for full training.
  --kl-coef VALUE                  Supervised policy KL coefficient. Default 0.01.
  --kl-end-coef VALUE              Final KL coefficient after decay. Default 0.005.
  --target-kl VALUE                Stop PPO epoch loop when approximate KL exceeds this value.
  --policy NAME                    Policy name. Only ppo is valid.
  --policies LIST                  Comma-separated policies. Only ppo is valid.
  --use-actor-critic               Train separate actor and global-information critic.
  --critic-lr-multiplier VALUE     Critic learning-rate multiplier when actor-critic is enabled. Default 2.0.
  --device DEVICE                  auto, cpu, cuda, etc.
  --opponent-pool PATH             Opponent pool JSON for league rollout.
  --learner-policy-id ID           Policy id filtered for PPO training.
  --skip-tests                     Skip Python tests.
  --skip-onnx-export               Do not export candidate.onnx.
  --skip-eval                      Do not run baseline vs candidate arena evaluation.
  --enforce-candidate-gate         Exit non-zero when no iteration passes candidate gate.
  --allow-rl-baseline-checkpoint   Allow intentionally continuing from an RL checkpoint.
  --recompute-old-policy-stats     Recompute old log-probs and values from checkpoint.
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

write_candidate_entry() {
    local output_path="$1"
    local policy="$2"
    local epoch_number="$3"
    local checkpoint_path="$4"
    local onnx_path="$5"
    local summary_path="$6"
    local gate_path="$7"

    "${PYTHON_CMD[@]}" - "$output_path" "$policy" "$epoch_number" "$checkpoint_path" "$onnx_path" "$summary_path" "$gate_path" <<'PY'
import json
import sys
from pathlib import Path

output = Path(sys.argv[1])
entry = {
    "policy": sys.argv[2],
    "epoch": int(sys.argv[3]),
    "checkpoint": sys.argv[4],
    "onnx": sys.argv[5],
    "summary": sys.argv[6],
    "gate_path": sys.argv[7],
}
with output.open("a", encoding="utf-8") as handle:
    handle.write(json.dumps(entry, ensure_ascii=False) + "\n")
PY
}

run_epoch_candidate_eval() {
    local epoch_pt="$1"
    local policy="$2"
    local policy_dir="$3"
    local policy_eval_dir="$4"
    local baseline_onnx="$5"
    local entry_path="$6"

    local epoch_name
    epoch_name="$(basename "$epoch_pt" .pt)"
    local epoch_number="${epoch_name#epoch_}"
    local epoch_onnx="$policy_dir/$epoch_name.onnx"
    local epoch_eval_dir="$policy_eval_dir/$epoch_name"
    "${PYTHON_CMD[@]}" backend/bot_trainer/v2/export_onnx.py \
        --checkpoint "$epoch_pt" \
        --output "$epoch_onnx"
    run_candidate_eval "$epoch_onnx" "$epoch_eval_dir" "$baseline_onnx"
    write_candidate_entry "$entry_path" "$policy" "$epoch_number" "$epoch_pt" "$epoch_onnx" "$RUN_EVAL_SUMMARY" "$RUN_EVAL_GATE"
}

is_valid_policy() {
    [[ "$1" == "ppo" ]]
}

contains_policy() {
    local needle="$1"
    shift
    local policy
    for policy in "$@"; do
        [[ "$policy" == "$needle" ]] && return 0
    done
    return 1
}

set_policy_paths() {
    local iter_dir="$1"
    local policy="$2"
    if (( MULTI_POLICY_TRAINING == 1 )); then
        POLICY_DIR="$iter_dir/policies/$policy"
    else
        POLICY_DIR="$iter_dir"
    fi
    POLICY_CHECKPOINT_DIR="$POLICY_DIR/checkpoints"
    POLICY_CANDIDATE_ONNX="$POLICY_DIR/candidate.onnx"
    POLICY_EVAL_DIR="$POLICY_DIR/eval"
}

set_final_policy_paths() {
    local policy="$1"
    if (( MULTI_POLICY_TRAINING == 1 )); then
        FINAL_POLICY_DIR="$OUTPUT_DIR/policies/$policy"
    else
        FINAL_POLICY_DIR="$OUTPUT_DIR"
    fi
    FINAL_POLICY_CANDIDATE_ONNX="$FINAL_POLICY_DIR/candidate.onnx"
    FINAL_POLICY_CHECKPOINT_DIR="$FINAL_POLICY_DIR/checkpoints"
    FINAL_POLICY_EVAL_SUMMARY="$FINAL_POLICY_DIR/candidate_eval_summary.json"
    FINAL_POLICY_GATE_OUTPUT="$FINAL_POLICY_DIR/candidate_gate.json"
    FINAL_POLICY_HISTORY="$FINAL_POLICY_DIR/iteration_history.json"
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
        --epoch-eval-jobs)
            require_value "$1" "${2:-}"
            EPOCH_EVAL_JOBS="$2"
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
        --policy)
            require_value "$1" "${2:-}"
            POLICY="$2"
            shift 2
            ;;
        --policies)
            require_value "$1" "${2:-}"
            POLICIES="$2"
            shift 2
            ;;
        --use-actor-critic)
            USE_ACTOR_CRITIC=1
            shift
            ;;
        --critic-lr-multiplier)
            require_value "$1" "${2:-}"
            CRITIC_LR_MULTIPLIER="$2"
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

if [[ "$CANDIDATE_SELECTION_MODE" != "epoch" && "$CANDIDATE_SELECTION_MODE" != "final" ]]; then
    echo "--candidate-selection-mode must be epoch or final." >&2
    exit 2
fi
if ! is_valid_policy "$POLICY"; then
    echo "--policy must be ppo." >&2
    exit 2
fi
ACTIVE_POLICIES=()
if [[ -n "$POLICIES" ]]; then
    IFS=',' read -ra requested_policies <<< "$POLICIES"
else
    requested_policies=("$POLICY")
fi
for requested_policy in "${requested_policies[@]}"; do
    requested_policy="${requested_policy//[[:space:]]/}"
    [[ -n "$requested_policy" ]] || continue
    if ! is_valid_policy "$requested_policy"; then
        echo "--policies contains invalid policy: $requested_policy" >&2
        exit 2
    fi
    if ! contains_policy "$requested_policy" "${ACTIVE_POLICIES[@]}"; then
        ACTIVE_POLICIES+=("$requested_policy")
    fi
done
if (( ${#ACTIVE_POLICIES[@]} == 0 )); then
    echo "No policies selected." >&2
    exit 2
fi
MULTI_POLICY_TRAINING=0
if (( ${#ACTIVE_POLICIES[@]} > 1 )); then
    MULTI_POLICY_TRAINING=1
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
echo "KL coef start/end:  $KL_COEF / $KL_END_COEF"
echo "Entropy start/end:  $ENTROPY_COEF / $ENTROPY_END_COEF"
echo "Actor-critic:       $([[ $USE_ACTOR_CRITIC == 1 ]] && echo true || echo false)"
echo "Critic LR x:        $CRITIC_LR_MULTIPLIER"
echo "Opponent pool:       $OPPONENT_POOL"
echo "Learner policy id:   $LEARNER_POLICY_ID"
echo "Eval matches:        $EVAL_MATCHES"
echo "Device:              $DEVICE"
echo "Policies:            ${ACTIVE_POLICIES[*]}"
echo "Python:              ${PYTHON_CMD[*]}"
echo "Cargo:               ${CARGO_CMD[*]}"
echo "Arena jobs:          $ARENA_JOBS"
echo "Epoch eval jobs:     $EPOCH_EVAL_JOBS"
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

checkpoint_architecture_guard_args=(
    backend/bot_trainer/v2/checkpoint_architecture_guard.py
    --checkpoint "$BASELINE_CHECKPOINT"
)
if (( USE_ACTOR_CRITIC == 1 )); then
    checkpoint_architecture_guard_args+=(--use-actor-critic)
fi
"${PYTHON_CMD[@]}" "${checkpoint_architecture_guard_args[@]}"

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
declare -A current_onnx_by_policy=()
declare -A current_checkpoint_by_policy=()
declare -A best_onnx_by_policy=()
declare -A best_checkpoint_by_policy=()
declare -A best_score_margin_by_policy=()
declare -A best_iter_by_policy=()
declare -A history_file_by_policy=()

for policy in "${ACTIVE_POLICIES[@]}"; do
    current_onnx_by_policy["$policy"]="$BASELINE_ONNX"
    current_checkpoint_by_policy["$policy"]="$BASELINE_CHECKPOINT"
    best_onnx_by_policy["$policy"]="$BASELINE_ONNX"
    best_checkpoint_by_policy["$policy"]="$BASELINE_CHECKPOINT"
    best_score_margin_by_policy["$policy"]="0.0"
    best_iter_by_policy["$policy"]=0
    history_file_by_policy["$policy"]="$OUTPUT_DIR/${policy}_iteration_results.jsonl"
    : > "${history_file_by_policy[$policy]}"
done

for (( iter = 1; iter <= ITERATIONS; iter++ )); do
    printf -v iter_tag "iter_%03d" "$iter"
    iter_dir="$OUTPUT_DIR/$iter_tag"
    iter_seed=$(( SEED + (iter - 1) * 1000000 ))

    echo ""
    echo "═══════════════════════════════════════════════════════════════"
    echo "  Iteration $iter / $ITERATIONS"
    echo "═══════════════════════════════════════════════════════════════"

    # ── Step 1/2/3/4: Generate trajectories, train, export, and evaluate each policy serially ─
    for policy in "${ACTIVE_POLICIES[@]}"; do
        set_policy_paths "$iter_dir" "$policy"
        mkdir -p "$POLICY_CHECKPOINT_DIR" "$POLICY_DIR"
        iter_trajectory_config_dir="$POLICY_DIR/trajectory_configs"
        iter_trajectory_jsonl="$POLICY_DIR/trajectories.jsonl"
        rollout_onnx="${current_onnx_by_policy[$policy]}"
        echo "  Generating trajectories: policy=$policy rollout=$(basename "$rollout_onnx")"
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
        "${PYTHON_CMD[@]}" "${trajectory_config_args[@]}"

        trajectory_config_path="$iter_trajectory_config_dir/trajectory_config_0.json"
        if [[ ! -f "$trajectory_config_path" ]]; then
            echo "No trajectory config generated at $trajectory_config_path" >&2
            exit 2
        fi
        trajectory_report="$POLICY_DIR/trajectory_arena_report.jsonl"
        arena_args=(
            run --manifest-path backend/Cargo.toml --release --bin bot_arena --
            --config "$trajectory_config_path"
            --output "$trajectory_report"
            --trajectories "$iter_trajectory_jsonl"
            --jobs "$ARENA_JOBS"
        )
        "${CARGO_CMD[@]}" "${arena_args[@]}"

        rl_train_args=(
            backend/bot_trainer/v2/rl_train.py
            --trajectories "$iter_trajectory_jsonl"
            --checkpoint "${current_checkpoint_by_policy[$policy]}"
            --epochs "$EPOCHS"
            --batch-size "$BATCH_SIZE"
            --lr "$LEARNING_RATE"
            --critic-lr-multiplier "$CRITIC_LR_MULTIPLIER"
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
            --policy "$policy"
            --output "$POLICY_CHECKPOINT_DIR"
            --device "$DEVICE"
        )
        if (( ENTROPY_DECAY_STEPS > 0 )); then
            rl_train_args+=(--entropy-decay-steps "$ENTROPY_DECAY_STEPS")
        fi
        if (( USE_ACTOR_CRITIC == 1 )); then
            rl_train_args+=(--use-actor-critic)
        fi
        if (( RECOMPUTE_OLD_POLICY_STATS == 1 )); then
            rl_train_args+=(--recompute-old-policy-stats)
        fi
        echo "  Starting PPO training: policy=$policy"
        "${PYTHON_CMD[@]}" "${rl_train_args[@]}"
        echo "  PPO training finished: policy=$policy"

        iter_best_pt="$POLICY_CHECKPOINT_DIR/best.pt"
        selected_checkpoint="$iter_best_pt"
        selected_onnx="$POLICY_CANDIDATE_ONNX"
        if (( SKIP_ONNX_EXPORT == 0 )); then
            "${PYTHON_CMD[@]}" backend/bot_trainer/v2/export_onnx.py \
                --checkpoint "$iter_best_pt" \
                --output "$POLICY_CANDIDATE_ONNX"
        fi

        if [[ "$CANDIDATE_SELECTION_MODE" == "epoch" && $SKIP_ONNX_EXPORT == 0 && $SKIP_EVAL == 0 ]]; then
            candidate_entries_jsonl="$POLICY_DIR/candidate_entries.jsonl"
            candidate_manifest="$POLICY_DIR/candidate_manifest.json"
            candidate_selection="$POLICY_DIR/candidate_selection.json"
            candidate_entries_dir="$POLICY_DIR/candidate_entries"
            rm -rf "$candidate_entries_dir"
            mkdir -p "$candidate_entries_dir"
            epoch_eval_pids=()
            epoch_eval_status=0
            for epoch_pt in "$POLICY_CHECKPOINT_DIR"/epoch_*.pt; do
                [[ -e "$epoch_pt" ]] || continue
                epoch_name="$(basename "$epoch_pt" .pt)"
                epoch_number="${epoch_name#epoch_}"
                entry_path="$candidate_entries_dir/$epoch_name.jsonl"
                if (( EPOCH_EVAL_JOBS <= 1 )); then
                    run_epoch_candidate_eval "$epoch_pt" "$policy" "$POLICY_DIR" "$POLICY_EVAL_DIR" "$BASELINE_ONNX" "$entry_path"
                else
                    run_epoch_candidate_eval "$epoch_pt" "$policy" "$POLICY_DIR" "$POLICY_EVAL_DIR" "$BASELINE_ONNX" "$entry_path" &
                    epoch_eval_pids+=("$!")
                    if (( ${#epoch_eval_pids[@]} >= EPOCH_EVAL_JOBS )); then
                        wait "${epoch_eval_pids[0]}" || epoch_eval_status=1
                        epoch_eval_pids=("${epoch_eval_pids[@]:1}")
                    fi
                fi
            done
            for pid in "${epoch_eval_pids[@]}"; do
                wait "$pid" || epoch_eval_status=1
            done
            if (( epoch_eval_status != 0 )); then
                echo "One or more epoch candidate evaluations failed." >&2
                exit 1
            fi
            cat "$candidate_entries_dir"/epoch_*.jsonl > "$candidate_entries_jsonl"
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
            copy_required_file "$selected_onnx" "$POLICY_CANDIDATE_ONNX"
            if [[ -n "$selected_gate" ]]; then
                copy_required_file "$selected_gate" "$POLICY_EVAL_DIR/candidate_gate.json"
            fi
            if [[ -n "$selected_summary" ]]; then
                copy_required_file "$selected_summary" "$POLICY_EVAL_DIR/candidate_eval_summary.json"
            fi
        fi

        iter_score_margin="0.0"
        iter_accepted=0
        if (( SKIP_ONNX_EXPORT == 0 && SKIP_EVAL == 0 )); then
            if [[ "$CANDIDATE_SELECTION_MODE" == "final" ]]; then
                run_candidate_eval "$POLICY_CANDIDATE_ONNX" "$POLICY_EVAL_DIR" "$BASELINE_ONNX"
            fi

            iter_score_margin="$("${PYTHON_CMD[@]}" - "$POLICY_EVAL_DIR/candidate_gate.json" <<'PY'
import json
import sys
from pathlib import Path

gate = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
margin = gate["candidate"]["avg_score_delta"] - gate["baseline"]["avg_score_delta"]
print(f"{margin:.4f}")
PY
)"
            iter_accepted="$("${PYTHON_CMD[@]}" - "$POLICY_EVAL_DIR/candidate_gate.json" <<'PY'
import json
import sys
from pathlib import Path

gate = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
print("1" if gate.get("accepted") else "0")
PY
)"

            echo "  Iteration $iter policy=$policy: score_margin=$iter_score_margin accepted=$iter_accepted"

            if (( iter_accepted == 1 )); then
                best_score_margin_by_policy["$policy"]="$iter_score_margin"
                best_checkpoint_by_policy["$policy"]="$selected_checkpoint"
                best_onnx_by_policy["$policy"]="$POLICY_CANDIDATE_ONNX"
                best_iter_by_policy["$policy"]="$iter"
                current_checkpoint_by_policy["$policy"]="$selected_checkpoint"
                current_onnx_by_policy["$policy"]="$POLICY_CANDIDATE_ONNX"
                echo "  Policy $policy advanced: candidate accepted (score_margin=$iter_score_margin)"
            elif (( $(echo "$iter_score_margin > ${best_score_margin_by_policy[$policy]}" | bc -l) )); then
                best_score_margin_by_policy["$policy"]="$iter_score_margin"
                best_checkpoint_by_policy["$policy"]="$selected_checkpoint"
                best_onnx_by_policy["$policy"]="$POLICY_CANDIDATE_ONNX"
                best_iter_by_policy["$policy"]="$iter"
                echo "  Policy $policy: candidate rejected but score_margin improved (rollout NOT updated, score_margin=$iter_score_margin)"
            else
                echo "  Policy $policy kept current best (best_score_margin=${best_score_margin_by_policy[$policy]})"
            fi
        fi

        "${PYTHON_CMD[@]}" - "${history_file_by_policy[$policy]}" "$iter" "$policy" "$selected_checkpoint" "$POLICY_CANDIDATE_ONNX" "$iter_score_margin" "$iter_accepted" <<'PY'
import json
import sys
from pathlib import Path

output = Path(sys.argv[1])
entry = {
    "iteration": int(sys.argv[2]),
    "policy": sys.argv[3],
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

# ── Finalize: copy each policy's own best result ─────────────────────
any_accepted=0
for policy in "${ACTIVE_POLICIES[@]}"; do
    set_final_policy_paths "$policy"
    mkdir -p "$FINAL_POLICY_CHECKPOINT_DIR"
    copy_required_file "${best_checkpoint_by_policy[$policy]}" "$FINAL_POLICY_CHECKPOINT_DIR/best.pt"
    if (( SKIP_ONNX_EXPORT == 0 )); then
        copy_required_file "${best_onnx_by_policy[$policy]}" "$FINAL_POLICY_CANDIDATE_ONNX"
    fi

    best_iter="${best_iter_by_policy[$policy]}"
    if (( best_iter > 0 )); then
        printf -v best_iter_tag "iter_%03d" "$best_iter"
        best_iter_dir="$OUTPUT_DIR/$best_iter_tag"
        set_policy_paths "$best_iter_dir" "$policy"
        best_iter_eval_dir="$POLICY_EVAL_DIR"
        if [[ -f "$best_iter_eval_dir/candidate_eval_summary.json" ]]; then
            copy_required_file "$best_iter_eval_dir/candidate_eval_summary.json" "$FINAL_POLICY_EVAL_SUMMARY"
        fi
        if [[ -f "$best_iter_eval_dir/candidate_gate.json" ]]; then
            copy_required_file "$best_iter_eval_dir/candidate_gate.json" "$FINAL_POLICY_GATE_OUTPUT"
        fi
    else
        best_iter_tag="baseline"
        last_iter="$("${PYTHON_CMD[@]}" - "${history_file_by_policy[$policy]}" <<'PY'
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
            set_policy_paths "$last_iter_dir" "$policy"
            last_iter_eval_dir="$POLICY_EVAL_DIR"
            if [[ -f "$last_iter_eval_dir/candidate_eval_summary.json" ]]; then
                copy_required_file "$last_iter_eval_dir/candidate_eval_summary.json" "$FINAL_POLICY_EVAL_SUMMARY"
            fi
            if [[ -f "$last_iter_eval_dir/candidate_gate.json" ]]; then
                copy_required_file "$last_iter_eval_dir/candidate_gate.json" "$FINAL_POLICY_GATE_OUTPUT"
            fi
        fi
    fi

    "${PYTHON_CMD[@]}" - "${history_file_by_policy[$policy]}" "$FINAL_POLICY_HISTORY" <<'PY'
import json
import sys
from pathlib import Path

rows = [json.loads(line) for line in Path(sys.argv[1]).read_text(encoding="utf-8").splitlines() if line.strip()]
Path(sys.argv[2]).write_text(json.dumps(rows, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
PY

    policy_accepted="$("${PYTHON_CMD[@]}" - "$FINAL_POLICY_HISTORY" <<'PY'
import json
import sys
from pathlib import Path

rows = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
print("1" if any(row.get("accepted") for row in rows) else "0")
PY
)"
    if (( policy_accepted == 1 )); then
        any_accepted=1
    elif (( ENFORCE_CANDIDATE_GATE == 0 )); then
        echo "No iteration passed the candidate gate for policy=$policy. Best score_margin=${best_score_margin_by_policy[$policy]}. See $FINAL_POLICY_GATE_OUTPUT" >&2
    fi
done

if (( MULTI_POLICY_TRAINING == 1 )); then
    history_args=("$OUTPUT_DIR/iteration_history.json")
    for policy in "${ACTIVE_POLICIES[@]}"; do
        set_final_policy_paths "$policy"
        history_args+=("$policy" "$FINAL_POLICY_HISTORY")
    done
    "${PYTHON_CMD[@]}" - "${history_args[@]}" <<'PY'
import json
import sys
from pathlib import Path

output = Path(sys.argv[1])
pairs = sys.argv[2:]
policies = {}
for index in range(0, len(pairs), 2):
    policy = pairs[index]
    history_path = Path(pairs[index + 1])
    policies[policy] = json.loads(history_path.read_text(encoding="utf-8"))
output.write_text(
    json.dumps({"trajectory_scope": "per_policy", "policies": policies}, indent=2, ensure_ascii=False) + "\n",
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
for policy in "${ACTIVE_POLICIES[@]}"; do
    set_final_policy_paths "$policy"
    best_iter="${best_iter_by_policy[$policy]}"
    if (( best_iter > 0 )); then
        printf -v best_iter_tag "iter_%03d" "$best_iter"
    else
        best_iter_tag="baseline"
    fi
    echo "[$policy] Best iteration: $best_iter_tag (score_margin=${best_score_margin_by_policy[$policy]})"
    echo "[$policy] Checkpoint:     $FINAL_POLICY_CHECKPOINT_DIR/best.pt"
    echo "[$policy] Candidate:      $FINAL_POLICY_CANDIDATE_ONNX"
done
echo "History:        $OUTPUT_DIR/iteration_history.json"
