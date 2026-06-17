param(
    [string]$InputPath = "backend/bot_trainer/datasets/data.txt",
    [string]$OutputDir = "backend/bot_trainer/v2/out",
    [int]$ProgressEvery = 10000,
    [int]$MaxMatches = 0
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Resolve-Path (Join-Path $ScriptDir "..\..\..")

Push-Location $RepoRoot
try {
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
