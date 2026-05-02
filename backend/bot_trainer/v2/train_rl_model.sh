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

run_candidate_eval() {
    local candidate_model="$1"
    local eval_dir="$2"
    local eval_baseline_onnx="$3"
    mkdir -p "$eval_dir"
    "${PYTHON_CMD[@]}" backend/bot_trainer/v2/league_config.py \
        --pool "$OPPONENT_POOL" \
        --output-dir "$eval_dir" \
        --matches "$EVAL_MATCHES" \
        --seed "$SEED" \
        --max-actions "$MAX_ACTIONS_PER_MATCH" \
        --mode eval \
        --candidate-onnx "$candidate_model" \
        --baseline-onnx "$eval_baseline_onnx"

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
current_onnx="$BASELINE_ONNX"
current_checkpoint="$BASELINE_CHECKPOINT"
best_onnx="$BASELINE_ONNX"
best_checkpoint="$BASELINE_CHECKPOINT"
best_score_margin="0.0"
best_iter=0

iteration_results=()

for (( iter = 1; iter <= ITERATIONS; iter++ )); do
    printf -v iter_tag "iter_%03d" "$iter"
    iter_dir="$OUTPUT_DIR/$iter_tag"
    iter_trajectory_config_dir="$iter_dir/trajectory_configs"
    iter_trajectory_jsonl="$iter_dir/trajectories.jsonl"
    iter_checkpoint_dir="$iter_dir/checkpoints"
    iter_candidate_onnx="$iter_dir/candidate.onnx"
    iter_eval_dir="$iter_dir/eval"
    iter_seed=$(( SEED + (iter - 1) * 1000000 ))

    echo ""
    echo "═══════════════════════════════════════════════════════════════"
    echo "  Iteration $iter / $ITERATIONS  (rollout model: $(basename "$current_onnx"))"
    echo "═══════════════════════════════════════════════════════════════"

    # ── Step 1: Generate trajectories with current model ──────────────
    mkdir -p "$iter_trajectory_config_dir"
    "${PYTHON_CMD[@]}" backend/bot_trainer/v2/league_config.py \
        --pool "$OPPONENT_POOL" \
        --output-dir "$iter_trajectory_config_dir" \
        --matches "$ITERATION_MATCHES" \
        --seed "$iter_seed" \
        --max-actions "$MAX_ACTIONS_PER_MATCH" \
        --mode trajectory \
        --rollout-onnx "$current_onnx"

    trajectory_files=()
    for config_path in "$iter_trajectory_config_dir"/trajectory_config_*.json; do
        [[ -e "$config_path" ]] || continue
        config_name="$(basename "$config_path" .json)"
        index="${config_name#trajectory_config_}"
        partial_report="$iter_dir/trajectory_arena_report_$index.jsonl"
        partial_trajectory="$iter_dir/trajectories_$index.jsonl"
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

    # ── Step 2: PPO training from current checkpoint ─────────────────
    rl_train_args=(
        backend/bot_trainer/v2/rl_train.py
        --trajectories "$iter_trajectory_jsonl"
        --checkpoint "$current_checkpoint"
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
        --output "$iter_checkpoint_dir"
        --device "$DEVICE"
    )
    if (( ENTROPY_DECAY_STEPS > 0 )); then
        rl_train_args+=(--entropy-decay-steps "$ENTROPY_DECAY_STEPS")
    fi
    if (( RECOMPUTE_OLD_POLICY_STATS == 1 )); then
        rl_train_args+=(--recompute-old-policy-stats)
    fi
    "${PYTHON_CMD[@]}" "${rl_train_args[@]}"

    # ── Step 3: Export ONNX ──────────────────────────────────────────
    iter_best_pt="$iter_checkpoint_dir/best.pt"
    selected_checkpoint="$iter_best_pt"
    selected_onnx="$iter_candidate_onnx"
    if (( SKIP_ONNX_EXPORT == 0 )); then
        "${PYTHON_CMD[@]}" backend/bot_trainer/v2/export_onnx.py \
            --checkpoint "$iter_best_pt" \
            --output "$iter_candidate_onnx"
    fi

    if [[ "$CANDIDATE_SELECTION_MODE" == "epoch" && $SKIP_ONNX_EXPORT == 0 && $SKIP_EVAL == 0 ]]; then
        candidate_entries_jsonl="$iter_dir/candidate_entries.jsonl"
        candidate_manifest="$iter_dir/candidate_manifest.json"
        candidate_selection="$iter_dir/candidate_selection.json"
        : > "$candidate_entries_jsonl"
        for epoch_pt in "$iter_checkpoint_dir"/epoch_*.pt; do
            [[ -e "$epoch_pt" ]] || continue
            epoch_name="$(basename "$epoch_pt" .pt)"
            epoch_number="${epoch_name#epoch_}"
            epoch_onnx="$iter_dir/$epoch_name.onnx"
            epoch_eval_dir="$iter_eval_dir/$epoch_name"
            "${PYTHON_CMD[@]}" backend/bot_trainer/v2/export_onnx.py \
                --checkpoint "$epoch_pt" \
                --output "$epoch_onnx"
            run_candidate_eval "$epoch_onnx" "$epoch_eval_dir" "$BASELINE_ONNX"
            "${PYTHON_CMD[@]}" - "$candidate_entries_jsonl" "$epoch_number" "$epoch_pt" "$epoch_onnx" "$RUN_EVAL_SUMMARY" "$RUN_EVAL_GATE" <<'PY'
import json
import sys
from pathlib import Path

output = Path(sys.argv[1])
entry = {
    "epoch": int(sys.argv[2]),
    "checkpoint": sys.argv[3],
    "onnx": sys.argv[4],
    "summary": sys.argv[5],
    "gate_path": sys.argv[6],
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
        copy_required_file "$selected_onnx" "$iter_candidate_onnx"
        if [[ -n "$selected_gate" ]]; then
            copy_required_file "$selected_gate" "$iter_eval_dir/candidate_gate.json"
        fi
        if [[ -n "$selected_summary" ]]; then
            copy_required_file "$selected_summary" "$iter_eval_dir/candidate_eval_summary.json"
        fi
    fi

    # ── Step 4: Evaluate candidate vs original baseline ──────────────
    iter_score_margin="0.0"
    iter_accepted=0

    if (( SKIP_ONNX_EXPORT == 0 && SKIP_EVAL == 0 )); then
        if [[ "$CANDIDATE_SELECTION_MODE" == "final" ]]; then
            run_candidate_eval "$iter_candidate_onnx" "$iter_eval_dir" "$BASELINE_ONNX"
        fi

        iter_score_margin="$("${PYTHON_CMD[@]}" - "$iter_eval_dir/candidate_gate.json" <<'PY'
import json
import sys
from pathlib import Path

gate = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
margin = gate["candidate"]["avg_score_delta"] - gate["baseline"]["avg_score_delta"]
print(f"{margin:.4f}")
PY
)"
        iter_accepted="$("${PYTHON_CMD[@]}" - "$iter_eval_dir/candidate_gate.json" <<'PY'
import json
import sys
from pathlib import Path

gate = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
print("1" if gate.get("accepted") else "0")
PY
)"
        if (( iter_accepted == 1 )); then
            iter_accepted=1
        fi

        echo "  Iteration $iter result: score_margin=$iter_score_margin accepted=$iter_accepted"

        if (( iter_accepted == 1 )) || "${PYTHON_CMD[@]}" -c "exit(0 if float('$iter_score_margin') > float('$best_score_margin') else 1)"; then
            best_score_margin="$iter_score_margin"
            best_checkpoint="$selected_checkpoint"
            best_onnx="$iter_candidate_onnx"
            best_iter="$iter"
            current_checkpoint="$selected_checkpoint"
            current_onnx="$iter_candidate_onnx"
            echo "  Rollout advanced to candidate (score_margin=$iter_score_margin, accepted=$iter_accepted)"
        else
            echo "  Rollout kept current best (best_score_margin=$best_score_margin)"
        fi
    fi

    iteration_results+=("$iter|$iter_score_margin|$iter_accepted")
