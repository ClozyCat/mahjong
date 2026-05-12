param(
    [string]$OutputDir = "backend/bot_trainer/v2/rl_runs/$(Get-Date -Format 'yyyyMMddHHmm')",
    [string]$BaselineCheckpoint = "backend/bot_trainer/v2/checkpoints/best.pt",
    [string]$BaselineOnnx = "backend/assets/models/mahjong_policy_net.onnx",
    [string]$PythonExe = "python",
    [string]$PythonVersion = "",
    [string]$CargoExe = "cargo",
    [int]$ArenaJobs = 0,
    [int]$Iterations = 5,
    [int]$IterationMatches = 1500,
    [int]$TrajectoryProgressEvery = 500,
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
    [ValidateSet("aggressive", "balanced", "defensive")]
    [string]$PlayStyle = "balanced",
    [string[]]$PlayStyles = @(),
    [ValidateSet("", "aggressive", "balanced", "defensive")]
    [string]$TrajectoryRolloutStyle = "",
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
    [switch]$RecordHeuristicComparison,
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
    if ($RecordHeuristicComparison) {
        $evalConfigArgs += @("--record-heuristic-comparison")
    }
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

function Resolve-ActivePlayStyles {
    $sourceStyles = if ($PlayStyles.Count -gt 0) { $PlayStyles } else { @($PlayStyle) }
    $activeStyles = @()
    $validStyles = @("aggressive", "balanced", "defensive")
    foreach ($styleValue in $sourceStyles) {
        foreach ($style in ([string]$styleValue -split ",")) {
            $normalizedStyle = $style.Trim()
            if ([string]::IsNullOrWhiteSpace($normalizedStyle)) {
                continue
            }
            if ($validStyles -notcontains $normalizedStyle) {
                throw "Invalid play style '$normalizedStyle'. Expected one of: $($validStyles -join ', ')"
            }
            if ($activeStyles -notcontains $normalizedStyle) {
                $activeStyles += $normalizedStyle
            }
        }
    }
    return $activeStyles
}

function Get-StyleArtifactPaths {
    param(
        [string]$IterationDir,
        [string]$Style,
        [bool]$UseStyleSubdir
    )

    $styleDir = if ($UseStyleSubdir) { Join-Path (Join-Path $IterationDir "styles") $Style } else { $IterationDir }
    return [ordered]@{
        style_dir = $styleDir
        checkpoint_dir = Join-Path $styleDir "checkpoints"
        candidate_onnx = Join-Path $styleDir "candidate.onnx"
        eval_dir = Join-Path $styleDir "eval"
    }
}

function Invoke-PlayStyleTraining {
    param(
        [string]$Style,
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
        "--play-style", $Style,
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
    $activePlayStyles = Resolve-ActivePlayStyles
    $multiStyleTraining = $activePlayStyles.Count -gt 1
    if (-not [string]::IsNullOrWhiteSpace($TrajectoryRolloutStyle)) {
        Write-Warning "-TrajectoryRolloutStyle is ignored because each play_style now generates its own trajectories."
    }

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
    Write-Host "Play styles:         $($activePlayStyles -join ', ')"
    Write-Host "Python:              $PythonExe $PythonVersion"
    Write-Host "Cargo:               $CargoExe"
    $arenaJobsLabel = if ($ArenaJobs -eq 0) { "auto" } else { $ArenaJobs }
    Write-Host ("Arena jobs:          {0}" -f $arenaJobsLabel)
    Write-Host ("Heuristic compare:   {0}" -f ([bool]$RecordHeuristicComparison))

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
    $styleStates = @{}
    foreach ($style in $activePlayStyles) {
        $styleStates[$style] = [ordered]@{
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

        # Step 1/2/3/4: Generate trajectories, train, export, and evaluate each style serially.
        foreach ($style in $activePlayStyles) {
            $paths = Get-StyleArtifactPaths -IterationDir $iterDir -Style $style -UseStyleSubdir $multiStyleTraining
            $iterTrajectoryConfigDir = Join-Path $paths.style_dir "trajectory_configs"
            $iterTrajectoryJsonl = Join-Path $paths.style_dir "trajectories.jsonl"
            New-Item -ItemType Directory -Force -Path $paths.checkpoint_dir | Out-Null
            $iterCheckpointDir = [string]$paths.checkpoint_dir
            $iterCandidateOnnx = [string]$paths.candidate_onnx
            $iterEvalDir = [string]$paths.eval_dir
            $styleState = $styleStates[$style]
            $rolloutOnnx = [string]$styleState.current_onnx
            $currentOnnxLeaf = Split-Path -Leaf $rolloutOnnx

            Write-Host ("  Generating trajectories: style={0} rollout={1}" -f $style, $currentOnnxLeaf)
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
            if ($RecordHeuristicComparison) {
                $trajectoryConfigArgs += @("--record-heuristic-comparison")
            }
            Invoke-TrainingPython $trajectoryConfigArgs
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

            $trajectoryFiles = @()
            Get-ChildItem -LiteralPath $iterTrajectoryConfigDir -Filter "trajectory_config_*.json" |
                Sort-Object Name |
                ForEach-Object {
                    $index = [System.IO.Path]::GetFileNameWithoutExtension($_.Name).Replace("trajectory_config_", "")
                    $partialReport = Join-Path $paths.style_dir "trajectory_arena_report_$index.jsonl"
                    $partialTrajectory = Join-Path $paths.style_dir "trajectories_$index.jsonl"
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

            Write-Host ("  Starting PPO training: style={0}" -f $style)
            Invoke-PlayStyleTraining `
                -Style $style `
                -TrajectoryJsonl $iterTrajectoryJsonl `
                -Checkpoint ([string]$styleState.current_checkpoint) `
                -CheckpointDir $iterCheckpointDir
            Write-Host ("  PPO training finished: style={0}" -f $style)

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
                $candidateManifest = Join-Path $paths.style_dir "candidate_manifest.json"
                $candidateSelection = Join-Path $paths.style_dir "candidate_selection.json"
                $candidateEntries = @()
                Get-ChildItem -LiteralPath $iterCheckpointDir -Filter "epoch_*.pt" |
                    Sort-Object Name |
                    ForEach-Object {
                        $epochName = [System.IO.Path]::GetFileNameWithoutExtension($_.Name)
                        $epochOnnx = Join-Path $paths.style_dir "$epochName.onnx"
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
                            play_style = $style
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

            $iterResult = [ordered]@{
                iteration = $iter
                play_style = $style
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

                Write-Host ("  Iteration {0} style={1}: score_margin={2} accepted={3}" -f $iter, $style, $iterResult.score_margin, $iterResult.accepted)

                if ($iterResult.accepted -or ($iterResult.score_margin -gt [double]$styleState.best_score_margin)) {
                    $styleState.best_score_margin = $iterResult.score_margin
                    $styleState.best_checkpoint = $selectedCheckpoint
                    $styleState.best_onnx = $iterCandidateOnnx
                    $styleState.best_iteration = $iter
                    $styleState.current_checkpoint = $selectedCheckpoint
                    $styleState.current_onnx = $iterCandidateOnnx
                    Write-Host ("  Style {0} advanced (score_margin={1}, accepted={2})" -f $style, $iterResult.score_margin, $iterResult.accepted)
                }
                else {
                    Write-Host ("  Style {0} kept current best (best_score_margin={1})" -f $style, $styleState.best_score_margin)
                }
            }

            $styleState.history = @($styleState.history) + $iterResult
        }
    }

    # Finalize: copy each play style's own best result.
    $allAccepted = @()
    $historyByStyle = [ordered]@{}
    $finalOutputs = [ordered]@{}

    foreach ($style in $activePlayStyles) {
        $styleState = $styleStates[$style]
        $styleOutputDir = if ($multiStyleTraining) { Join-Path (Join-Path $OutputDir "styles") $style } else { $OutputDir }
        $FinalCandidateOnnx = Join-Path $styleOutputDir "candidate.onnx"
        $FinalCheckpointDir = Join-Path $styleOutputDir "checkpoints"
        $FinalEvalSummary = Join-Path $styleOutputDir "candidate_eval_summary.json"
        $FinalGateOutput = Join-Path $styleOutputDir "candidate_gate.json"

        New-Item -ItemType Directory -Force -Path $FinalCheckpointDir | Out-Null
        Copy-RequiredFile -SourcePath ([string]$styleState.best_checkpoint) -TargetPath (Join-Path $FinalCheckpointDir "best.pt")
        if (-not $SkipOnnxExport) {
            Copy-RequiredFile -SourcePath ([string]$styleState.best_onnx) -TargetPath $FinalCandidateOnnx
        }

        $bestIteration = [int]$styleState.best_iteration
        $bestIterTag = if ($bestIteration -gt 0) { "iter_{0:D3}" -f $bestIteration } else { "baseline" }
        if ($bestIteration -gt 0) {
            $bestIterDir = Join-Path $OutputDir $bestIterTag
            $bestIterPaths = Get-StyleArtifactPaths -IterationDir $bestIterDir -Style $style -UseStyleSubdir $multiStyleTraining
            $bestIterEvalDir = [string]$bestIterPaths.eval_dir
            if (Test-Path -LiteralPath "$bestIterEvalDir/candidate_eval_summary.json" -PathType Leaf) {
                Copy-RequiredFile -SourcePath "$bestIterEvalDir/candidate_eval_summary.json" -TargetPath $FinalEvalSummary
            }
            if (Test-Path -LiteralPath "$bestIterEvalDir/candidate_gate.json" -PathType Leaf) {
                Copy-RequiredFile -SourcePath "$bestIterEvalDir/candidate_gate.json" -TargetPath $FinalGateOutput
            }
        }
        elseif (@($styleState.history).Count -gt 0) {
            $lastIterTag = "iter_{0:D3}" -f @($styleState.history)[-1].iteration
            $lastIterDir = Join-Path $OutputDir $lastIterTag
            $lastIterPaths = Get-StyleArtifactPaths -IterationDir $lastIterDir -Style $style -UseStyleSubdir $multiStyleTraining
            $lastIterEvalDir = [string]$lastIterPaths.eval_dir
            if (Test-Path -LiteralPath "$lastIterEvalDir/candidate_eval_summary.json" -PathType Leaf) {
                Copy-RequiredFile -SourcePath "$lastIterEvalDir/candidate_eval_summary.json" -TargetPath $FinalEvalSummary
            }
            if (Test-Path -LiteralPath "$lastIterEvalDir/candidate_gate.json" -PathType Leaf) {
                Copy-RequiredFile -SourcePath "$lastIterEvalDir/candidate_gate.json" -TargetPath $FinalGateOutput
            }
        }

        $historyByStyle[$style] = @($styleState.history)
        $finalOutputs[$style] = [ordered]@{
            best_iteration = $bestIterTag
            best_score_margin = $styleState.best_score_margin
            checkpoint = Join-Path $FinalCheckpointDir "best.pt"
            candidate = $FinalCandidateOnnx
            history = if ($multiStyleTraining) { Join-Path $styleOutputDir "iteration_history.json" } else { Join-Path $OutputDir "iteration_history.json" }
        }

        if ($multiStyleTraining) {
            Write-Utf8NoBom -Path $finalOutputs[$style].history -Content ((@($styleState.history) | ConvertTo-Json -Depth 8) + "`n")
        }
        $acceptedForStyle = (@($styleState.history) | Where-Object { $_.accepted }) | Select-Object -First 1
        if ($null -ne $acceptedForStyle) {
            $allAccepted += $acceptedForStyle
        }
        elseif (-not $EnforceCandidateGate) {
            Write-Warning "No iteration passed the candidate gate for play_style=$style. Best score_margin=$($styleState.best_score_margin). See $FinalGateOutput"
        }
    }

    $historyPath = Join-Path $OutputDir "iteration_history.json"
    if ($multiStyleTraining) {
        $historyDocument = [ordered]@{
            trajectory_scope = "per_play_style"
            styles = $historyByStyle
        }
        Write-Utf8NoBom -Path $historyPath -Content (($historyDocument | ConvertTo-Json -Depth 10) + "`n")
    }
    else {
        Write-Utf8NoBom -Path $historyPath -Content ((@($styleStates[$activePlayStyles[0]].history) | ConvertTo-Json -Depth 8) + "`n")
    }

    if ($EnforceCandidateGate -and $allAccepted.Count -eq 0) {
        Write-Warning "No iteration passed the candidate gate."
        exit 1
    }

    Write-Host ""
    Write-Host "RL iterative self-play pipeline finished."
    Write-Host "Iterations:     $Iterations"
    foreach ($style in $activePlayStyles) {
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
