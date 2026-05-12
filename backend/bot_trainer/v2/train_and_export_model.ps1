param(
    [string]$DataDir = "backend/bot_trainer/v2/out",
    [string]$CheckpointDir = "backend/bot_trainer/v2/checkpoints",
    [string]$OnnxOutput = "backend/assets/backup_model/backup.onnx",
    [int]$Epochs = 20,
    [int]$BatchSize = 4096,
    [int]$NumWorkers = 0,
    [string]$DataCacheDir = "",
    [string]$PythonExe = "python",
    [string]$PythonVersion = "",
    [ValidateSet("auto", "cuda", "cpu", "dml")]
    [string]$Device = "cuda",
    [double]$LearningRate = 0.001,
    [double]$WeightDecay = 0.0001,
    [double]$ClaimLossWeight = 1.0,
    [double]$SelfKongLossWeight = 1.0,
    [double]$HuLossWeight = 1.0,
    [double]$ValueLossWeight = 0.25,
    [double]$RiskLossWeight = 0.25,
    [double]$FanLossWeight = 0.25,
    [switch]$RebuildDataCache,
    [switch]$NoAmp,
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
    Write-Host "Python:      $PythonExe $PythonVersion"

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
        "--risk-loss-weight", "$RiskLossWeight",
        "--fan-loss-weight", "$FanLossWeight"
    )
    if (-not $NoAmp) {
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