done

# ── Finalize: copy best results to top-level ──────────────────────────────
FINAL_CANDIDATE_ONNX="$OUTPUT_DIR/candidate.onnx"
FINAL_CHECKPOINT_DIR="$OUTPUT_DIR/checkpoints"
FINAL_EVAL_SUMMARY="$OUTPUT_DIR/candidate_eval_summary.json"
FINAL_GATE_OUTPUT="$OUTPUT_DIR/candidate_gate.json"

mkdir -p "$FINAL_CHECKPOINT_DIR"
copy_required_file "$best_checkpoint" "$FINAL_CHECKPOINT_DIR/best.pt"
if (( SKIP_ONNX_EXPORT == 0 )); then
    copy_required_file "$best_onnx" "$FINAL_CANDIDATE_ONNX"
fi

if (( best_iter > 0 )); then
    printf -v best_iter_tag "iter_%03d" "$best_iter"
    best_iter_eval_dir="$OUTPUT_DIR/$best_iter_tag/eval"
    if [[ -f "$best_iter_eval_dir/candidate_eval_summary.json" ]]; then
        copy_required_file "$best_iter_eval_dir/candidate_eval_summary.json" "$FINAL_EVAL_SUMMARY"
    fi
    if [[ -f "$best_iter_eval_dir/candidate_gate.json" ]]; then
        copy_required_file "$best_iter_eval_dir/candidate_gate.json" "$FINAL_GATE_OUTPUT"
    fi
