param(
    [string]$OutputDir = "backend/bot_trainer/v2/rl_runs/$(Get-Date -Format 'yyyyMMddHHmm')",
    [string]$BaselineCheckpoint = "backend/bot_trainer/v2/checkpoints/best.pt",
    [string]$BaselineOnnx = "backend/assets/models/mahjong_policy_net.onnx",
    [string]$PythonExe = "python",
    [string]$PythonVersion = "",
    [string]$CargoExe = "cargo",
    [int]$ArenaJobs = 0,
    [int]$Iterations = 5,
    [int]$IterationMatches = 500,
    [int]$TrajectoryProgressEvery = 20,
    [int]$EvalMatches = 1000,
    [int]$Seed = 20260429,
    [int]$MaxActionsPerMatch = 2400,
    [int]$Epochs = 1,
    [int]$BatchSize = 4096,
    [double]$LearningRate = 0.000003,
    [double]$Gamma = 0.995,
    [double]$GaeLambda = 0.95,
    [double]$ClipEpsilon = 0.2,
    [double]$ValueClipEpsilon = 0.2,
    [double]$EntropyCoef = 0.02,
    [double]$EntropyEndCoef = 0.005,
    [int]$EntropyDecaySteps = 0,
    [double]$KlCoef = 0.01,
    [double]$KlEndCoef = 0.0,
    [double]$TargetKl = 0.03,
    [string]$Device = "auto",
    [string]$OpponentPool = "backend/bot_trainer/v2/opponent_pool.json",
    [string]$LearnerPolicyId = "learner",
    [string]$SelfPlayPolicyId = "selfplay_neural",
    [ValidateSet("heuristic", "neural")]
    [string]$SelfPlayPolicyMode = "neural",
    [switch]$SkipTests,
    [switch]$SkipOnnxExport,
    [switch]$SkipEval,
    [switch]$EnforceCandidateGate,
    [switch]$AllowRlBaselineCheckpoint,
    [switch]$RecomputeOldPolicyStats,
    [ValidateSet("epoch", "final")]
    [string]$CandidateSelectionMode = "epoch"
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Resolve-Path (Join-Path $ScriptDir "..\..\..") 
$env:PYTHONUTF8 = "1"
$env:PYTHONIOENCODING = "utf-8"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

function Invoke-TrainingPython {
    param([string[]]$Arguments)
    if ($PythonExe -eq "py" -and $PythonVersion.Length -gt 0) {
        & $PythonExe "-$PythonVersion" @Arguments
    }
    else {
        & $PythonExe @Arguments
    }
}

function Assert-PythonModule {
    param([string]$ModuleName)
    Invoke-TrainingPython @("-c", "import $ModuleName")
    if ($LASTEXITCODE -ne 0) {
        throw "Python module '$ModuleName' is required for RL training."
    }
}

function Assert-FileExists {
    param(
        [string]$Path,
        [string]$Purpose,
        [string]$Advice
    )
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "$Purpose not found: $Path`n$Advice"
    }
}

function Write-Utf8NoBom {
    param(
        [string]$Path,
        [string]$Content
    )
    $encoding = New-Object System.Text.UTF8Encoding $false
    [System.IO.File]::WriteAllText((Resolve-Path -LiteralPath (Split-Path -Parent $Path)).Path + [System.IO.Path]::DirectorySeparatorChar + (Split-Path -Leaf $Path), $Content, $encoding)
}

function Copy-RequiredFile {
    param(
        [string]$SourcePath,
        [string]$TargetPath
    )
    if (-not (Test-Path -LiteralPath $SourcePath -PathType Leaf)) {
        throw "Required file was not found: $SourcePath"
    }
    Copy-Item -LiteralPath $SourcePath -Destination $TargetPath -Force
}

