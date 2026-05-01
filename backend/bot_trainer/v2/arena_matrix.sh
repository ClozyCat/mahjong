#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../../.." && pwd)"
BACKEND_MANIFEST="$REPO_ROOT/backend/Cargo.toml"
ORIGINAL_PWD="$(pwd)"

MATCHES="${MATCHES:-200}"
SEED="${SEED:-20260429}"
RANDOM_SEED="${RANDOM_SEED:-0}"
POLICY_POOL="${POLICY_POOL:-$SCRIPT_DIR/arena_policy_pool.json}"
OUTPUT_DIR="${OUTPUT_DIR:-$SCRIPT_DIR/arena_runs}"
PROGRESS_EVERY="${PROGRESS_EVERY:-10}"
JOBS="${JOBS:-0}"
MAX_ACTIONS_PER_MATCH="${MAX_ACTIONS_PER_MATCH:-2400}"
CARGO_EXE="${CARGO_EXE:-cargo}"

resolve_user_path() {
    local path="$1"
    case "$path" in
        /*|[A-Za-z]:*)
            printf '%s\n' "$path"
            ;;
        *)
            printf '%s/%s\n' "$ORIGINAL_PWD" "$path"
            ;;
    esac
}

POLICY_POOL="$(resolve_user_path "$POLICY_POOL")"
OUTPUT_DIR="$(resolve_user_path "$OUTPUT_DIR")"

mkdir -p "$OUTPUT_DIR"
OUTPUT_PATH="$OUTPUT_DIR/arena_results.jsonl"
POLICIES_PATH="$OUTPUT_DIR/arena_policies.json"

find_python() {
    if command -v python >/dev/null 2>&1; then
        printf '%s\n' python
    elif command -v python3 >/dev/null 2>&1; then
        printf '%s\n' python3
    else
        echo "Python was not found; cannot read policy pool JSON." >&2
        exit 2
    fi
}

PYTHON_BIN="$(find_python)"

SEAT_POLICY_IDS_TEXT="$("$PYTHON_BIN" - "$POLICY_POOL" "$POLICIES_PATH" <<'PY'
import json
import sys
from pathlib import Path

pool_path = Path(sys.argv[1])
policies_path = Path(sys.argv[2])

if not pool_path.is_file():
    print(f"Policy pool was not found: {pool_path}", file=sys.stderr)
    raise SystemExit(2)

pool = json.loads(pool_path.read_text(encoding="utf-8"))
if "policies" in pool:
    raw_policies = pool["policies"]
elif "learner" in pool and "opponents" in pool:
    raw_policies = [pool["learner"], *pool["opponents"]]
else:
    print(
        f"Policy pool must contain either 'policies' or 'learner' plus 'opponents': {pool_path}",
        file=sys.stderr,
    )
    raise SystemExit(2)

if len(raw_policies) < 1:
    print(
        f"Policy pool must define at least 1 arena model, but found {len(raw_policies)}: {pool_path}",
        file=sys.stderr,
    )
    raise SystemExit(2)

policies = []
for index, source in enumerate(raw_policies):
    if not isinstance(source, dict):
        print(f"Policy at index {index} must be an object.", file=sys.stderr)
        raise SystemExit(2)
    for required in ("id", "mode"):
        if not source.get(required):
            print(f"Policy at index {index} must define '{required}'.", file=sys.stderr)
            raise SystemExit(2)
    mode = str(source["mode"]).strip().lower()
    if mode not in {"heuristic", "neural"}:
        print(
            f"Policy '{source['id']}' has unsupported mode '{source['mode']}'. Expected heuristic or neural.",
            file=sys.stderr,
        )
        raise SystemExit(2)
    policy = {
        "id": str(source["id"]),
        "mode": mode,
        "model_path": source.get("model_path"),
    }
    if "sample_actions" in source:
        policy["sample_actions"] = bool(source["sample_actions"])
    if "temperature" in source:
        policy["temperature"] = float(source["temperature"])
    policies.append(policy)

policies_path.parent.mkdir(parents=True, exist_ok=True)
policies_path.write_text(
    json.dumps(
        policies,
        indent=2,
        ensure_ascii=False,
    )
    + "\n",
    encoding="utf-8",
)
for policy in policies:
    print(policy["id"])
PY
)"
mapfile -t SEAT_POLICY_IDS <<< "$SEAT_POLICY_IDS_TEXT"
POLICY_COUNT="${#SEAT_POLICY_IDS[@]}"

if (( PROGRESS_EVERY <= 0 )); then
    echo "PROGRESS_EVERY must be greater than 0." >&2
    exit 2
fi
if [[ "$RANDOM_SEED" != "0" && "$RANDOM_SEED" != "1" ]]; then
    echo "RANDOM_SEED must be 0 or 1." >&2
    exit 2
fi

random_arena_seed() {
    "$PYTHON_BIN" - <<'PY'
import secrets

print(secrets.randbelow(2_147_483_647) + 1)
PY
}

write_chunk_config() {
    local config_path="$1"
    local chunk_matches="$2"
    local chunk_seed="$3"
    local rotation_offset="$4"

    "$PYTHON_BIN" - "$POLICIES_PATH" "$config_path" "$chunk_matches" "$chunk_seed" "$MAX_ACTIONS_PER_MATCH" "$rotation_offset" <<'PY'
import json
import sys
from pathlib import Path

policies_path = Path(sys.argv[1])
config_path = Path(sys.argv[2])
matches = int(sys.argv[3])
seed = int(sys.argv[4])
max_actions = int(sys.argv[5])
rotation_offset = int(sys.argv[6])
policies = json.loads(policies_path.read_text(encoding="utf-8"))
config_path.write_text(
    json.dumps(
        {
            "matches": matches,
            "seed": seed,
            "max_actions_per_match": max_actions,
            "report_trajectories": False,
            "seat_rotation": "cyclic",
            "seat_rotation_offset": rotation_offset,
            "policies": policies,
        },
        indent=2,
        ensure_ascii=False,
    )
    + "\n",
    encoding="utf-8",
)
PY
}

seat_order_text() {
    local ids=()
    local index
    for index in "$@"; do
        ids+=("${SEAT_POLICY_IDS[$index]}")
    done
    local joined="${ids[*]}"
    printf '%s\n' "${joined// /, }"
}

cyclic_seat_order_text() {
    local offset="$1"
    local order=()
    local seat
    for seat in 0 1 2 3; do
        order+=($(( (seat + offset) % POLICY_COUNT )))
    done
    seat_order_text "${order[@]}"
}

print_summary() {
    local output_path="$1"

    "$PYTHON_BIN" - "$output_path" <<'PY'
import json
import sys
from collections import defaultdict
from pathlib import Path

output_path = Path(sys.argv[1])
if not output_path.exists():
    print(f"Arena output was not found: {output_path}", file=sys.stderr)
    raise SystemExit(0)

reports = [
    json.loads(line)
    for line in output_path.read_text(encoding="utf-8").splitlines()
    if line.strip()
]
if not reports:
    print(f"Arena output is empty: {output_path}", file=sys.stderr)
    raise SystemExit(0)

completed = sum(1 for report in reports if report.get("completed"))
total_actions = sum(report.get("action_count", 0) for report in reports)
avg_actions = total_actions / len(reports)

print()
print("Arena summary")
print(f"Output: {output_path}")
print(
    f"Matches: {len(reports)} completed={completed} "
    f"incomplete={len(reports) - completed} avg_actions={avg_actions:.1f}"
)
print()
print("Policy summary:")

groups = defaultdict(list)
for report in reports:
    for seat in report.get("seats", []):
        groups[seat.get("policy_id", "unknown")].append(seat)

for policy_id in sorted(groups):
    rows = groups[policy_id]
    score_sum = sum(row.get("score_delta", 0) for row in rows)
    wins = sum(row.get("wins", 0) for row in rows)
    dealt_in = sum(row.get("dealt_in", 0) for row in rows)
    decisions = sum(row.get("decision_count", 0) for row in rows)
    latency_sum = sum(row.get("decision_latency_ms_sum", 0) for row in rows)
    model_loaded = sum(1 for row in rows if row.get("model_loaded"))
    fallback_count = sum(row.get("fallback_count", 0) for row in rows)
    neural_actions = sum(row.get("neural_action_count", 0) for row in rows)
    same_as_heuristic = sum(row.get("same_as_heuristic_count", 0) for row in rows)
    heuristic_comparisons = sum(row.get("heuristic_comparison_count", 0) for row in rows)
    same_rate = (
        same_as_heuristic / heuristic_comparisons if heuristic_comparisons else 0.0
    )
    avg_score = score_sum / len(rows) if rows else 0.0
    avg_latency = latency_sum / decisions if decisions else 0.0
    tenpai = sum(1 for row in rows if row.get("final_tenpai"))
    print(
        f"  {policy_id:<10} seats={len(rows):4d} wins={wins:3d} "
        f"dealt_in={dealt_in:3d} score_sum={score_sum:7d} "
        f"avg_score={avg_score:7.1f} decisions={decisions:6d} "
        f"avg_latency_ms={avg_latency:6.1f} final_tenpai={tenpai:3d} "
        f"model_loaded={model_loaded:4d} fallback={fallback_count:5d} "
        f"neural_actions={neural_actions:5d} same_as_heuristic={same_rate:5.2f}"
    )
PY
}

echo "Initial seat order: $(cyclic_seat_order_text 0)"
echo "Policy pool: $POLICY_POOL"
echo "Output: $OUTPUT_DIR"
echo "Random seed: $RANDOM_SEED"

cd "$REPO_ROOT"
rm -f "$OUTPUT_PATH"

completed_matches=0
chunk_index=0

while (( completed_matches < MATCHES )); do
    chunk_matches=$(( MATCHES - completed_matches ))
    if (( chunk_matches > PROGRESS_EVERY )); then
        chunk_matches="$PROGRESS_EVERY"
    fi

    chunk_config_path="$(printf '%s/arena_config_%03d.json' "$OUTPUT_DIR" "$chunk_index")"
    chunk_output_path="$(printf '%s/arena_results_%03d.jsonl' "$OUTPUT_DIR" "$chunk_index")"
    rm -f "$chunk_output_path"
    if [[ "$RANDOM_SEED" == "1" ]]; then
        chunk_seed="$(random_arena_seed)"
    else
        chunk_seed="$(( SEED + completed_matches ))"
    fi
    rotation_offset=$(( completed_matches % POLICY_COUNT ))
    write_chunk_config \
        "$chunk_config_path" \
        "$chunk_matches" \
        "$chunk_seed" \
        "$rotation_offset"

    "$CARGO_EXE" run \
        --manifest-path "$BACKEND_MANIFEST" \
        --release \
        --bin bot_arena \
        -- \
        --config "$chunk_config_path" \
        --output "$chunk_output_path" \
        --jobs "$JOBS"

    cat "$chunk_output_path" >> "$OUTPUT_PATH"
    completed_matches=$(( completed_matches + chunk_matches ))
    echo "Arena progress: completed $completed_matches/$MATCHES chunk=$(( chunk_index + 1 )) seed=$chunk_seed rotation_offset=$rotation_offset first_match_seats=$(cyclic_seat_order_text "$rotation_offset")"

    chunk_index=$(( chunk_index + 1 ))
done

print_summary "$OUTPUT_PATH"
