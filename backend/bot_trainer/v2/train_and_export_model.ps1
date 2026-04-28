param(
    [string]$DataDir = "backend/bot_trainer/v2/out",
    [string]$CheckpointDir = "backend/bot_trainer/v2/checkpoints",
    [string]$OnnxOutput = "backend/assets/models/mahjong_policy_net.onnx",
    [int]$Epochs = 20,
    [int]$BatchSize = 4096,
    [string]$Device = "rocm",
    [int]$NumWorkers = 0,
    [string]$PythonExe = "python",
    [string]$PythonVersion = "",
    [double]$LearningRate = 0.001,
    [double]$WeightDecay = 0.0001,
    [string]$RocmGfxOverride = "10.3.0",
    [switch]$NoRocmGfxOverride,
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
$env:PYTHONFAULTHANDLER = "1"
if (-not $env:AMD_LOG_LEVEL) {
    $env:AMD_LOG_LEVEL = "0"
}
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

function Resolve-TrainingDevice {
    param([string]$Requested)

    $deviceProbe = @'
import sys

try:
    import torch
except ModuleNotFoundError as exc:
    print('PyTorch is required: pip install torch', file=sys.stderr)
    raise SystemExit(2) from exc

requested = sys.argv[1].strip().lower()

def fail(message: str, code: int = 3) -> None:
    print(message, file=sys.stderr)
    raise SystemExit(code)

def cuda_available() -> bool:
    return bool(torch.cuda.is_available())

def rocm_available() -> bool:
    return cuda_available() and bool(getattr(torch.version, 'hip', None))

def nvidia_cuda_available() -> bool:
    return cuda_available() and bool(getattr(torch.version, 'cuda', None)) and not bool(getattr(torch.version, 'hip', None))

def rocm_failure_message() -> str:
    return (
        'Requested ROCm/HIP, but this Python environment does not expose a ROCm PyTorch backend. '
        'If torch.cuda.is_available() crashes, the failure is in ROCm/HIP runtime initialization. '
        'HIP SDK alone is not enough; use a ROCm-enabled PyTorch build and a supported GPU/OS combination.'
    )

def resolve_backend_and_device() -> tuple[str, str]:
    if requested in ('auto', 'gpu'):
        if rocm_available():
            return 'rocm', 'cuda'
        if nvidia_cuda_available():
            return 'cuda', 'cuda'
        fail(
            'No supported GPU backend is available. CPU fallback is disabled. '
            'For AMD ROCm, install a ROCm-enabled PyTorch build; for NVIDIA CUDA, install a CUDA-enabled PyTorch build.'
        )

    if requested in ('rocm', 'hip', 'amd'):
        if rocm_available():
            return 'rocm', 'cuda'
        fail(rocm_failure_message())

    if requested in ('cuda', 'cu', 'nvidia'):
        if nvidia_cuda_available():
            return 'cuda', 'cuda'
        if rocm_available():
            fail('Requested NVIDIA CUDA, but this PyTorch build is ROCm/HIP. Use -Device rocm or -Device auto.')
        fail('Requested NVIDIA CUDA, but torch.cuda.is_available() is False or torch.version.cuda is empty.')

    if requested in ('dml', 'directml'):
        fail('DirectML is disabled for this script. Use -Device rocm with a ROCm-enabled PyTorch build.')

    if requested == 'cpu':
        fail('CPU training is disabled for this script. Install a supported GPU backend instead.')

    try:
        device = torch.device(requested)
    except Exception as exc:
        fail('Unsupported device ' + repr(requested) + ': ' + str(exc), 2)

    if device.type == 'cpu':
        fail('Training device resolved to CPU. CPU fallback is disabled.')
    if device.type == 'cuda':
        if rocm_available():
            return 'rocm', 'cuda'
        if nvidia_cuda_available():
            return 'cuda', 'cuda'
        fail('Requested cuda device, but no CUDA/ROCm backend is available.')
    return device.type, requested

backend, device = resolve_backend_and_device()
print('RESOLVED_BACKEND=' + backend)
print('RESOLVED_DEVICE=' + device)
'@

    $oldErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $probeOutput = Invoke-TrainingPython @("-c", $deviceProbe, $Requested) 2>&1
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

    $resolvedBackendLine = @($probeOutput | ForEach-Object { $_.ToString() } | Where-Object { $_ -like "RESOLVED_BACKEND=*" } | Select-Object -Last 1)
    $resolvedDeviceLine = @($probeOutput | ForEach-Object { $_.ToString() } | Where-Object { $_ -like "RESOLVED_DEVICE=*" } | Select-Object -Last 1)
    if (-not $resolvedBackendLine -or -not $resolvedDeviceLine) {
        [Console]::Error.WriteLine("Failed to resolve training device from Python probe.")
        exit 3
    }

    return @{
        Backend = ($resolvedBackendLine -replace "^RESOLVED_BACKEND=", "").Trim()
        Device = ($resolvedDeviceLine -replace "^RESOLVED_DEVICE=", "").Trim()
    }
}

Push-Location $RepoRoot
try {
    $requestedDevice = $Device.Trim().ToLowerInvariant()
    $shouldSetRocmOverride = -not $NoRocmGfxOverride -and $RocmGfxOverride.Length -gt 0 -and @("auto", "gpu", "rocm", "hip", "amd") -contains $requestedDevice
    if ($shouldSetRocmOverride -and -not $env:HSA_OVERRIDE_GFX_VERSION) {
        $env:HSA_OVERRIDE_GFX_VERSION = $RocmGfxOverride
    }

    $Resolved = Resolve-TrainingDevice $Device
    $ResolvedBackend = $Resolved.Backend
    $ResolvedDevice = $Resolved.Device

    Write-Host "Training Mahjong bot v2 model"
    Write-Host "Data:        $DataDir"
    Write-Host "Checkpoints: $CheckpointDir"
    Write-Host "Backend:     $ResolvedBackend"
    Write-Host "Device:      $ResolvedDevice (requested: $Device)"
    Write-Host "Epochs:      $Epochs"
    Write-Host "Batch size:  $BatchSize"
    Write-Host "Workers:     $NumWorkers"
    Write-Host "Python:      $PythonExe $PythonVersion"
    if ($ResolvedBackend -eq "rocm") {
        Write-Host "ROCm GFX:    $env:HSA_OVERRIDE_GFX_VERSION"
    }

    if (-not $SkipTests) {
        Invoke-TrainingPython @("-c", "import importlib.util, sys; sys.exit(0 if importlib.util.find_spec('pytest') else 2)")
        if ($LASTEXITCODE -eq 0) {
            Invoke-TrainingPython @("-m", "pytest", "backend/bot_trainer/v2", "-q")
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
        "--device", $ResolvedDevice,
        "--num-workers", "$NumWorkers",
        "--lr", "$LearningRate",
        "--weight-decay", "$WeightDecay"
    )
    if (-not $NoAmp) {
        $trainArgs += "--amp"
    }
    if ($CompileModel) {
        $trainArgs += "--compile"
    }
    Invoke-TrainingPython $trainArgs
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    if (-not $SkipOnnxExport) {
        # 注意：通常 ONNX 导出 (export_onnx.py) 跑在 CPU 上就行，所以这里没有显式传 Device 参数是安全的
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
}
