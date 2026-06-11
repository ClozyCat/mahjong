param(
    [string]$DataDir = "backend/bot_trainer/v2/out",
    [string]$CheckpointDir = "backend/bot_trainer/v2/checkpoints",
    [string]$OnnxOutput = "backend/assets/sft/sft.onnx",
    [int]$Epochs = 20,
    [int]$BatchSize = 4096,
    [int]$NumWorkers = 0,
    [string]$DataCacheDir = "",
    [string]$PythonExe = "python",
    [string]$PythonVersion = "",
    [ValidateSet("auto", "cuda", "cpu", "dml")]
    [string]$Device = "cuda",
    [double]$LearningRate = 0.0003,
    [double]$WeightDecay = 0.0001,
    [double]$ClaimLossWeight = 1.0,
    [double]$SelfKongLossWeight = 1.0,
    [double]$HuLossWeight = 1.0,
    [double]$ValueLossWeight = 0.75,
    [double]$FanLossWeight = 0.5,
    [double]$QualifyingFanLossWeight = 0.75,
    [double]$RiskLossWeight = 1.0,
    [double]$RiskPosWeight = 300.0,
    [double]$ValueLossStartWeight = 0.25,
    [double]$FanLossStartWeight = 0.1,
    [double]$QualifyingFanLossStartWeight = 0.1,
    [double]$RiskLossStartWeight = 0.25,
    [int]$AuxLossWarmupEpochs = 4,
    [double]$ClaimRareActionWeight = 2.0,
    [double]$SelfKongRareActionWeight = 3.0,
    [double]$HuPositiveWeight = 3.0,
    [double]$GradClipNorm = 1.0,
    [int]$MaxNanTolerance = 2,
    [int]$EarlyStopPatience = 0,
    [switch]$RebuildDataCache,
    [switch]$Amp,
    [switch]$CompileModel,
    [switch]$SkipTests,
    [switch]$SkipOnnxExport
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Resolve-Path (Join-Path $ScriptDir "..\..\..")
$env:PYTHONUTF8 = "1"
$env:PYTHONIOENCODING = "utf-8"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8
$ExpectedMetadataSchemaVersion = 4

function Invoke-TrainingPython {
    param([string[]]$Arguments)
    if ($PythonExe -eq "py" -and $PythonVersion.Length -gt 0) {
        & $PythonExe "-$PythonVersion" @Arguments
    }
    else {
        & $PythonExe @Arguments
    }
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

function Assert-NvidiaCudaGpu {
    $nvidiaSmi = Get-Command nvidia-smi -ErrorAction SilentlyContinue
    if (-not $nvidiaSmi) {
        [Console]::Error.WriteLine("CUDA GPU is required, but nvidia-smi was not found.")
        exit 3
    }

    & $nvidiaSmi.Source --query-gpu=name --format=csv,noheader 1>$null
    if ($LASTEXITCODE -ne 0) {
        [Console]::Error.WriteLine("CUDA GPU is required, but nvidia-smi could not detect an NVIDIA GPU.")
        exit 3
    }
}

function Assert-PythonCuda {

    $deviceProbe = @'
import sys

try:
    import torch
except ModuleNotFoundError as exc:
    print('PyTorch is required: pip install torch', file=sys.stderr)
    raise SystemExit(2) from exc

if getattr(torch.version, 'hip', None):
    print('CUDA GPU is required, but this PyTorch build is ROCm/HIP.', file=sys.stderr)
    raise SystemExit(3)

if not getattr(torch.version, 'cuda', None):
    print('CUDA GPU is required, but this PyTorch build has no CUDA runtime.', file=sys.stderr)
    raise SystemExit(3)

if not torch.cuda.is_available():
    print('CUDA GPU is required, but torch.cuda.is_available() is False.', file=sys.stderr)
    raise SystemExit(3)

print('CUDA_DEVICE=' + torch.cuda.get_device_name(0))
'@

    $oldErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $probeOutput = Invoke-TrainingPython @("-c", $deviceProbe) 2>&1
    }
    finally {
        $ErrorActionPreference = $oldErrorActionPreference
    }
    if ($LASTEXITCODE -ne 0) {
        foreach ($line in $probeOutput) {
            [Console]::Error.WriteLine($line.ToString())
        }
        exit $LASTEXITCODE
    }

    $cudaDeviceLine = @($probeOutput | ForEach-Object { $_.ToString() } | Where-Object { $_ -like "CUDA_DEVICE=*" } | Select-Object -Last 1)
    if (-not $cudaDeviceLine) {
        [Console]::Error.WriteLine("Failed to verify CUDA device from Python probe.")
        exit 3
    }

    return ($cudaDeviceLine -replace "^CUDA_DEVICE=", "").Trim()
}

function Assert-DatasetContract {
    param(
        [string]$DatasetDir,
        [string]$CacheDir
    )

    $metadataPath = Join-Path $DatasetDir "metadata.json"
    if (-not (Test-Path -LiteralPath $metadataPath -PathType Leaf)) {
        [Console]::Error.WriteLine("Dataset metadata not found: $metadataPath")
        [Console]::Error.WriteLine("Run: .\backend\bot_trainer\v2\export_full_dataset.ps1 -OutputDir $DatasetDir")
        exit 2
    }

    try {
        $metadata = Get-Content -LiteralPath $metadataPath -Raw | ConvertFrom-Json
    }
    catch {
        [Console]::Error.WriteLine("Dataset metadata is not valid JSON: $metadataPath")
        [Console]::Error.WriteLine($_.Exception.Message)
        exit 2
    }

    $schemaVersion = $metadata.schema_version
    if ($schemaVersion -ne $ExpectedMetadataSchemaVersion) {
        [Console]::Error.WriteLine(
            "Unsupported dataset schema: $schemaVersion; expected $ExpectedMetadataSchemaVersion."
        )
        [Console]::Error.WriteLine(
            "Re-export data before training: .\backend\bot_trainer\v2\export_full_dataset.ps1 -OutputDir $DatasetDir"
        )
        [Console]::Error.WriteLine(
            "Then rebuild/remove the tensor cache: use -RebuildDataCache or delete $CacheDir"
        )
        exit 2
    }

    foreach ($name in @("train.jsonl", "val.jsonl", "test.jsonl")) {
        $splitPath = Join-Path $DatasetDir $name
        if (-not (Test-Path -LiteralPath $splitPath -PathType Leaf)) {
            [Console]::Error.WriteLine("Dataset split not found: $splitPath")
            [Console]::Error.WriteLine("Run: .\backend\bot_trainer\v2\export_full_dataset.ps1 -OutputDir $DatasetDir")
            exit 2
        }
    }
}

$PreviousTemp = $env:TEMP
$PreviousTmp = $env:TMP
$PreviousPytestTempRoot = $env:PYTEST_DEBUG_TEMPROOT

Push-Location $RepoRoot
try {
    $ScriptTempDir = Join-Path $RepoRoot ".tmp\bot-trainer-v2-sft"
    New-Item -ItemType Directory -Force -Path $ScriptTempDir | Out-Null
    $env:TEMP = (Resolve-Path -LiteralPath $ScriptTempDir).Path
    $env:TMP = $env:TEMP
    $env:PYTEST_DEBUG_TEMPROOT = $env:TEMP

    $CudaDeviceName = ""
    if ($Device -eq "cuda") {
        Assert-NvidiaCudaGpu
        $CudaDeviceName = Assert-PythonCuda
    }
    $ResolvedDataCacheDir = if ($DataCacheDir.Length -gt 0) { $DataCacheDir } else { Join-Path $DataDir ".tensor_cache" }

    Write-Host "Training Mahjong bot v2 model"
    Write-Host "Data:        $DataDir"
    Write-Host "Checkpoints: $CheckpointDir"
    Write-Host "Device:      $Device"
    if ($Device -eq "cuda") {
        Write-Host "CUDA GPU:    $CudaDeviceName"
    }
    Write-Host "Epochs:      $Epochs"
    Write-Host "Batch size:  $BatchSize"
    Write-Host "Workers:     $NumWorkers"
    Write-Host "Data cache:  $ResolvedDataCacheDir"
    Write-Host "Aux weights: value=$ValueLossWeight fan=$FanLossWeight qualifying_fan=$QualifyingFanLossWeight risk=$RiskLossWeight risk_pos=$RiskPosWeight"
    Write-Host "Rare weights: claim=$ClaimRareActionWeight self_kong=$SelfKongRareActionWeight hu=$HuPositiveWeight"
    Write-Host "Grad clip:   $GradClipNorm"
    Write-Host "NaN tolerance: $MaxNanTolerance"
    Write-Host "Early stop:  $EarlyStopPatience"
    Write-Host "Python:      $PythonExe $PythonVersion"

    Assert-DatasetContract -DatasetDir $DataDir -CacheDir $ResolvedDataCacheDir

    if (-not $SkipTests) {
        Invoke-TrainingPython @("-c", "import importlib.util, sys; sys.exit(0 if importlib.util.find_spec('pytest') else 2)")
        if ($LASTEXITCODE -eq 0) {
            Invoke-TrainingPython @(
                "-m", "pytest",
                "backend/bot_trainer/v2",
                "-q",
                "--basetemp", (Join-Path $ScriptTempDir "pytest")
            )
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        }
        else {
            Write-Host "pytest is not installed for this Python; skipping Python tests. Use -SkipTests to silence this check."
        }
    }

    $trainArgs = @(
        "backend/bot_trainer/v2/train.py",
        "--data", $DataDir,
        "--epochs", "$Epochs",
        "--batch-size", "$BatchSize",
        "--output", $CheckpointDir,
        "--device", $Device,
        "--num-workers", "$NumWorkers",
        "--data-cache-dir", $ResolvedDataCacheDir,
        "--lr", "$LearningRate",
        "--weight-decay", "$WeightDecay",
        "--claim-loss-weight", "$ClaimLossWeight",
        "--self-kong-loss-weight", "$SelfKongLossWeight",
        "--hu-loss-weight", "$HuLossWeight",
        "--value-loss-weight", "$ValueLossWeight",
        "--fan-loss-weight", "$FanLossWeight",
        "--qualifying-fan-loss-weight", "$QualifyingFanLossWeight",
        "--risk-loss-weight", "$RiskLossWeight",
        "--risk-pos-weight", "$RiskPosWeight",
        "--value-loss-start-weight", "$ValueLossStartWeight",
        "--fan-loss-start-weight", "$FanLossStartWeight",
        "--qualifying-fan-loss-start-weight", "$QualifyingFanLossStartWeight",
        "--risk-loss-start-weight", "$RiskLossStartWeight",
        "--aux-loss-warmup-epochs", "$AuxLossWarmupEpochs",
        "--claim-rare-action-weight", "$ClaimRareActionWeight",
        "--self-kong-rare-action-weight", "$SelfKongRareActionWeight",
        "--hu-positive-weight", "$HuPositiveWeight",
        "--grad-clip-norm", "$GradClipNorm",
        "--max-nan-tolerance", "$MaxNanTolerance",
        "--early-stop-patience", "$EarlyStopPatience"
    )
    if ($Amp) {
        $trainArgs += "--amp"
    }
    if ($CompileModel) {
        $trainArgs += "--compile"
    }
    if ($RebuildDataCache) {
        $trainArgs += "--rebuild-data-cache"
    }
    Invoke-TrainingPython $trainArgs
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    if (-not $SkipOnnxExport) {
        Invoke-TrainingPython @(
            "backend/bot_trainer/v2/export_onnx.py",
            "--checkpoint", (Join-Path $CheckpointDir "best.pt"),
            "--output", $OnnxOutput
        )
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

        cargo test --manifest-path backend/Cargo.toml bot::neural::tests::runs_local_onnx_model_when_available -- --nocapture
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
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
