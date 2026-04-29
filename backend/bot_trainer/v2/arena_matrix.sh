#!/usr/bin/env bash
set -euo pipefail

MATCHES="${MATCHES:-200}"
SEED="${SEED:-20260429}"
OUTPUT_DIR="${OUTPUT_DIR:-backend/bot_trainer/v2/arena_runs}"

mkdir -p "$OUTPUT_DIR"
CONFIG_PATH="$OUTPUT_DIR/arena_config.json"
OUTPUT_PATH="$OUTPUT_DIR/arena_results.jsonl"

cat > "$CONFIG_PATH" <<JSON
{
  "matches": $MATCHES,
  "seed": $SEED,
  "max_actions_per_match": 2400,
  "report_trajectories": false,
  "policies": [
    {"id":"heuristic","mode":"heuristic","neural_weight":0,"model_path":null},
    {"id":"neural","mode":"neural","neural_weight":0,"model_path":"backend/assets/models/mahjong_policy_net.onnx"},
    {"id":"hybrid05","mode":"hybrid","neural_weight":5,"model_path":"backend/assets/models/mahjong_policy_net.onnx"},
    {"id":"hybrid15","mode":"hybrid","neural_weight":15,"model_path":"backend/assets/models/mahjong_policy_net.onnx"},
    {"id":"hybrid30","mode":"hybrid","neural_weight":30,"model_path":"backend/assets/models/mahjong_policy_net.onnx"},
    {"id":"hybrid60","mode":"hybrid","neural_weight":60,"model_path":"backend/assets/models/mahjong_policy_net.onnx"}
  ]
}
JSON

cargo run --manifest-path backend/Cargo.toml --release --bin bot_arena -- --config "$CONFIG_PATH" --output "$OUTPUT_PATH"
