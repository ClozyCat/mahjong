param(
    [string]$OutputDir = "backend/bot_trainer/v2/rl_runs/latest",
    [string]$BaselineCheckpoint = "backend/bot_trainer/v2/checkpoints/best.pt",
    [string]$BaselineOnnx = "backend/assets/models/mahjong_policy_net.onnx",
    [string]$PythonExe = "python",
    [string]$PythonVersion = "",
    [string]$CargoExe = "cargo",
    [int]$TrajectoryMatches = 200,
    [int]$TrajectoryProgressEvery = 1,
    [int]$EvalMatches = 200,
    [int]$Seed = 20260429,
    [int]$MaxActionsPerMatch = 2400,
    [int]$Epochs = 3,
    [int]$BatchSize = 256,
    [double]$LearningRate = 0.00001,
    [double]$Gamma = 0.99,
    [double]$ClipEpsilon = 0.2,
    [string]$Device = "auto",
    [string]$SelfPlayPolicyId = "selfplay_hybrid30",
    [ValidateSet("heuristic", "hybrid", "neural")]
    [string]$SelfPlayPolicyMode = "hybrid",
    [int]$SelfPlayNeuralWeight = 30,
    [switch]$SkipTests,
    [switch]$SkipTrajectoryGeneration,
    [switch]$SkipOnnxExport,
    [switch]$SkipEval
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

function Write-Utf8NoBom {
    param(
        [string]$Path,
        [string]$Content
    )
    $encoding = New-Object System.Text.UTF8Encoding $false
    [System.IO.File]::WriteAllText((Resolve-Path -LiteralPath (Split-Path -Parent $Path)).Path + [System.IO.Path]::DirectorySeparatorChar + (Split-Path -Leaf $Path), $Content, $encoding)
}

Push-Location $RepoRoot
try {
    Assert-PythonModule "torch"
    Assert-PythonModule "onnxruntime"

    New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
    $TrajectoryConfig = Join-Path $OutputDir "trajectory_config.json"
    $TrajectoryJsonl = Join-Path $OutputDir "trajectories.jsonl"
    $ArenaReportJsonl = Join-Path $OutputDir "trajectory_arena_report.jsonl"
    $CheckpointDir = Join-Path $OutputDir "checkpoints"
    $CandidateOnnx = Join-Path $OutputDir "candidate.onnx"
    $EvalConfig = Join-Path $OutputDir "candidate_eval_config.json"
    $EvalJsonl = Join-Path $OutputDir "candidate_eval.jsonl"

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

    if (-not $SkipTests) {
        Invoke-TrainingPython @("-m", "pytest", "backend/bot_trainer/v2/test_rl_dataset.py", "backend/bot_trainer/v2/test_dataset.py", "-q")
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
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
            "--trajectories", $TrajectoryJsonl
        )
        if ($TrajectoryProgressEvery -gt 0) {
            $arenaArgs += @("--progress-every", "$TrajectoryProgressEvery")
        }
        & $CargoExe @arenaArgs
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }

    Invoke-TrainingPython @(
        "backend/bot_trainer/v2/rl_train.py",
        "--trajectories", $TrajectoryJsonl,
        "--checkpoint", $BaselineCheckpoint,
        "--epochs", "$Epochs",
        "--batch-size", "$BatchSize",
        "--lr", "$LearningRate",
        "--gamma", "$Gamma",
        "--clip-epsilon", "$ClipEpsilon",
        "--output", $CheckpointDir,
        "--device", $Device
    )
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
                    id = "baseline_hybrid30"
                    mode = "hybrid"
                    neural_weight = 30
                    model_path = $BaselineOnnx
                },
                @{
                    id = "rl_candidate_hybrid30"
                    mode = "hybrid"
                    neural_weight = 30
                    model_path = $CandidateOnnx
                }
            )
        }
        Write-Utf8NoBom $EvalConfig ($evalConfigObject | ConvertTo-Json -Depth 8)

        & $CargoExe run --manifest-path backend/Cargo.toml --release --bin bot_arena -- --config $EvalConfig --output $EvalJsonl
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }

    Write-Host "RL training pipeline finished."
    Write-Host "Checkpoint:  $(Join-Path $CheckpointDir "best.pt")"
    Write-Host "Candidate:   $CandidateOnnx"
    Write-Host "Evaluation:  $EvalJsonl"
}
finally {
    Pop-Location
}
