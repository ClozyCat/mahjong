param(
    [string]$OutputDir = "backend/bot_trainer/v2/rl_runs/$(Get-Date -Format 'yyyyMMddHHmm')",
    [string]$BaselineCheckpoint = "backend/bot_trainer/v2/checkpoints/actor_critic_bootstrap.pt",
    [string]$BaselineOnnx = "backend/assets/ppo/actor_critic_bootstrap.onnx",
    [string]$PythonExe = "python",
    [string]$PythonVersion = "",
    [string]$CargoExe = "cargo",
    [int]$ArenaJobs = 1,
    [int]$EpochEvalJobs = 1,
    [int]$Iterations = 5,
    [int]$IterationMatches = 1500,
    [int]$EvalMatches = 1000,
    [int]$Seed = 20260429,
    [int]$MaxActionsPerMatch = 2400,
    [int]$Epochs = 1,
    [int]$BatchSize = 2048,
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
    [ValidateSet("ppo")]
    [string]$Policy = "ppo",
    [string[]]$Policies = @(),
    [switch]$UseActorCritic,
    [double]$CriticLrMultiplier = 2.0,
    [string]$Device = "auto",
    [string]$OpponentPool = "backend/bot_trainer/v2/opponent_pool.json",
    [string]$LearnerPolicyId = "learner",
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
    $evalConfigArgs = @(
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
    Invoke-TrainingPython $evalConfigArgs
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

function Write-CandidateEntry {
    param(
        [string]$Path,
        [string]$PolicyName,
        [int]$EpochNumber,
        [string]$Checkpoint,
        [string]$Onnx,
        [string]$Summary,
        [string]$Gate
    )

    $entry = [ordered]@{
        policy = $PolicyName
        epoch = $EpochNumber
        checkpoint = $Checkpoint
        onnx = $Onnx
        summary = $Summary
        gate_path = $Gate
    }
    Write-Utf8NoBom -Path $Path -Content (($entry | ConvertTo-Json -Depth 8 -Compress) + "`n")
}

$EpochEvaluationScript = {
    param(
        [string]$RepoRoot,
        [string]$PythonExe,
        [string]$PythonVersion,
        [string]$CargoExe,
        [string]$OpponentPool,
        [int]$EvalMatches,
        [int]$Seed,
        [int]$MaxActionsPerMatch,
        [int]$ArenaJobs,
        [string]$PolicyName,
        [string]$EpochPt,
        [string]$PolicyDir,
        [string]$PolicyEvalDir,
        [string]$BaselineOnnx,
        [string]$EntryPath
    )

    $ErrorActionPreference = "Stop"
    Set-Location $RepoRoot
    function Invoke-JobPython {
        param([string[]]$Arguments)
        if ($PythonExe -eq "py" -and $PythonVersion.Length -gt 0) {
            & $PythonExe "-$PythonVersion" @Arguments 2>&1
        }
        else {
            & $PythonExe @Arguments 2>&1
        }
    }
    function Write-JobUtf8NoBom {
        param([string]$Path, [string]$Content)
        $encoding = New-Object System.Text.UTF8Encoding $false
        [System.IO.File]::WriteAllText((Resolve-Path -LiteralPath (Split-Path -Parent $Path)).Path + [System.IO.Path]::DirectorySeparatorChar + (Split-Path -Leaf $Path), $Content, $encoding)
    }
    function Assert-JobCommandSucceeded {
        param(
            [string]$StepName,
            [int]$ExitCode
        )
        if ($ExitCode -ne 0) {
            throw "Epoch evaluation step failed: $StepName (exit code $ExitCode)"
        }
    }

    $epochName = [System.IO.Path]::GetFileNameWithoutExtension($EpochPt)
    $epochNumber = [int]$epochName.Replace("epoch_", "")
    $epochOnnx = Join-Path $PolicyDir "$epochName.onnx"
    $epochEvalDir = Join-Path $PolicyEvalDir $epochName
    Invoke-JobPython @("backend/bot_trainer/v2/export_onnx.py", "--checkpoint", $EpochPt, "--output", $epochOnnx)
    Assert-JobCommandSucceeded "export_onnx.py" $LASTEXITCODE

    New-Item -ItemType Directory -Force -Path $epochEvalDir | Out-Null
    Invoke-JobPython @(
        "backend/bot_trainer/v2/league_config.py",
        "--pool", $OpponentPool,
        "--output-dir", $epochEvalDir,
        "--matches", "$EvalMatches",
        "--seed", "$Seed",
        "--max-actions", "$MaxActionsPerMatch",
        "--mode", "eval",
        "--candidate-onnx", $epochOnnx,
        "--baseline-onnx", $BaselineOnnx
    )
    Assert-JobCommandSucceeded "league_config.py" $LASTEXITCODE

    $localConfig = Join-Path $epochEvalDir "candidate_eval_config.json"
    $localJsonl = Join-Path $epochEvalDir "candidate_eval.jsonl"
    $localSummary = Join-Path $epochEvalDir "candidate_eval_summary.json"
    $localGate = Join-Path $epochEvalDir "candidate_gate.json"
    & $CargoExe run --manifest-path backend/Cargo.toml --release --bin bot_arena -- --config $localConfig --output $localJsonl --jobs $ArenaJobs 2>&1
    Assert-JobCommandSucceeded "bot_arena" $LASTEXITCODE
    Invoke-JobPython @("backend/bot_trainer/v2/arena_summary.py", "--input", $localJsonl, "--output", $localSummary)
    Assert-JobCommandSucceeded "arena_summary.py" $LASTEXITCODE
    Invoke-JobPython @(
        "backend/bot_trainer/v2/candidate_gate.py",
        "--summary", $localSummary,
        "--baseline-policy", "baseline_neural",
        "--candidate-policy", "rl_candidate_neural",
        "--output", $localGate
    )

    $entry = [ordered]@{
        policy = $PolicyName
        epoch = $epochNumber
        checkpoint = $EpochPt
        onnx = $epochOnnx
        summary = $localSummary
        gate_path = $localGate
    }
    Write-JobUtf8NoBom -Path $EntryPath -Content (($entry | ConvertTo-Json -Depth 8 -Compress) + "`n")
}

function Resolve-ActivePolicies {
    $sourcePolicies = if ($Policies.Count -gt 0) { $Policies } else { @($Policy) }
    $activePolicyList = @()
    $validPolicies = @("ppo")
    foreach ($policyValue in $sourcePolicies) {
        $policyParts = [string]$policyValue -split ","; foreach ($policyName in $policyParts) {
            $normalizedPolicy = $policyName.Trim()
            if ([string]::IsNullOrWhiteSpace($normalizedPolicy)) {
                continue
            }
            if ($validPolicies -notcontains $normalizedPolicy) {
                throw "Invalid policy '$normalizedPolicy'. Expected one of: $($validPolicies -join ', ')"
            }
            if ($activePolicyList -notcontains $normalizedPolicy) {
                $activePolicyList += $normalizedPolicy
            }
        }
    }
    return $activePolicyList
}

function Get-PolicyArtifactPaths {
    param(
        [string]$IterationDir,
        [string]$PolicyName,
        [bool]$UsePolicySubdir
    )

    $policyDir = if ($UsePolicySubdir) { Join-Path (Join-Path $IterationDir "policies") $PolicyName } else { $IterationDir }
    return [ordered]@{
        policy_dir = $policyDir
        checkpoint_dir = Join-Path $policyDir "checkpoints"
        candidate_onnx = Join-Path $policyDir "candidate.onnx"
        eval_dir = Join-Path $policyDir "eval"
    }
}

function Invoke-PolicyTraining {
    param(
        [string]$PolicyName,
        [string]$TrajectoryJsonl,
        [string]$Checkpoint,
        [string]$CheckpointDir
    )

    $rlTrainArgs = @(
        "backend/bot_trainer/v2/rl_train.py",
        "--trajectories", $TrajectoryJsonl,
        "--checkpoint", $Checkpoint,
        "--epochs", "$Epochs",
        "--batch-size", "$BatchSize",
        "--lr", "$LearningRate",
        "--critic-lr-multiplier", "$CriticLrMultiplier",
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
        "--policy", $PolicyName,
        "--output", $CheckpointDir,
        "--device", $Device
    )
    if ($EntropyDecaySteps -gt 0) {
        $rlTrainArgs += @("--entropy-decay-steps", "$EntropyDecaySteps")
    }
    if ($UseActorCritic) {
        $rlTrainArgs += @("--use-actor-critic")
    }
    if ($RecomputeOldPolicyStats) {
        $rlTrainArgs += @("--recompute-old-policy-stats")
    }

    Invoke-TrainingPython $rlTrainArgs
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
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
    $activePolicies = Resolve-ActivePolicies
    $multiPolicyTraining = $activePolicies.Count -gt 1
    Write-Host "Mahjong RL training (iterative self-play)"
    Write-Host "Output:              $OutputDir"
    Write-Host "Baseline checkpoint: $BaselineCheckpoint"
    Write-Host "Baseline ONNX:       $BaselineOnnx"
    Write-Host "Iterations:          $Iterations"
    Write-Host "Matches/iteration:   $IterationMatches"
    Write-Host "PPO epochs/iter:     $Epochs"
    Write-Host "Gamma:               $Gamma"
    Write-Host "KL coef:             $KlCoef"
    Write-Host "Actor-critic:        $([bool]$UseActorCritic)"
    Write-Host "Critic LR x:         $CriticLrMultiplier"
    Write-Host "Opponent pool:       $OpponentPool"
    Write-Host "Learner policy id:   $LearnerPolicyId"
    Write-Host "Eval matches:        $EvalMatches"
    Write-Host "Device:              $Device"
    Write-Host "Policies:            $($activePolicies -join ', ')"
    Write-Host "Python:              $PythonExe $PythonVersion"
    Write-Host "Cargo:               $CargoExe"
    Write-Host "Arena jobs:          $ArenaJobs"
    Write-Host "Epoch eval jobs:     $EpochEvalJobs"

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
    $checkpointArchitectureGuardArgs = @(
        "backend/bot_trainer/v2/checkpoint_architecture_guard.py",
        "--checkpoint", $BaselineCheckpoint
    )
    if ($UseActorCritic) {
        $checkpointArchitectureGuardArgs += @("--use-actor-critic")
    }
    Invoke-TrainingPython $checkpointArchitectureGuardArgs
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
    $policyStates = @{}
    foreach ($style in $activePolicies) {
        $policyStates[$style] = [ordered]@{
            current_onnx = $BaselineOnnx
            current_checkpoint = $BaselineCheckpoint
            best_onnx = $BaselineOnnx
            best_checkpoint = $BaselineCheckpoint
            best_score_margin = 0.0
            best_iteration = 0
            history = @()
        }
    }

    for ($iter = 1; $iter -le $Iterations; $iter++) {
        $iterTag = "iter_{0:D3}" -f $iter
        $iterDir = Join-Path $OutputDir $iterTag
        $iterSeed = $Seed + ($iter - 1) * 1000000

        Write-Host ""
        Write-Host "==============================================================="
        Write-Host ("  Iteration {0} / {1}" -f $iter, $Iterations)
        Write-Host "==============================================================="

        # Step 1/2/3/4: Generate trajectories, train, export, and evaluate each policy serially.
        foreach ($style in $activePolicies) {
            $paths = Get-PolicyArtifactPaths -IterationDir $iterDir -PolicyName $style -UsePolicySubdir $multiPolicyTraining
            $iterTrajectoryConfigDir = Join-Path $paths.policy_dir "trajectory_configs"
            $iterTrajectoryJsonl = Join-Path $paths.policy_dir "trajectories.jsonl"
            New-Item -ItemType Directory -Force -Path $paths.checkpoint_dir | Out-Null
            $iterCheckpointDir = [string]$paths.checkpoint_dir
            $iterCandidateOnnx = [string]$paths.candidate_onnx
            $iterEvalDir = [string]$paths.eval_dir
            $policyState = $policyStates[$style]
            $rolloutOnnx = [string]$policyState.current_onnx
            $currentOnnxLeaf = Split-Path -Leaf $rolloutOnnx

            Write-Host ("  Generating trajectories: policy={0} rollout={1}" -f $style, $currentOnnxLeaf)
            New-Item -ItemType Directory -Force -Path $iterTrajectoryConfigDir | Out-Null
            $trajectoryConfigArgs = @(
                "backend/bot_trainer/v2/league_config.py",
                "--pool", $OpponentPool,
                "--output-dir", $iterTrajectoryConfigDir,
                "--matches", "$IterationMatches",
                "--seed", "$iterSeed",
                "--max-actions", "$MaxActionsPerMatch",
                "--mode", "trajectory",
                "--rollout-onnx", $rolloutOnnx
            )
            Invoke-TrainingPython $trajectoryConfigArgs
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

            $trajectoryConfigPath = Join-Path $iterTrajectoryConfigDir "trajectory_config_0.json"
            if (-not (Test-Path -LiteralPath $trajectoryConfigPath -PathType Leaf)) {
                throw "No trajectory config generated at $trajectoryConfigPath"
            }
            $trajectoryReport = Join-Path $paths.policy_dir "trajectory_arena_report.jsonl"
            $arenaArgs = @(
                "run",
                "--manifest-path", "backend/Cargo.toml",
                "--release",
                "--bin", "bot_arena",
                "--",
                "--config", $trajectoryConfigPath,
                "--output", $trajectoryReport,
                "--trajectories", $iterTrajectoryJsonl,
                "--jobs", "$ArenaJobs"
            )
            & $CargoExe @arenaArgs
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

            Write-Host ("  Starting PPO training: policy={0}" -f $style)
            Invoke-PolicyTraining `
                -PolicyName $style `
                -TrajectoryJsonl $iterTrajectoryJsonl `
                -Checkpoint ([string]$policyState.current_checkpoint) `
                -CheckpointDir $iterCheckpointDir
            Write-Host ("  PPO training finished: policy={0}" -f $style)

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
                $candidateManifest = Join-Path $paths.policy_dir "candidate_manifest.json"
                $candidateSelection = Join-Path $paths.policy_dir "candidate_selection.json"
                $candidateEntriesDir = Join-Path $paths.policy_dir "candidate_entries"
                if (Test-Path -LiteralPath $candidateEntriesDir) {
                    Remove-Item -LiteralPath $candidateEntriesDir -Recurse -Force
                }
                New-Item -ItemType Directory -Force -Path $candidateEntriesDir | Out-Null
                $runningEpochEvalJobs = @()
                Get-ChildItem -LiteralPath $iterCheckpointDir -Filter "epoch_*.pt" |
                    Sort-Object Name |
                    ForEach-Object {
                        $epochName = [System.IO.Path]::GetFileNameWithoutExtension($_.Name)
                        $epochNumber = [int]$epochName.Replace("epoch_", "")
                        $entryPath = Join-Path $candidateEntriesDir "$epochName.jsonl"
                        if ($EpochEvalJobs -le 1) {
                            $epochOnnx = Join-Path $paths.policy_dir "$epochName.onnx"
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
                            Write-CandidateEntry `
                                -Path $entryPath `
                                -PolicyName $style `
                                -EpochNumber $epochNumber `
                                -Checkpoint $_.FullName `
                                -Onnx $epochOnnx `
                                -Summary $epochEval.summary `
                                -Gate $epochEval.gate
                        }
                        else {
                            $runningEpochEvalJobs += Start-Job -ScriptBlock $EpochEvaluationScript -ArgumentList @(
                                $RepoRoot.Path,
                                $PythonExe,
                                $PythonVersion,
                                $CargoExe,
                                $OpponentPool,
                                $EvalMatches,
                                $Seed,
                                $MaxActionsPerMatch,
                                $ArenaJobs,
                                $style,
                                $_.FullName,
                                [string]$paths.policy_dir,
                                $iterEvalDir,
                                $BaselineOnnx,
                                $entryPath
                            )
                            while ($runningEpochEvalJobs.Count -ge $EpochEvalJobs) {
                                $completedJob = Wait-Job -Job $runningEpochEvalJobs -Any
                                $jobOutput = Receive-Job -Job $completedJob -ErrorAction Continue
                                if ($jobOutput) {
                                    Write-Host ($jobOutput | Out-String)
                                }
                                if ($completedJob.State -ne "Completed") {
                                    $jobReason = $completedJob.ChildJobs[0].JobStateInfo.Reason
                                    if ($jobReason) {
                                        Write-Host ($jobReason | Out-String)
                                    }
                                    $completedJob.ChildJobs[0].Error | ForEach-Object {
                                        Write-Host ($_ | Out-String)
                                    }
                                    throw "Epoch evaluation job failed: $($completedJob.State)"
                                }
                                Remove-Job -Job $completedJob
                                $runningEpochEvalJobs = @($runningEpochEvalJobs | Where-Object { $_.Id -ne $completedJob.Id })
                            }
                        }
                    }
                foreach ($job in $runningEpochEvalJobs) {
                    Wait-Job -Job $job | Out-Null
                    $jobOutput = Receive-Job -Job $job -ErrorAction Continue
                    if ($jobOutput) {
                        Write-Host ($jobOutput | Out-String)
                    }
                    if ($job.State -ne "Completed") {
                        $jobReason = $job.ChildJobs[0].JobStateInfo.Reason
                        if ($jobReason) {
                            Write-Host ($jobReason | Out-String)
                        }
                        $job.ChildJobs[0].Error | ForEach-Object {
                            Write-Host ($_ | Out-String)
                        }
                        throw "Epoch evaluation job failed: $($job.State)"
                    }
                    Remove-Job -Job $job
                }
                $candidateEntries = @()
                Get-ChildItem -LiteralPath $candidateEntriesDir -Filter "epoch_*.jsonl" |
                    Sort-Object Name |
                    ForEach-Object {
                        $candidateEntries += (Get-Content -LiteralPath $_.FullName -Raw | ConvertFrom-Json)
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

            $iterResult = [ordered]@{
                iteration = $iter
                policy = $style
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

                Write-Host ("  Iteration {0} policy={1}: score_margin={2} accepted={3}" -f $iter, $style, $iterResult.score_margin, $iterResult.accepted)

                if ($iterResult.accepted -or ($iterResult.score_margin -gt [double]$policyState.best_score_margin)) {
                    $policyState.best_score_margin = $iterResult.score_margin
                    $policyState.best_checkpoint = $selectedCheckpoint
                    $policyState.best_onnx = $iterCandidateOnnx
                    $policyState.best_iteration = $iter
                    $policyState.current_checkpoint = $selectedCheckpoint
                    $policyState.current_onnx = $iterCandidateOnnx
                    Write-Host ("  Policy {0} advanced (score_margin={1}, accepted={2})" -f $style, $iterResult.score_margin, $iterResult.accepted)
                }
                else {
                    Write-Host ("  Policy {0} kept current best (best_score_margin={1})" -f $style, $policyState.best_score_margin)
                }
            }

            $policyState.history = @($policyState.history) + $iterResult
        }
    }

    # Finalize: copy each policy's own best result.
    $allAccepted = @()
    $historyByPolicy = [ordered]@{}
    $finalOutputs = [ordered]@{}

    foreach ($style in $activePolicies) {
        $policyState = $policyStates[$style]
        $policyOutputDir = if ($multiPolicyTraining) { Join-Path (Join-Path $OutputDir "policies") $style } else { $OutputDir }
        $FinalCandidateOnnx = Join-Path $policyOutputDir "candidate.onnx"
        $FinalCheckpointDir = Join-Path $policyOutputDir "checkpoints"
        $FinalEvalSummary = Join-Path $policyOutputDir "candidate_eval_summary.json"
        $FinalGateOutput = Join-Path $policyOutputDir "candidate_gate.json"

        New-Item -ItemType Directory -Force -Path $FinalCheckpointDir | Out-Null
        Copy-RequiredFile -SourcePath ([string]$policyState.best_checkpoint) -TargetPath (Join-Path $FinalCheckpointDir "best.pt")
        if (-not $SkipOnnxExport) {
            Copy-RequiredFile -SourcePath ([string]$policyState.best_onnx) -TargetPath $FinalCandidateOnnx
        }

        $bestIteration = [int]$policyState.best_iteration
        $bestIterTag = if ($bestIteration -gt 0) { "iter_{0:D3}" -f $bestIteration } else { "baseline" }
        if ($bestIteration -gt 0) {
            $bestIterDir = Join-Path $OutputDir $bestIterTag
            $bestIterPaths = Get-PolicyArtifactPaths -IterationDir $bestIterDir -PolicyName $style -UsePolicySubdir $multiPolicyTraining
            $bestIterEvalDir = [string]$bestIterPaths.eval_dir
            if (Test-Path -LiteralPath "$bestIterEvalDir/candidate_eval_summary.json" -PathType Leaf) {
                Copy-RequiredFile -SourcePath "$bestIterEvalDir/candidate_eval_summary.json" -TargetPath $FinalEvalSummary
            }
            if (Test-Path -LiteralPath "$bestIterEvalDir/candidate_gate.json" -PathType Leaf) {
                Copy-RequiredFile -SourcePath "$bestIterEvalDir/candidate_gate.json" -TargetPath $FinalGateOutput
            }
        }
        elseif (@($policyState.history).Count -gt 0) {
            $lastIterTag = "iter_{0:D3}" -f @($policyState.history)[-1].iteration
            $lastIterDir = Join-Path $OutputDir $lastIterTag
            $lastIterPaths = Get-PolicyArtifactPaths -IterationDir $lastIterDir -PolicyName $style -UsePolicySubdir $multiPolicyTraining
            $lastIterEvalDir = [string]$lastIterPaths.eval_dir
            if (Test-Path -LiteralPath "$lastIterEvalDir/candidate_eval_summary.json" -PathType Leaf) {
                Copy-RequiredFile -SourcePath "$lastIterEvalDir/candidate_eval_summary.json" -TargetPath $FinalEvalSummary
            }
            if (Test-Path -LiteralPath "$lastIterEvalDir/candidate_gate.json" -PathType Leaf) {
                Copy-RequiredFile -SourcePath "$lastIterEvalDir/candidate_gate.json" -TargetPath $FinalGateOutput
            }
        }

        $historyByPolicy[$style] = @($policyState.history)
        $finalOutputs[$style] = [ordered]@{
            best_iteration = $bestIterTag
            best_score_margin = $policyState.best_score_margin
            checkpoint = Join-Path $FinalCheckpointDir "best.pt"
            candidate = $FinalCandidateOnnx
            history = if ($multiPolicyTraining) { Join-Path $policyOutputDir "iteration_history.json" } else { Join-Path $OutputDir "iteration_history.json" }
        }

        if ($multiPolicyTraining) {
            Write-Utf8NoBom -Path $finalOutputs[$style].history -Content ((@($policyState.history) | ConvertTo-Json -Depth 8) + "`n")
        }
        $acceptedForStyle = (@($policyState.history) | Where-Object { $_.accepted }) | Select-Object -First 1
        if ($null -ne $acceptedForStyle) {
            $allAccepted += $acceptedForStyle
        }
        elseif (-not $EnforceCandidateGate) {
            Write-Warning "No iteration passed the candidate gate for policy=$style. Best score_margin=$($policyState.best_score_margin). See $FinalGateOutput"
        }
    }

    $historyPath = Join-Path $OutputDir "iteration_history.json"
    if ($multiPolicyTraining) {
        $historyDocument = [ordered]@{
            trajectory_scope = "per_policy"
            policies = $historyByPolicy
        }
        Write-Utf8NoBom -Path $historyPath -Content (($historyDocument | ConvertTo-Json -Depth 10) + "`n")
    }
    else {
        Write-Utf8NoBom -Path $historyPath -Content ((@($policyStates[$activePolicies[0]].history) | ConvertTo-Json -Depth 8) + "`n")
    }

    if ($EnforceCandidateGate -and $allAccepted.Count -eq 0) {
        Write-Warning "No iteration passed the candidate gate."
        exit 1
    }

    Write-Host ""
    Write-Host "RL iterative self-play pipeline finished."
    Write-Host "Iterations:     $Iterations"
    foreach ($style in $activePolicies) {
        $output = $finalOutputs[$style]
        Write-Host ("[{0}] Best iteration: {1} (score_margin={2})" -f $style, $output.best_iteration, $output.best_score_margin)
        Write-Host ("[{0}] Checkpoint:     {1}" -f $style, $output.checkpoint)
        Write-Host ("[{0}] Candidate:      {1}" -f $style, $output.candidate)
    }
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
