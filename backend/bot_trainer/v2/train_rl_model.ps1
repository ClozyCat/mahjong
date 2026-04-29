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
    [double]$ClipEpsilon = 0.2,
    [double]$EntropyCoef = 0.02,
    [double]$EntropyEndCoef = 0.005,
    [int]$EntropyDecaySteps = 0,
    [string]$Device = "auto",
    [string]$SelfPlayPolicyId = "selfplay_hybrid30",
    [ValidateSet("heuristic", "hybrid", "neural")]
    [string]$SelfPlayPolicyMode = "hybrid",
    [int]$SelfPlayNeuralWeight = 30,
    [switch]$SkipTests,
    [switch]$SkipTrajectoryGeneration,
    [switch]$SkipOnnxExport,
    [switch]$SkipEval,
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
    $TrajectoryConfig = Join-Path $OutputDir "trajectory_config.json"
    $TrajectoryJsonl = Join-Path $OutputDir "trajectories.jsonl"
    $ArenaReportJsonl = Join-Path $OutputDir "trajectory_arena_report.jsonl"
    $CheckpointDir = Join-Path $OutputDir "checkpoints"
    $CandidateOnnx = Join-Path $OutputDir "candidate.onnx"
    $EvalConfig = Join-Path $OutputDir "candidate_eval_config.json"
    $EvalJsonl = Join-Path $OutputDir "candidate_eval.jsonl"
    $EvalSummary = Join-Path $OutputDir "candidate_eval_summary.json"

    Write-Host "Mahjong RL training"
    Write-Host "Output:              $OutputDir"
    Write-Host "Baseline checkpoint: $BaselineCheckpoint"
    Write-Host "Baseline ONNX:       $BaselineOnnx"
    Write-Host "Trajectory matches:  $TrajectoryMatches"
    Write-Host "Trajectory progress: every $TrajectoryProgressEvery match(es)"
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
        $trajectoryConfigObject = @{
            matches = $TrajectoryMatches
            seed = $Seed
            max_actions_per_match = $MaxActionsPerMatch
            report_trajectories = $true
            policies = @(
                @{
                    id = $SelfPlayPolicyId
                    mode = $SelfPlayPolicyMode
                    neural_weight = $SelfPlayNeuralWeight
                    model_path = $BaselineOnnx
                }
            )
        }
        Write-Utf8NoBom $TrajectoryConfig ($trajectoryConfigObject | ConvertTo-Json -Depth 8)

        $arenaArgs = @(
            "run",
            "--manifest-path", "backend/Cargo.toml",
            "--release",
            "--bin", "bot_arena",
            "--",
            "--config", $TrajectoryConfig,
            "--output", $ArenaReportJsonl,
            "--trajectories", $TrajectoryJsonl,
            "--jobs", "$ArenaJobs"
        )
        if ($TrajectoryProgressEvery -gt 0) {
            $arenaArgs += @("--progress-every", "$TrajectoryProgressEvery")
        }
        & $CargoExe @arenaArgs
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }

    $rlTrainArgs = @(
        "backend/bot_trainer/v2/rl_train.py",
        "--trajectories", $TrajectoryJsonl,
        "--checkpoint", $BaselineCheckpoint,
        "--epochs", "$Epochs",
        "--batch-size", "$BatchSize",
        "--lr", "$LearningRate",
        "--gamma", "$Gamma",
        "--clip-epsilon", "$ClipEpsilon",
        "--entropy-coef", "$EntropyCoef",
        "--entropy-end-coef", "$EntropyEndCoef",
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
        $evalConfigObject = @{
            matches = $EvalMatches
            seed = $Seed
            max_actions_per_match = $MaxActionsPerMatch
            report_trajectories = $false
            policies = @(
                @{
                    id = "baseline_$SelfPlayPolicyId"
                    mode = $SelfPlayPolicyMode
                    neural_weight = $SelfPlayNeuralWeight
                    model_path = $BaselineOnnx
                },
                @{
                    id = "rl_candidate_neural"
                    mode = "neural"
                    neural_weight = 0
                    model_path = $CandidateOnnx
                }
            )
        }
        Write-Utf8NoBom $EvalConfig ($evalConfigObject | ConvertTo-Json -Depth 8)

        & $CargoExe run --manifest-path backend/Cargo.toml --release --bin bot_arena -- --config $EvalConfig --output $EvalJsonl --jobs $ArenaJobs
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

        Invoke-TrainingPython @(
            "backend/bot_trainer/v2/arena_summary.py",
            "--input", $EvalJsonl,
            "--output", $EvalSummary
        )
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
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
