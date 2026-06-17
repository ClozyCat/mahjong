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
    [string]$MatrixMatches = "20",
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
    if (Test-Path $trajOut) {
        Remove-Item -LiteralPath $trajOut -Force
    }
    $trajectoryConfigFiles = Get-ChildItem -LiteralPath $trajConfigDir -Filter "trajectory_config_*.json" | Sort-Object Name
    foreach ($configFile in $trajectoryConfigFiles) {
        $chunkSummary = "$OutputDir/iter_$iter/trajectory_summary_$($configFile.BaseName).json"
        $chunkTrajectories = "$OutputDir/iter_$iter/trajectories_$($configFile.BaseName).jsonl"
        cargo run --release --manifest-path backend/Cargo.toml --bin bot_arena -- `
            --config $configFile.FullName `
            --output "$chunkSummary" `
            --trajectories "$chunkTrajectories" `
            --jobs 4
        if (Test-Path $chunkTrajectories) {
            Get-Content -LiteralPath $chunkTrajectories | Add-Content -LiteralPath $trajOut
        }
    }

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
        --batch-size 512 `
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
        --value-finetune-epochs 3 `
        --batch-size 512 `
        --lr 3e-5 `
        --temperature 0.5 `
        --sft-checkpoint $SftCheckpoint `
        --policy-id learner

    # 4. Export best AWR checkpoint to ONNX
    Write-Host "[4/4] Exporting AWR ONNX..."
    $awrOnnx = "$OutputDir/iter_$iter/awr.onnx"
    python backend/bot_trainer/v2/export_onnx.py `
        --checkpoint "$awrDir/awr_best.pt" `
        --output $awrOnnx

    # 5. Matrix evaluation
    Write-Host "Evaluating candidate vs multiple opponents..."
    $matrixConfigDir = "$OutputDir/iter_$iter/matrix_config"
    New-Item -ItemType Directory -Force -Path $matrixConfigDir | Out-Null

    python backend/bot_trainer/v2/league_config.py `
        --pool $Pool `
        --output-dir $matrixConfigDir `
        --matches $MatrixMatches `
        --seed $($iterSeed + 2) `
        --mode matrix `
        --candidate-onnx $awrOnnx `
        --baseline-onnx $SftOnnx

    $matrixSummaries = @()
    $configFiles = Get-ChildItem -LiteralPath $matrixConfigDir -Filter "matrix_config_*.json" | Sort-Object Name
    foreach ($configFile in $configFiles) {
        $resultFile = "$OutputDir/iter_$iter/matrix_result_$($configFile.BaseName).jsonl"
        $summaryFile = "$OutputDir/iter_$iter/matrix_summary_$($configFile.BaseName).json"
        cargo run --release --manifest-path backend/Cargo.toml --bin bot_arena -- `
            --config $configFile.FullName `
            --output $resultFile
        if (Test-Path $resultFile) {
            python backend/bot_trainer/v2/arena_summary.py `
                --input $resultFile `
                --output $summaryFile
            $matrixSummaries += $summaryFile
        }
    }

    # 6. Candidate gate (matrix mode)
    $gateResult = "$OutputDir/iter_$iter/gate_result.json"
    $exitCode = 0
    try {
        python backend/bot_trainer/v2/candidate_gate.py `
            --summary $matrixSummaries `
            --pool $Pool `
            --output $gateResult
    } catch {
        $exitCode = $LASTEXITCODE
    }

    if ($exitCode -eq 0) {
        Write-Host ">>> ITERATION $iter : CANDIDATE ACCEPTED <<<" -ForegroundColor Green
    } else {
        Write-Host ">>> ITERATION $iter : CANDIDATE REJECTED <<<" -ForegroundColor Yellow
    }

    # 7. League update: if accepted, add this model to opponent pool
    $leagueHistory = "$OutputDir/league_history.json"
    $history = @()
    if (Test-Path $leagueHistory) {
        $history = @(Get-Content -Raw -LiteralPath $leagueHistory | ConvertFrom-Json)
    }
    if ($exitCode -eq 0) {
        $stableDir = "backend/assets/league"
        New-Item -ItemType Directory -Force -Path $stableDir | Out-Null
        $stableOnnx = "$stableDir/awr_iter_$iter.onnx"
        Copy-Item -LiteralPath $awrOnnx -Destination $stableOnnx -Force
        $dataFile = "$awrOnnx.data"
        if (Test-Path $dataFile) {
            Copy-Item -LiteralPath $dataFile -Destination "$stableOnnx.data" -Force
        }
        $manifestFile = "$awrOnnx.manifest.json"
        if (Test-Path $manifestFile) {
            Copy-Item -LiteralPath $manifestFile -Destination "$stableOnnx.manifest.json" -Force
        }
        $history += @{
            iter = $iter
            model_path = $stableOnnx
            temperature = 1.0
        }
        $absLeagueHistory = Join-Path (Get-Location).Path $leagueHistory
        $historyJson = ConvertTo-Json -InputObject $history -Depth 3 -Compress
        New-Item -ItemType Directory -Force -Path (Split-Path $absLeagueHistory -Parent) | Out-Null
        $historyJson | Set-Content -LiteralPath $absLeagueHistory -Encoding UTF8
        Write-Host "  Added to league pool ($stableOnnx)"
    }

    # Rebuild opponent_pool.json with league history — rotate, don't just append
    if ($history.Count -gt 0) {
        $origPool = Get-Content -LiteralPath $Pool -Raw | ConvertFrom-Json

        # SFT base variants: always keep as candidates for rotation
        $sftBase = @($origPool.rollout_opponents | Where-Object { $_.id -like "sft_*" })
        if ($sftBase.Count -eq 0) {
            # Fallback: read from a pristine pool copy or hardcode
            $sftBase = @(
                @{id="sft_cold"; model_path="backend/assets/sft/sft.onnx"; sample_actions=$true; temperature=0.5; weight=1},
                @{id="sft_warm"; model_path="backend/assets/sft/sft.onnx"; sample_actions=$true; temperature=1.0; weight=2},
                @{id="sft_hot";  model_path="backend/assets/sft/sft.onnx"; sample_actions=$true; temperature=2.0; weight=1}
            )
        }

        # AWR league models with recency weight (more recent = higher weight)
        $awrPool = @($history | ForEach-Object {
            @{
                id = "awr_iter_$($_.iter)"
                model_path = $_.model_path
                sample_actions = $true
                temperature = if ($_.iter % 3 -eq 0) { 0.5 } elseif ($_.iter % 3 -eq 1) { 1.0 } else { 1.5 }
                weight = [Math]::Max(1, 5 - ($history.Count - 1 - ([array]::IndexOf($history, $_))))
            }
        })

        # Build weighted candidate pool: SFT base (each weight 1) + AWR (recency-weighted)
        $candidatePool = @()
        foreach ($s in $sftBase) { $candidatePool += $s }
        foreach ($a in $awrPool)  { $candidatePool += $a }

        $rng = [Random]::new($iterSeed + 3)
        $maxSlots = 6

        # Always reserve 1 slot for a random SFT baseline
        $sftSlot = $sftBase[$rng.Next($sftBase.Count)]

        # Always include the most recent accepted AWR if it passed this round
        $mustInclude = @()
        if ($exitCode -eq 0) {
            $mustInclude = @($awrPool | Where-Object { $_.id -eq "awr_iter_$iter" })
        }

        # Remaining slots: weighted random sample from candidate pool (excluding already selected)
        $remainingSlots = $maxSlots - 1 - $mustInclude.Count
        if ($remainingSlots -lt 0) { $remainingSlots = 0 }

        $others = @($candidatePool | Where-Object {
            $_.id -ne $sftSlot.id -and ($mustInclude.Count -eq 0 -or $_.id -ne $mustInclude[0].id)
        })

        # Weighted shuffle: repeat each candidate by its weight, then shuffle
        $weightedOthers = @()
        foreach ($o in $others) {
            1..([int]$o.weight) | ForEach-Object { $weightedOthers += $o }
        }
        $shuffled = @($weightedOthers | Sort-Object { $rng.Next() })
        $selectedOthers = @{}
        foreach ($s in $shuffled) {
            if ($selectedOthers.Count -ge $remainingSlots) { break }
            $selectedOthers[$s.id] = $s
        }

        $updatedPool = @{
            schema_version = 3
            learner = $origPool.learner
            rollout_opponents = @(@($sftSlot) + $mustInclude + @($selectedOthers.Values))
        }
        $absPool = Join-Path (Get-Location).Path $Pool
        (ConvertTo-Json -InputObject $updatedPool -Depth 4) | Set-Content -LiteralPath $absPool -Encoding UTF8
        Write-Host "  League pool updated: 1 SFT baseline + $($mustInclude.Count) required AWR + $(@($selectedOthers.Values).Count) random = $(@($updatedPool.rollout_opponents).Count) total"
    }
}

Write-Host "`nAWR training pipeline complete. Output: $OutputDir" -ForegroundColor Cyan
Pop-Location
