#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../../.." && pwd)"
BACKEND_MANIFEST="$REPO_ROOT/backend/Cargo.toml"
ORIGINAL_PWD="$(pwd)"

MATCH_COUNT="${MATCH_COUNT:-200}"
SEED="${SEED:-20260429}"
RANDOM_SEED="${RANDOM_SEED:-0}"
ARENA_CONFIG="${ARENA_CONFIG:-$SCRIPT_DIR/arena_policy_pool.json}"
OUTPUT_DIR="${OUTPUT_DIR:-$SCRIPT_DIR/arena_runs}"
PROGRESS_EVERY="${PROGRESS_EVERY:-10}"
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

ARENA_CONFIG="$(resolve_user_path "$ARENA_CONFIG")"
OUTPUT_DIR="$(resolve_user_path "$OUTPUT_DIR")"

mkdir -p "$OUTPUT_DIR"
OUTPUT_PATH="$OUTPUT_DIR/arena_results.jsonl"
CONFIG_TEMPLATE_PATH="$OUTPUT_DIR/arena_template.json"

find_python() {
    if command -v python >/dev/null 2>&1; then
        printf '%s\n' python
    elif command -v python3 >/dev/null 2>&1; then
        printf '%s\n' python3
    else
        echo "Python was not found; cannot read arena evaluation config JSON." >&2
        exit 2
    fi
}

PYTHON_BIN="$(find_python)"

SUBJECT_POLICY_IDS_TEXT="$("$PYTHON_BIN" - "$ARENA_CONFIG" "$CONFIG_TEMPLATE_PATH" <<'PY'
import json
import sys
from pathlib import Path

config_path = Path(sys.argv[1])
template_path = Path(sys.argv[2])

if not config_path.is_file():
    print(f"Arena evaluation config was not found: {config_path}", file=sys.stderr)
    raise SystemExit(2)

template = json.loads(config_path.read_text(encoding="utf-8"))
if "policies" in template or "learner" in template:
    print(
        f"Arena matrix now accepts only evaluation configs with 'subjects' and exactly three 'opponents': {config_path}",
        file=sys.stderr,
    )
    raise SystemExit(2)

subjects = template.get("subjects")
opponents = template.get("opponents")
if not isinstance(subjects, list) or not subjects:
    print(f"Arena evaluation config must define at least one subject: {config_path}", file=sys.stderr)
    raise SystemExit(2)
if not isinstance(opponents, list) or len(opponents) != 3:
    count = len(opponents) if isinstance(opponents, list) else 0
    print(f"Arena evaluation config must define exactly three opponents, found {count}: {config_path}", file=sys.stderr)
    raise SystemExit(2)

for section_name, section in (("subjects", subjects), ("opponents", opponents)):
    for index, source in enumerate(section):
        if not isinstance(source, dict):
            print(f"{section_name}[{index}] must be an object.", file=sys.stderr)
            raise SystemExit(2)
        for required in ("id", "model_path"):
            if not source.get(required):
                print(f"{section_name}[{index}] must define '{required}'.", file=sys.stderr)
                raise SystemExit(2)
for index, source in enumerate(subjects):
    if not source.get("display_name"):
        print(f"subjects[{index}] must define 'display_name'.", file=sys.stderr)
        raise SystemExit(2)

template_path.parent.mkdir(parents=True, exist_ok=True)
template_path.write_text(json.dumps(template, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
for subject in subjects:
    print(subject["id"])
PY
)"
mapfile -t SUBJECT_POLICY_IDS <<< "$SUBJECT_POLICY_IDS_TEXT"

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

    "$PYTHON_BIN" - "$CONFIG_TEMPLATE_PATH" "$config_path" "$chunk_matches" "$chunk_seed" <<'PY'
import json
import sys
from pathlib import Path

template_path = Path(sys.argv[1])
config_path = Path(sys.argv[2])
matches = int(sys.argv[3])
seed = int(sys.argv[4])
template = json.loads(template_path.read_text(encoding="utf-8"))
template["matches"] = matches
template["seed"] = seed

config_path.write_text(
    json.dumps(
        template,
        indent=2,
        ensure_ascii=False,
    )
    + "\n",
    encoding="utf-8",
)
PY
}

subject_order_text() {
    local joined="${SUBJECT_POLICY_IDS[*]}"
    printf '%s\n' "${joined// /, }"
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
    neural_actions = sum(row.get("neural_action_count", 0) for row in rows)
    avg_score = score_sum / len(rows) if rows else 0.0
    avg_latency = latency_sum / decisions if decisions else 0.0
    tenpai = sum(1 for row in rows if row.get("final_tenpai"))
    print(
        f"  {policy_id:<10} seats={len(rows):4d} wins={wins:3d} "
        f"dealt_in={dealt_in:3d} score_sum={score_sum:7d} "
        f"avg_score={avg_score:7.1f} decisions={decisions:6d} "
        f"avg_latency_ms={avg_latency:6.1f} final_tenpai={tenpai:3d} "
        f"model_loaded={model_loaded:4d} neural_actions={neural_actions:5d}"
    )
PY
}

echo "Subjects: $(subject_order_text)"
echo "Arena config: $ARENA_CONFIG"
echo "Output: $OUTPUT_DIR"
echo "Random seed: $RANDOM_SEED"

cd "$REPO_ROOT"
rm -f "$OUTPUT_PATH"

completed_matches=0
chunk_index=0

while (( completed_matches < MATCH_COUNT )); do
    chunk_matches=$(( MATCH_COUNT - completed_matches ))
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
    write_chunk_config \
        "$chunk_config_path" \
        "$chunk_matches" \
        "$chunk_seed"

    "$CARGO_EXE" run \
        --manifest-path "$BACKEND_MANIFEST" \
        --release \
        --bin bot_arena \
        -- \
        --config "$chunk_config_path" \
        --output "$chunk_output_path"

    cat "$chunk_output_path" >> "$OUTPUT_PATH"
    completed_matches=$(( completed_matches + chunk_matches ))
    echo "Arena progress: completed $completed_matches/$MATCH_COUNT chunk=$(( chunk_index + 1 )) seed=$chunk_seed subjects=$(subject_order_text)"

    chunk_index=$(( chunk_index + 1 ))
done

print_summary "$OUTPUT_PATH"
