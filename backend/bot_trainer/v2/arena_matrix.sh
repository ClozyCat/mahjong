#!/usr/bin/env bash
set -euo pipefail

MATCHES="${MATCHES:-200}"
SEED="${SEED:-20260429}"
OUTPUT_DIR="${OUTPUT_DIR:-backend/bot_trainer/v2/arena_runs}"
PROGRESS_EVERY="${PROGRESS_EVERY:-10}"
JOBS="${JOBS:-0}"
SEAT_ORDER="${SEAT_ORDER:-default}"

mkdir -p "$OUTPUT_DIR"
CONFIG_PATH="$OUTPUT_DIR/arena_config.json"
OUTPUT_PATH="$OUTPUT_DIR/arena_results.jsonl"

policy_json() {
    case "$1" in
        heuristic)
            printf '{"id":"heuristic","mode":"heuristic","neural_weight":0,"model_path":null}'
            ;;
        neural)
            printf '{"id":"neural","mode":"neural","neural_weight":0,"model_path":"backend/assets/models/mahjong_policy_net.onnx"}'
            ;;
        *)
            echo "Unknown policy id: $1" >&2
            exit 2
            ;;
    esac
}

resolve_seat_order() {
    local seat_order="$1"
    case "$seat_order" in
        default|current)
            printf '%s\n' heuristic neural heuristic neural
            ;;
        rotate1)
            printf '%s\n' neural heuristic neural heuristic
            ;;
        *,*)
            printf '%s\n' "$seat_order" | tr ',' '\n' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//' | sed '/^$/d'
            ;;
        *)
            echo "Unknown seat order preset: $seat_order" >&2
            echo "Presets: default, current, rotate1" >&2
            echo "Custom form: heuristic,neural,heuristic,neural" >&2
            exit 2
            ;;
    esac
}

mapfile -t SEAT_POLICY_IDS < <(resolve_seat_order "$SEAT_ORDER")
if [[ "${#SEAT_POLICY_IDS[@]}" -ne 4 ]]; then
    echo "SEAT_ORDER must resolve to exactly 4 policy ids." >&2
    echo "Presets: default, current, rotate1" >&2
    echo "Custom form: heuristic,neural,heuristic,neural" >&2
    exit 2
fi

POLICY_JSON_LINES=()
for policy_id in "${SEAT_POLICY_IDS[@]}"; do
    POLICY_JSON_LINES+=("$(policy_json "$policy_id")")
done

{
cat <<JSON
{
  "matches": $MATCHES,
  "seed": $SEED,
  "max_actions_per_match": 2400,
  "report_trajectories": false,
  "policies": [
JSON
for index in "${!POLICY_JSON_LINES[@]}"; do
    suffix=","
    if [[ "$index" -eq 3 ]]; then
        suffix=""
    fi
    printf '    %s%s\n' "${POLICY_JSON_LINES[$index]}" "$suffix"
done
cat <<JSON
  ]
}
JSON
} > "$CONFIG_PATH"

print_summary() {
    local output_path="$1"
    local python_bin=""
    if command -v python >/dev/null 2>&1; then
        python_bin="python"
    elif command -v python3 >/dev/null 2>&1; then
        python_bin="python3"
    else
        echo "Python was not found; skipping arena summary." >&2
        return
    fi

    "$python_bin" - "$output_path" <<'PY'
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
    avg_score = score_sum / len(rows) if rows else 0.0
    avg_latency = latency_sum / decisions if decisions else 0.0
    tenpai = sum(1 for row in rows if row.get("final_tenpai"))
    print(
        f"  {policy_id:<10} seats={len(rows):4d} wins={wins:3d} "
        f"dealt_in={dealt_in:3d} score_sum={score_sum:7d} "
        f"avg_score={avg_score:7.1f} decisions={decisions:6d} "
        f"avg_latency_ms={avg_latency:6.1f} final_tenpai={tenpai:3d}"
    )
PY
}

echo "Seat order: ${SEAT_POLICY_IDS[*]}"

cargo run \
    --manifest-path backend/Cargo.toml \
    --release \
    --bin bot_arena \
    -- \
    --config "$CONFIG_PATH" \
    --output "$OUTPUT_PATH" \
    --progress-every "$PROGRESS_EVERY" \
    --jobs "$JOBS"

print_summary "$OUTPUT_PATH"
