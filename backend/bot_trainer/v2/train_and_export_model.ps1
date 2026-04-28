param(
    [string]$DataDir = "backend/bot_trainer/v2/out",
    [string]$CheckpointDir = "backend/bot_trainer/v2/checkpoints",
    [string]$OnnxOutput = "backend/assets/models/mahjong_policy_net.onnx",
    [int]$Epochs = 20,
    [int]$BatchSize = 4096,
    [string]$Device = "auto",  # [主要改动] 从 "cuda" 改为 "auto"，适配 AMD 显卡
    [int]$NumWorkers = 0,      # 保持为 0，这在 Windows 的 DirectML 环境下是最稳定、不卡死的选择
    [string]$PythonExe = "python",
    [string]$PythonVersion = "",
    [double]$LearningRate = 0.001,
    [double]$WeightDecay = 0.0001,
    [switch]$NoAmp,            # 虽然我们脚本里保留了 AMP 推送，但之前 Python 脚本里已经做了智能拦截，DirectML 下会自动屏蔽
    [switch]$CompileModel,
    [switch]$SkipTests,
    [switch]$SkipOnnxExport
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Resolve-Path (Join-Path $ScriptDir "..\..\..")

function Invoke-TrainingPython {
    param([string[]]$Arguments)
    if ($PythonExe -eq "py" -and $PythonVersion.Length -gt 0) {
        & $PythonExe "-$PythonVersion" @Arguments
    }
    else {
        & $PythonExe @Arguments
    }
}

Push-Location $RepoRoot
try {
    Write-Host "Training Mahjong bot v2 model"
    Write-Host "Data:        $DataDir"
    Write-Host "Checkpoints: $CheckpointDir"
    Write-Host "Device:      $Device"
    Write-Host "Epochs:      $Epochs"
    Write-Host "Batch size:  $BatchSize"
    Write-Host "Workers:     $NumWorkers"
    Write-Host "Python:      $PythonExe $PythonVersion"

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
        "--device", $Device,
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