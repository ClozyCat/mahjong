param(
    [int]$Matches = 200,
    [int]$Seed = 20260429,
    [string]$OutputDir = "backend/bot_trainer/v2/arena_runs"
)

$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$config = @{
    matches = $Matches
    seed = $Seed
    max_actions_per_match = 2400
    report_trajectories = $false
    policies = @(
        @{ id = "heuristic"; mode = "heuristic"; neural_weight = 0; model_path = $null },
        @{ id = "neural"; mode = "neural"; neural_weight = 0; model_path = "backend/assets/models/mahjong_policy_net.onnx" },
        @{ id = "hybrid05"; mode = "hybrid"; neural_weight = 5; model_path = "backend/assets/models/mahjong_policy_net.onnx" },
        @{ id = "hybrid15"; mode = "hybrid"; neural_weight = 15; model_path = "backend/assets/models/mahjong_policy_net.onnx" },
        @{ id = "hybrid30"; mode = "hybrid"; neural_weight = 30; model_path = "backend/assets/models/mahjong_policy_net.onnx" },
        @{ id = "hybrid60"; mode = "hybrid"; neural_weight = 60; model_path = "backend/assets/models/mahjong_policy_net.onnx" }
    )
}

$configPath = Join-Path $OutputDir "arena_config.json"
$outputPath = Join-Path $OutputDir "arena_results.jsonl"
$config | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 $configPath

cargo run --manifest-path backend/Cargo.toml --release --bin bot_arena -- --config $configPath --output $outputPath