function Invoke-CandidateEvaluation {
    param(
        [string]$CandidateModel,
        [string]$EvalDir,
        [string]$EvalBaselineOnnx
    )

    New-Item -ItemType Directory -Force -Path $EvalDir | Out-Null
    Invoke-TrainingPython @(
        "backend/bot_trainer/v2/league_config.py",
        "--pool", $OpponentPool,
        "--output-dir", $EvalDir,
        "--matches", "$EvalMatches",
        "--seed", "$Seed",
        "--max-actions", "$MaxActionsPerMatch",
        "--mode", "eval",
        "--candidate-onnx", $CandidateModel,
        "--baseline-onnx", $EvalBaselineOnnx
    )
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    $localConfig = Join-Path $EvalDir "candidate_eval_config.json"
    $localJsonl = Join-Path $EvalDir "candidate_eval.jsonl"
    $localSummary = Join-Path $EvalDir "candidate_eval_summary.json"
    $localGate = Join-Path $EvalDir "candidate_gate.json"

    & $CargoExe run --manifest-path backend/Cargo.toml --release --bin bot_arena -- --config $localConfig --output $localJsonl --jobs $ArenaJobs
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Invoke-TrainingPython @(
        "backend/bot_trainer/v2/arena_summary.py",
        "--input", $localJsonl,
        "--output", $localSummary
    )
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Invoke-TrainingPython @(
        "backend/bot_trainer/v2/candidate_gate.py",
        "--summary", $localSummary,
        "--baseline-policy", "baseline_neural",
        "--candidate-policy", "rl_candidate_neural",
        "--output", $localGate
    )
    $localGateExit = $LASTEXITCODE

    return [ordered]@{
        config = $localConfig
        jsonl = $localJsonl
        summary = $localSummary
        gate = $localGate
        gate_exit = $localGateExit
    }
}

function New-PytestWindowsSiteCustomize {
    param([string]$TempDir)
    $siteDir = Join-Path $TempDir "pytest_site"
    New-Item -ItemType Directory -Force -Path $siteDir | Out-Null
    $siteCustomize = Join-Path $siteDir "sitecustomize.py"
    Write-Utf8NoBom $siteCustomize @'
import os
import pathlib

if os.name == "nt":
    _original_mkdir = pathlib.Path.mkdir

    def _mkdir_with_accessible_mode(self, mode=0o777, parents=False, exist_ok=False):
        if mode == 0o700:
            mode = 0o777
        return _original_mkdir(self, mode=mode, parents=parents, exist_ok=exist_ok)

    pathlib.Path.mkdir = _mkdir_with_accessible_mode
'@
    return (Resolve-Path $siteDir).Path
}

