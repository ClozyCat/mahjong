param(
    [string]$BuildLabel = (Get-Date -Format "yyyyMMdd-HHmmss"),
    [string]$OutputRoot = "output/windows",
    [string]$Configuration = "release",
    [switch]$SkipNpmCi
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Invoke-CheckedCommand {
    param(
        [Parameter(Mandatory = $true)]
        [string]$FilePath,
        [Parameter(ValueFromRemainingArguments = $true)]
        [string[]]$Arguments,
        [string]$WorkingDirectory = (Get-Location).Path
    )

    Push-Location $WorkingDirectory
    try {
        & $FilePath @Arguments
        if ($LASTEXITCODE -ne 0) {
            throw "$FilePath $($Arguments -join ' ') failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
}

function Get-CSharpCompilerPath {
    $candidates = @(
        "C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe",
        "C:\Windows\Microsoft.NET\Framework\v4.0.30319\csc.exe"
    )

    foreach ($candidate in $candidates) {
        if (Test-Path $candidate) {
            return $candidate
        }
    }

    throw "Unable to locate csc.exe from .NET Framework."
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$frontendDir = Join-Path $repoRoot "frontend"
$backendDir = Join-Path $repoRoot "backend"
$launcherSource = Join-Path $repoRoot "scripts\windows-launcher\MahjongLauncher.cs"
$outputDir = Join-Path $repoRoot $OutputRoot
$buildDir = Join-Path $outputDir $BuildLabel
$payloadDir = Join-Path $buildDir "payload"
$payloadZip = Join-Path $buildDir "payload.zip"
$launcherExe = Join-Path $buildDir "Mahjong.exe"
$readmePath = Join-Path $buildDir "README.txt"
$cscPath = Get-CSharpCompilerPath

if (-not (Get-Command npm -ErrorAction SilentlyContinue)) {
    throw "npm not found. Please install Node.js first."
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "cargo not found. Please install Rust first."
}

if (-not (Test-Path $launcherSource)) {
    throw "Launcher source not found at $launcherSource"
}

Write-Host "Preparing output directory: $buildDir"
if (Test-Path $buildDir) {
    Remove-Item -LiteralPath $buildDir -Recurse -Force
}
New-Item -ItemType Directory -Path $payloadDir | Out-Null

if (-not $SkipNpmCi) {
    Write-Host "Installing frontend dependencies"
    Invoke-CheckedCommand -FilePath "npm" -Arguments "ci" -WorkingDirectory $frontendDir
}

Write-Host "Building frontend"
Invoke-CheckedCommand -FilePath "npm" -Arguments "run", "build" -WorkingDirectory $frontendDir

Write-Host "Building backend"
Invoke-CheckedCommand -FilePath "cargo" -Arguments "build", "--$Configuration" -WorkingDirectory $backendDir

$backendExe = Join-Path $backendDir "target\$Configuration\backend.exe"
$frontendDist = Join-Path $frontendDir "dist"
if (-not (Test-Path $backendExe)) {
    throw "Backend executable not found at $backendExe"
}
if (-not (Test-Path (Join-Path $frontendDist "index.html"))) {
    throw "Frontend dist output not found at $frontendDist"
}

Write-Host "Staging payload files"
$payloadBackendDir = Join-Path $payloadDir "backend"
$payloadWebDir = Join-Path $payloadDir "web"
New-Item -ItemType Directory -Path $payloadBackendDir | Out-Null
New-Item -ItemType Directory -Path $payloadWebDir | Out-Null
Copy-Item -LiteralPath $backendExe -Destination (Join-Path $payloadBackendDir "mahjong-backend.exe") -Force
Copy-Item -Path (Join-Path $frontendDist "*") -Destination $payloadWebDir -Recurse -Force

Write-Host "Creating embedded payload archive"
Compress-Archive -Path (Join-Path $payloadDir "*") -DestinationPath $payloadZip -CompressionLevel Optimal

Write-Host "Compiling launcher executable"
$compilerArguments = @(
    "/nologo",
    "/target:winexe",
    "/optimize+",
    "/out:$launcherExe",
    "/resource:$payloadZip,MahjongLauncher.Payload.zip",
    "/r:System.dll",
    "/r:System.Core.dll",
    "/r:System.IO.Compression.dll",
    "/r:System.IO.Compression.FileSystem.dll",
    "/r:System.Windows.Forms.dll",
    $launcherSource
)
Invoke-CheckedCommand -FilePath $cscPath -Arguments $compilerArguments -WorkingDirectory $repoRoot

$instructions = @"
Mahjong Windows package is ready:
  $launcherExe

How it works:
  1. Double-click Mahjong.exe
  2. The launcher extracts the bundled frontend and backend to %LOCALAPPDATA%\Mahjong
  3. It starts the local server and opens the browser automatically

User data:
  %LOCALAPPDATA%\Mahjong\data\mahjong.db
"@

Set-Content -Path $readmePath -Value $instructions

Write-Host ""
Write-Host $instructions
