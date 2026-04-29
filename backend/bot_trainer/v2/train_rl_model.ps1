param(
    [string]$OutputDir = "backend/bot_trainer/v2/rl_runs/latest",
    [string]$BaselineCheckpoint = "backend/bot_trainer/v2/checkpoints/best.pt",
    [string]$BaselineOnnx = "backend/assets/models/mahjong_policy_net.onnx",
    [string]$PythonExe = "python",
    [string]$PythonVersion = "",
    [string]$CargoExe = "cargo",
    [int]$ArenaJobs = 0,
    [int]$TrajectoryMatches = 200,
    [int]$TrajectoryProgressEvery = 20,
    [int]$EvalMatches = 200,
    [int]$Seed = 20260429,
    [int]$MaxActionsPerMatch = 2400,
    [int]$Epochs = 3,
    [int]$BatchSize = 256,
    [double]$LearningRate = 0.00001,
    [double]$Gamma = 0.99,
    [double]$GaeLambda = 0.95,
    [double]$ClipEpsilon = 0.2,
    [double]$ValueClipEpsilon = 0.2,
    [double]$EntropyCoef = 0.02,
    [double]$EntropyEndCoef = 0.005,
    [int]$EntropyDecaySteps = 0,
    [double]$KlCoef = 0.01,
    [double]$KlEndCoef = 0.0,
    [string]$Device = "auto",
    [string]$OpponentPool = "backend/bot_trainer/v2/opponent_pool.json",
    [string]$LearnerPolicyId = "learner",
    [string]$SelfPlayPolicyId = "selfplay_neural",
    [ValidateSet("heuristic", "neural")]
    [string]$SelfPlayPolicyMode = "neural",
    [switch]$SkipTests,
    [switch]$SkipTrajectoryGeneration,
    [switch]$SkipOnnxExport,
    [switch]$SkipEval,
    [switch]$EnforceCandidateGate,
    [switch]$RecomputeOldPolicyStats
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
    $TrajectoryConfigDir = Join-Path $OutputDir "trajectory_configs"
    $TrajectoryJsonl = Join-Path $OutputDir "trajectories.jsonl"
    $ArenaReportJsonl = Join-Path $OutputDir "trajectory_arena_report.jsonl"
    $CheckpointDir = Join-Path $OutputDir "checkpoints"
    $CandidateOnnx = Join-Path $OutputDir "candidate.onnx"
    $EvalConfig = Join-Path $OutputDir "candidate_eval_config.json"
    $EvalJsonl = Join-Path $OutputDir "candidate_eval.jsonl"
    $EvalSummary = Join-Path $OutputDir "candidate_eval_summary.json"
    $GateOutput = Join-Path $OutputDir "candidate_gate.json"

    Write-Host "Mahjong RL training"
    Write-Host "Output:              $OutputDir"
    Write-Host "Baseline checkpoint: $BaselineCheckpoint"
    Write-Host "Baseline ONNX:       $BaselineOnnx"
    Write-Host "Trajectory matches:  $TrajectoryMatches"
    Write-Host "Trajectory progress: every $TrajectoryProgressEvery match(es)"
    Write-Host "Opponent pool:       $OpponentPool"
    Write-Host "Learner policy id:   $LearnerPolicyId"
    Write-Host "Eval matches:        $EvalMatches"
    Write-Host "Device:              $Device"
    Write-Host "Python:              $PythonExe $PythonVersion"
    Write-Host "Cargo:               $CargoExe"
    Write-Host "Arena jobs:          $(if ($ArenaJobs -eq 0) { "auto" } else { $ArenaJobs })"

    Assert-FileExists `
        $BaselineCheckpoint `
        "Baseline checkpoint" `
        "Run supervised training first with backend/bot_trainer/v2/train_and_export_model.ps1, or pass -BaselineCheckpoint <existing .pt file>."
    Assert-FileExists `
        $BaselineOnnx `
        "Baseline ONNX model" `
        "Export the supervised model first, or pass -BaselineOnnx <existing .onnx file>."

    if ($SkipTrajectoryGeneration) {
        Assert-FileExists `
            $TrajectoryJsonl `
            "Trajectory JSONL" `
            "Remove -SkipTrajectoryGeneration, or place an existing trajectories.jsonl at $TrajectoryJsonl."
    }

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

    if (-not $SkipTrajectoryGeneration) {
        New-Item -ItemType Directory -Force -Path $TrajectoryConfigDir | Out-Null
        Invoke-TrainingPython @(
            "backend/bot_trainer/v2/league_config.py",
            "--pool", $OpponentPool,
            "--output-dir", $TrajectoryConfigDir,
            "--matches", "$TrajectoryMatches",
            "--seed", "$Seed",
            "--max-actions", "$MaxActionsPerMatch",
            "--mode", "trajectory",
            "--rollout-onnx", $BaselineOnnx
        )
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

        $trajectoryFiles = @()
        Get-ChildItem -LiteralPath $TrajectoryConfigDir -Filter "trajectory_config_*.json" |
            Sort-Object Name |
            ForEach-Object {
                $index = [System.IO.Path]::GetFileNameWithoutExtension($_.Name).Replace("trajectory_config_", "")
                $partialReport = Join-Path $OutputDir "trajectory_arena_report_$index.jsonl"
                $partialTrajectory = Join-Path $OutputDir "trajectories_$index.jsonl"
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
            throw "No trajectory configs generated in $TrajectoryConfigDir"
        }
        Get-Content -LiteralPath $trajectoryFiles | Set-Content -Encoding UTF8 $TrajectoryJsonl
    }

    $rlTrainArgs = @(
        "backend/bot_trainer/v2/rl_train.py",
        "--trajectories", $TrajectoryJsonl,
        "--checkpoint", $BaselineCheckpoint,
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
        "--output", $CheckpointDir,
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

    if (-not $SkipOnnxExport) {
        Invoke-TrainingPython @(
            "backend/bot_trainer/v2/export_onnx.py",
            "--checkpoint", (Join-Path $CheckpointDir "best.pt"),
            "--output", $CandidateOnnx
        )
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }

    if (-not $SkipEval) {
        Invoke-TrainingPython @(
            "backend/bot_trainer/v2/league_config.py",
            "--pool", $OpponentPool,
            "--output-dir", $OutputDir,
            "--matches", "$EvalMatches",
            "--seed", "$Seed",
            "--max-actions", "$MaxActionsPerMatch",
            "--mode", "eval",
            "--candidate-onnx", $CandidateOnnx,
            "--baseline-onnx", $BaselineOnnx
        )
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

        & $CargoExe run --manifest-path backend/Cargo.toml --release --bin bot_arena -- --config $EvalConfig --output $EvalJsonl --jobs $ArenaJobs
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

        Invoke-TrainingPython @(
            "backend/bot_trainer/v2/arena_summary.py",
            "--input", $EvalJsonl,
            "--output", $EvalSummary
        )
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

        Invoke-TrainingPython @(
            "backend/bot_trainer/v2/candidate_gate.py",
            "--summary", $EvalSummary,
            "--baseline-policy", "baseline_neural",
            "--candidate-policy", "rl_candidate_neural",
            "--output", $GateOutput
        )
        $gateExit = $LASTEXITCODE
        if ($EnforceCandidateGate -and $gateExit -ne 0) {
            exit $gateExit
        }
        if (-not $EnforceCandidateGate -and $gateExit -ne 0) {
            Write-Warning "Candidate gate rejected this model. See $GateOutput"
        }
    }

    Write-Host "RL training pipeline finished."
    Write-Host "Checkpoint:  $(Join-Path $CheckpointDir "best.pt")"
    Write-Host "Candidate:   $CandidateOnnx"
    Write-Host "Evaluation:  $EvalJsonl"
    Write-Host "Summary:     $EvalSummary"
}
finally {
    Pop-Location
}
