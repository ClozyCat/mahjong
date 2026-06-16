#!/usr/bin/env pwsh
<#
.SYNOPSIS
  AWR/AWAC offline policy improvement pipeline.
  Replaces the old PPO self-play pipeline (train_rl_model.ps1).

.DESCRIPTION
  1. Generate league trajectories via arena (stochastic sampling, all 4 seats)
  2. Value head pretraining (train_value.py)
  3. AWR training (train_awr.py)
  4. Export AWR checkpoint to ONNX
  5. Evaluate candidate vs baseline
  6. Promote if candidate passes gate
#>

param(
    [string]$Iterations = "3",
    [string]$TrajectoryMatches = "100",
    [string]$Seed = (Get-Date -Format "yyyyMMdd"),
    [string]$SftOnnx = "backend/assets/sft/sft.onnx",
    [string]$SftCheckpoint = "backend/bot_trainer/v2/checkpoints/best.pt",
    [string]$OutputDir = "output/awr_training",
    [string]$Pool = "backend/bot_trainer/v2/opponent_pool.json"
)

$ErrorActionPreference = "Stop"
Push-Location $PSScriptRoot/../..

Write-Host "=== AWR Training Pipeline ===" -ForegroundColor Cyan
Write-Host "Iterations: $Iterations, TrajectoryMatches: $TrajectoryMatches, Seed: $Seed"

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

for ($iter = 0; $iter -lt [int]$Iterations; $iter++) {
    Write-Host "`n--- Iteration $($iter+1)/$Iterations ---" -ForegroundColor Yellow

    $iterSeed = [int]$Seed + $iter * 1000

    # 1. Generate trajectories
    Write-Host "[1/4] Generating trajectories..."
    $trajConfigDir = "$OutputDir/iter_$iter/configs"
    New-Item -ItemType Directory -Force -Path $trajConfigDir | Out-Null

    python backend/bot_trainer/v2/league_config.py `
        --pool $Pool `
        --output-dir $trajConfigDir `
        --matches $TrajectoryMatches `
        --seed $iterSeed `
        --mode trajectory `
        --rollout-onnx $SftOnnx

    $trajOut = "$OutputDir/iter_$iter/trajectories.jsonl"
    $trajSummary = "$OutputDir/iter_$iter/trajectory_summary.json"
    cargo run --release --manifest-path backend/Cargo.toml --bin bot_arena -- `
        --config "$trajConfigDir/trajectory_config_0.json" `
        --output "$trajSummary" `
        --trajectories "$trajOut" `
        --jobs 4

    if (-not (Test-Path $trajOut)) {
        Write-Error "Trajectory generation failed for iteration $iter"
        exit 1
    }

    # 2. Value pretraining
    Write-Host "[2/4] Pretraining value head..."
    $valueCkpt = "$OutputDir/iter_$iter/value_pretrained.pt"
    if (-not (Test-Path $SftCheckpoint)) {
        $altCheckpoint = "output/sft/best.pt"
        if (Test-Path $altCheckpoint) {
            $SftCheckpoint = $altCheckpoint
        } else {
            $legacyCheckpoint = "output/sft_checkpoints/best.pt"
            if (Test-Path $legacyCheckpoint) {
                $SftCheckpoint = $legacyCheckpoint
            } else {
                Write-Error "SFT checkpoint not found at $SftCheckpoint, $altCheckpoint, or $legacyCheckpoint"
                exit 1
            }
        }
    }
    python backend/bot_trainer/v2/train_value.py `
        --trajectories $trajOut `
        --checkpoint $SftCheckpoint `
        --output $valueCkpt `
        --epochs 10 `
        --batch-size 256 `
        --lr 1e-3 `
        --policy-id learner

    # 3. AWR training
    Write-Host "[3/4] Running AWR training..."
    $awrDir = "$OutputDir/iter_$iter/awr_checkpoints"
    python backend/bot_trainer/v2/train_awr.py `
        --trajectories $trajOut `
        --checkpoint $valueCkpt `
        --output-dir $awrDir `
        --epochs 5 `
        --batch-size 256 `
        --lr 3e-5 `
        --temperature 1.0 `
        --policy-id learner

    # 4. Export best AWR checkpoint to ONNX
    Write-Host "[4/4] Exporting AWR ONNX..."
    $awrOnnx = "$OutputDir/iter_$iter/awr.onnx"
    python backend/bot_trainer/v2/export_onnx.py `
        --checkpoint "$awrDir/awr_best.pt" `
        --output $awrOnnx

    # 5. Evaluate vs baseline
    Write-Host "Evaluating candidate vs baseline..."
    $evalConfigDir = "$OutputDir/iter_$iter/eval_config"
    New-Item -ItemType Directory -Force -Path $evalConfigDir | Out-Null

    python backend/bot_trainer/v2/league_config.py `
        --pool $Pool `
        --output-dir $evalConfigDir `
        --matches 50 `
        --seed $($iterSeed + 1) `
        --mode eval `
        --candidate-onnx $awrOnnx `
        --baseline-onnx $SftOnnx

    $evalOut = "$OutputDir/iter_$iter/eval_results.json"
    cargo run --release --manifest-path backend/Cargo.toml --bin bot_arena -- `
        --config "$evalConfigDir/candidate_eval_config.json" `
        --output "$evalOut"

    # 6. Candidate gate
    $gateResult = "$OutputDir/iter_$iter/gate_result.json"
    $exitCode = 0
    try {
        python backend/bot_trainer/v2/candidate_gate.py `
            --summary $evalOut `
            --output $gateResult
    } catch {
        $exitCode = $LASTEXITCODE
    }

    if ($exitCode -eq 0) {
        Write-Host ">>> ITERATION $iter : CANDIDATE ACCEPTED <<<" -ForegroundColor Green
    } else {
        Write-Host ">>> ITERATION $iter : CANDIDATE REJECTED <<<" -ForegroundColor Yellow
    }
}

Write-Host "`nAWR training pipeline complete. Output: $OutputDir" -ForegroundColor Cyan
Pop-Location