else
    best_iter_tag="baseline"
    if (( ${#iteration_results[@]} > 0 )); then
        last_index=$(( ${#iteration_results[@]} - 1 ))
        last_iter="${iteration_results[$last_index]%%|*}"
        printf -v last_iter_tag "iter_%03d" "$last_iter"
        last_iter_eval_dir="$OUTPUT_DIR/$last_iter_tag/eval"
        if [[ -f "$last_iter_eval_dir/candidate_eval_summary.json" ]]; then
            copy_required_file "$last_iter_eval_dir/candidate_eval_summary.json" "$FINAL_EVAL_SUMMARY"
        fi
        if [[ -f "$last_iter_eval_dir/candidate_gate.json" ]]; then
            copy_required_file "$last_iter_eval_dir/candidate_gate.json" "$FINAL_GATE_OUTPUT"
        fi
    fi
fi

# Write iteration history
"${PYTHON_CMD[@]}" - "$OUTPUT_DIR/iteration_history.json" "${iteration_results[@]}" <<'PY'
import json
import sys
from pathlib import Path

output = Path(sys.argv[1])
entries = []
for row in sys.argv[2:]:
    parts = row.split("|", 2)
    entries.append({
        "iteration": int(parts[0]),
        "score_margin": float(parts[1]),
        "accepted": parts[2] == "1",
    })
output.write_text(json.dumps(entries, indent=2) + "\n", encoding="utf-8")
PY

any_accepted=0
for result in "${iteration_results[@]}"; do
    if [[ "$result" == *"|1" ]]; then
        any_accepted=1
        break
    fi
done

if (( ENFORCE_CANDIDATE_GATE == 1 && any_accepted == 0 )); then
    echo "No iteration passed the candidate gate." >&2
    exit 1
fi
if (( ENFORCE_CANDIDATE_GATE != 1 && any_accepted == 0 )); then
    echo "No iteration passed the candidate gate. Best score_margin=$best_score_margin. See $FINAL_GATE_OUTPUT" >&2
fi

echo ""
echo "RL iterative self-play pipeline finished."
echo "Iterations:     $ITERATIONS"
echo "Best iteration: $best_iter_tag (score_margin=$best_score_margin)"
echo "Checkpoint:     $best_checkpoint"
echo "Candidate:      $FINAL_CANDIDATE_ONNX"
echo "History:        $OUTPUT_DIR/iteration_history.json"
