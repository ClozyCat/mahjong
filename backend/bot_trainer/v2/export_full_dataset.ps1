param(
    [string]$InputPath = "backend/bot_trainer/datasets/data.txt",
    [string]$OutputDir = "backend/bot_trainer/v2/out",
    [int]$ProgressEvery = 1000,
    [int]$MaxMatches = 0
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Resolve-Path (Join-Path $ScriptDir "..\..\..")

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

Push-Location $RepoRoot
try {
    Assert-NvidiaCudaGpu

    $arguments = @(
        "run",
        "--release",
        "--manifest-path",
        "backend/Cargo.toml",
        "--bin",
        "export_bot_dataset_v2",
        "--",
        "--input",
        $InputPath,
        "--output",
        $OutputDir,
        "--progress-every",
        "$ProgressEvery"
    )

    if ($MaxMatches -gt 0) {
        $arguments += @("--max-matches", "$MaxMatches")
    }

    Write-Host "Running exporter from $RepoRoot"
    Write-Host "Input:  $InputPath"
    Write-Host "Output: $OutputDir"
    Write-Host "Progress interval: every $ProgressEvery matches"
    & cargo @arguments
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