function Resolve-UsableTempPath {
    param([string]$Candidate)

    if (-not [string]::IsNullOrWhiteSpace($Candidate) -and (Test-Path -LiteralPath $Candidate -PathType Container)) {
        return (Resolve-Path -LiteralPath $Candidate).Path
    }

    if (-not [string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
        $localTemp = Join-Path $env:LOCALAPPDATA "Temp"
        New-Item -ItemType Directory -Force -Path $localTemp | Out-Null
        return (Resolve-Path -LiteralPath $localTemp).Path
    }

    $fallback = Join-Path $RepoRoot ".tmp\windows-temp"
    New-Item -ItemType Directory -Force -Path $fallback | Out-Null
    return (Resolve-Path -LiteralPath $fallback).Path
}

$PreviousTemp = $env:TEMP
$PreviousTmp = $env:TMP
$PreviousPytestTempRoot = $env:PYTEST_DEBUG_TEMPROOT

Push-Location $RepoRoot
try {
    Assert-PythonModule "torch"
    Assert-PythonModule "onnxruntime"

    New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
    $TempDir = Join-Path $OutputDir "tmp"
    New-Item -ItemType Directory -Force -Path $TempDir | Out-Null
    $env:TEMP = (Resolve-Path $TempDir).Path
    $env:TMP = $env:TEMP
    $env:PYTEST_DEBUG_TEMPROOT = $env:TEMP

    Write-Host "Mahjong RL training (iterative self-play)"
    Write-Host "Output:              $OutputDir"
    Write-Host "Baseline checkpoint: $BaselineCheckpoint"
    Write-Host "Baseline ONNX:       $BaselineOnnx"
    Write-Host "Iterations:          $Iterations"
    Write-Host "Matches/iteration:   $IterationMatches"
    Write-Host "PPO epochs/iter:     $Epochs"
    Write-Host "Gamma:               $Gamma"
    Write-Host "KL coef:             $KlCoef"
    Write-Host "Opponent pool:       $OpponentPool"
    Write-Host "Learner policy id:   $LearnerPolicyId"
    Write-Host "Eval matches:        $EvalMatches"
    Write-Host "Device:              $Device"
    Write-Host "Python:              $PythonExe $PythonVersion"
    Write-Host "Cargo:               $CargoExe"
    $arenaJobsLabel = if ($ArenaJobs -eq 0) { "auto" } else { $ArenaJobs }
    Write-Host ("Arena jobs:          {0}" -f $arenaJobsLabel)

    Assert-FileExists `
        $BaselineCheckpoint `
        "Baseline checkpoint" `
        "Run supervised training first with backend/bot_trainer/v2/train_and_export_model.ps1, or pass -BaselineCheckpoint <existing .pt file>."
    Assert-FileExists `
        $BaselineOnnx `
        "Baseline ONNX model" `
        "Export the supervised model first, or pass -BaselineOnnx <existing .onnx file>."
    $baselineGuardArgs = @(
        "backend/bot_trainer/v2/baseline_guard.py",
        "--checkpoint", $BaselineCheckpoint,
        "--onnx", $BaselineOnnx
    )
    if ($AllowRlBaselineCheckpoint) {
        $baselineGuardArgs += @("--allow-rl-checkpoint")
    }
    Invoke-TrainingPython $baselineGuardArgs
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    if (-not $SkipTests) {
        $pytestSiteDir = New-PytestWindowsSiteCustomize $TempDir
        $previousPythonPath = $env:PYTHONPATH
        try {
            if ([string]::IsNullOrEmpty($previousPythonPath)) {
                $env:PYTHONPATH = $pytestSiteDir
            }
            else {
                $env:PYTHONPATH = "$pytestSiteDir;$previousPythonPath"
            }
            Invoke-TrainingPython @(
                "-m", "pytest",
                "backend/bot_trainer/v2/test_rl_dataset.py",
                "backend/bot_trainer/v2/test_model.py",
                "backend/bot_trainer/v2/test_dataset.py",
                "-q",
                "-p", "no:cacheprovider",
                "--basetemp", (Join-Path $TempDir "pytest")
            )
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        }
        finally {
            $env:PYTHONPATH = $previousPythonPath
        }
    }

    # Iterative Self-Play Loop
    $currentOnnx = $BaselineOnnx
    $currentCheckpoint = $BaselineCheckpoint
    $bestOnnx = $BaselineOnnx
    $bestCheckpoint = $BaselineCheckpoint
    $bestScoreMargin = 0.0
    $bestIteration = 0
    $iterationHistory = @()

    for ($iter = 1; $iter -le $Iterations; $iter++) {
        $iterTag = "iter_{0:D3}" -f $iter
        $iterDir = Join-Path $OutputDir $iterTag
        $iterTrajectoryConfigDir = Join-Path $iterDir "trajectory_configs"
        $iterTrajectoryJsonl = Join-Path $iterDir "trajectories.jsonl"
        $iterCheckpointDir = Join-Path $iterDir "checkpoints"
        $iterCandidateOnnx = Join-Path $iterDir "candidate.onnx"
        $iterEvalDir = Join-Path $iterDir "eval"
        $iterSeed = $Seed + ($iter - 1) * 1000000

        Write-Host ""
        $currentOnnxLeaf = Split-Path -Leaf $currentOnnx
        Write-Host "==============================================================="
        Write-Host ("  Iteration {0} / {1}  (rollout model: {2})" -f $iter, $Iterations, $currentOnnxLeaf)
        Write-Host "==============================================================="

        # Step 1: Generate trajectories with current best model
        New-Item -ItemType Directory -Force -Path $iterTrajectoryConfigDir | Out-Null
        Invoke-TrainingPython @(
            "backend/bot_trainer/v2/league_config.py",
            "--pool", $OpponentPool,
            "--output-dir", $iterTrajectoryConfigDir,
            "--matches", "$IterationMatches",
            "--seed", "$iterSeed",
            "--max-actions", "$MaxActionsPerMatch",
            "--mode", "trajectory",
            "--rollout-onnx", $currentOnnx
        )
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

        $trajectoryFiles = @()
        Get-ChildItem -LiteralPath $iterTrajectoryConfigDir -Filter "trajectory_config_*.json" |
            Sort-Object Name |
            ForEach-Object {
                $index = [System.IO.Path]::GetFileNameWithoutExtension($_.Name).Replace("trajectory_config_", "")
                $partialReport = Join-Path $iterDir "trajectory_arena_report_$index.jsonl"
                $partialTrajectory = Join-Path $iterDir "trajectories_$index.jsonl"
                $trajectoryFiles += $partialTrajectory
                $arenaArgs = @(
                    "run",
                    "--manifest-path", "backend/Cargo.toml",
                    "--release",
                    "--bin", "bot_arena",
                    "--",
                    "--config", $_.FullName,
                    "--output", $partialReport,
                    "--trajectories", $partialTrajectory,
                    "--jobs", "$ArenaJobs"
                )
                if ($TrajectoryProgressEvery -gt 0) {
                    $arenaArgs += @("--progress-every", "$TrajectoryProgressEvery")
                }
                & $CargoExe @arenaArgs
                if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
            }
        if ($trajectoryFiles.Count -eq 0) {
            throw "No trajectory configs generated in $iterTrajectoryConfigDir"
        }
        Get-Content -LiteralPath $trajectoryFiles | Set-Content -Encoding UTF8 $iterTrajectoryJsonl

        # Step 2: PPO training from current checkpoint
        $rlTrainArgs = @(
            "backend/bot_trainer/v2/rl_train.py",
            "--trajectories", $iterTrajectoryJsonl,
            "--checkpoint", $currentCheckpoint,
            "--epochs", "$Epochs",
            "--batch-size", "$BatchSize",
            "--lr", "$LearningRate",
            "--gamma", "$Gamma",
            "--gae-lambda", "$GaeLambda",
            "--policy-id", $LearnerPolicyId,
            "--clip-epsilon", "$ClipEpsilon",
            "--value-clip-epsilon", "$ValueClipEpsilon",
            "--entropy-coef", "$EntropyCoef",
            "--entropy-end-coef", "$EntropyEndCoef",
            "--kl-coef", "$KlCoef",
            "--kl-end-coef", "$KlEndCoef",
            "--target-kl", "$TargetKl",
            "--output", $iterCheckpointDir,
            "--device", $Device
        )
        if ($EntropyDecaySteps -gt 0) {
            $rlTrainArgs += @("--entropy-decay-steps", "$EntropyDecaySteps")
        }
        if ($RecomputeOldPolicyStats) {
            $rlTrainArgs += @("--recompute-old-policy-stats")
        }
        Invoke-TrainingPython $rlTrainArgs
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

        # Step 3: Export ONNX
        $iterBestPt = Join-Path $iterCheckpointDir "best.pt"
        $selectedCheckpoint = $iterBestPt
        $selectedOnnx = $iterCandidateOnnx
        $selectedGate = $null
        $selectedSummary = $null
        if (-not $SkipOnnxExport) {
            Invoke-TrainingPython @(
                "backend/bot_trainer/v2/export_onnx.py",
                "--checkpoint", $iterBestPt,
                "--output", $iterCandidateOnnx
            )
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        }

        if (($CandidateSelectionMode -eq "epoch") -and (-not $SkipOnnxExport) -and (-not $SkipEval)) {
            $candidateManifest = Join-Path $iterDir "candidate_manifest.json"
            $candidateSelection = Join-Path $iterDir "candidate_selection.json"
            $candidateEntries = @()
            Get-ChildItem -LiteralPath $iterCheckpointDir -Filter "epoch_*.pt" |
                Sort-Object Name |
                ForEach-Object {
                    $epochName = [System.IO.Path]::GetFileNameWithoutExtension($_.Name)
                    $epochOnnx = Join-Path $iterDir "$epochName.onnx"
                    $epochEvalDir = Join-Path $iterEvalDir $epochName
                    Invoke-TrainingPython @(
                        "backend/bot_trainer/v2/export_onnx.py",
                        "--checkpoint", $_.FullName,
                        "--output", $epochOnnx
                    )
                    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
                    $epochEval = Invoke-CandidateEvaluation `
                        -CandidateModel $epochOnnx `
                        -EvalDir $epochEvalDir `
                        -EvalBaselineOnnx $BaselineOnnx
                    $epochNumber = [int]$epochName.Replace("epoch_", "")
                    $candidateEntries += [ordered]@{
                        epoch = $epochNumber
                        checkpoint = $_.FullName
                        onnx = $epochOnnx
                        summary = $epochEval.summary
                        gate_path = $epochEval.gate
                    }
                }
            Write-Utf8NoBom -Path $candidateManifest -Content ((@{ candidates = $candidateEntries } | ConvertTo-Json -Depth 12) + "`n")
            Invoke-TrainingPython @(
                "backend/bot_trainer/v2/candidate_selector.py",
                "--manifest", $candidateManifest,
                "--output", $candidateSelection
            )
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
            $selection = Get-Content -LiteralPath $candidateSelection -Raw | ConvertFrom-Json
            $selectedCheckpoint = [string]$selection.checkpoint
            $selectedOnnx = [string]$selection.onnx
            $selectedGate = [string]$selection.selected.gate
            $selectedSummary = [string]$selection.selected.summary
            Copy-RequiredFile -SourcePath $selectedOnnx -TargetPath $iterCandidateOnnx
            if (-not [string]::IsNullOrWhiteSpace($selectedGate)) {
                Copy-RequiredFile -SourcePath $selectedGate -TargetPath (Join-Path $iterEvalDir "candidate_gate.json")
            }
            if (-not [string]::IsNullOrWhiteSpace($selectedSummary)) {
                Copy-RequiredFile -SourcePath $selectedSummary -TargetPath (Join-Path $iterEvalDir "candidate_eval_summary.json")
            }
        }

        # Step 4: Evaluate candidate vs original baseline
        $iterResult = [ordered]@{
            iteration = $iter
            checkpoint = $selectedCheckpoint
            onnx = $iterCandidateOnnx
            accepted = $false
            score_margin = 0.0
        }

        if ((-not $SkipOnnxExport) -and (-not $SkipEval)) {
            if ($CandidateSelectionMode -eq "epoch") {
                $evalResult = [ordered]@{
                    gate = (Join-Path $iterEvalDir "candidate_gate.json")
                }
            }
            else {
                $evalResult = Invoke-CandidateEvaluation `
                    -CandidateModel $iterCandidateOnnx `
                    -EvalDir $iterEvalDir `
                    -EvalBaselineOnnx $BaselineOnnx
            }

            $gateOutput = Get-Content -LiteralPath $evalResult.gate -Raw | ConvertFrom-Json
            $iterResult.accepted = [bool]$gateOutput.accepted
            $scoreMargin = $gateOutput.candidate.avg_score_delta - $gateOutput.baseline.avg_score_delta
            $iterResult.score_margin = [math]::Round($scoreMargin, 4)

            Write-Host ("  Iteration {0} result: score_margin={1} accepted={2}" -f $iter, $iterResult.score_margin, $iterResult.accepted)

            # Update the rollout model only when gate passes or the candidate improves best margin.
            if ($iterResult.accepted -or ($iterResult.score_margin -gt $bestScoreMargin)) {
                $bestScoreMargin = $iterResult.score_margin
                $bestCheckpoint = $selectedCheckpoint
                $bestOnnx = $iterCandidateOnnx
                $bestIteration = $iter
                $currentCheckpoint = $selectedCheckpoint
                $currentOnnx = $iterCandidateOnnx
                Write-Host ("  Rollout advanced to candidate (score_margin={0}, accepted={1})" -f $iterResult.score_margin, $iterResult.accepted)
            }
            else {
                Write-Host ("  Rollout kept current best (best_score_margin={0})" -f $bestScoreMargin)
            }
        }

        $iterationHistory += $iterResult
    }

    # Finalize: copy best results to top-level
    $FinalCandidateOnnx = Join-Path $OutputDir "candidate.onnx"
    $FinalCheckpointDir = Join-Path $OutputDir "checkpoints"
    $FinalEvalSummary = Join-Path $OutputDir "candidate_eval_summary.json"
    $FinalGateOutput = Join-Path $OutputDir "candidate_gate.json"

    New-Item -ItemType Directory -Force -Path $FinalCheckpointDir | Out-Null
    Copy-RequiredFile -SourcePath $bestCheckpoint -TargetPath (Join-Path $FinalCheckpointDir "best.pt")
    if (-not $SkipOnnxExport) {
        Copy-RequiredFile -SourcePath $bestOnnx -TargetPath $FinalCandidateOnnx
    }

    # Copy eval results from the best iteration
    $bestIterTag = if ($bestIteration -gt 0) { "iter_{0:D3}" -f $bestIteration } else { "baseline" }
    if ($bestIteration -gt 0) {
        $bestIterEvalDir = Join-Path $OutputDir "$bestIterTag/eval"
        if (Test-Path -LiteralPath "$bestIterEvalDir/candidate_eval_summary.json" -PathType Leaf) {
            Copy-RequiredFile -SourcePath "$bestIterEvalDir/candidate_eval_summary.json" -TargetPath $FinalEvalSummary
        }
        if (Test-Path -LiteralPath "$bestIterEvalDir/candidate_gate.json" -PathType Leaf) {
            Copy-RequiredFile -SourcePath "$bestIterEvalDir/candidate_gate.json" -TargetPath $FinalGateOutput
        }
    }
    elseif ($iterationHistory.Count -gt 0) {
        $lastIterTag = "iter_{0:D3}" -f $iterationHistory[-1].iteration
        $lastIterEvalDir = Join-Path $OutputDir "$lastIterTag/eval"
        if (Test-Path -LiteralPath "$lastIterEvalDir/candidate_eval_summary.json" -PathType Leaf) {
            Copy-RequiredFile -SourcePath "$lastIterEvalDir/candidate_eval_summary.json" -TargetPath $FinalEvalSummary
        }
        if (Test-Path -LiteralPath "$lastIterEvalDir/candidate_gate.json" -PathType Leaf) {
            Copy-RequiredFile -SourcePath "$lastIterEvalDir/candidate_gate.json" -TargetPath $FinalGateOutput
        }
    }

    # Write iteration history
    $historyPath = Join-Path $OutputDir "iteration_history.json"
    Write-Utf8NoBom -Path $historyPath -Content ((@($iterationHistory) | ConvertTo-Json -Depth 8) + "`n")

    $finalAccepted = ($iterationHistory | Where-Object { $_.accepted }) | Select-Object -First 1
    if ($EnforceCandidateGate -and $null -eq $finalAccepted) {
        Write-Warning "No iteration passed the candidate gate."
        exit 1
    }
    if ((-not $EnforceCandidateGate) -and $null -eq $finalAccepted) {
        Write-Warning "No iteration passed the candidate gate. Best score_margin=$bestScoreMargin. See $FinalGateOutput"
    }

    Write-Host ""
    Write-Host "RL iterative self-play pipeline finished."
    Write-Host "Iterations:     $Iterations"
    Write-Host ("Best iteration: {0} (score_margin={1})" -f $bestIterTag, $bestScoreMargin)
    Write-Host "Checkpoint:     $bestCheckpoint"
    Write-Host "Candidate:      $FinalCandidateOnnx"
    Write-Host "History:        $historyPath"
}
finally {
    Pop-Location
    $env:TEMP = Resolve-UsableTempPath $PreviousTemp
    $env:TMP = Resolve-UsableTempPath $PreviousTmp
    if ([string]::IsNullOrWhiteSpace($PreviousPytestTempRoot) -or -not (Test-Path -LiteralPath $PreviousPytestTempRoot -PathType Container)) {
        Remove-Item Env:PYTEST_DEBUG_TEMPROOT -ErrorAction SilentlyContinue
    }
    else {
        $env:PYTEST_DEBUG_TEMPROOT = (Resolve-Path -LiteralPath $PreviousPytestTempRoot).Path
    }
}
