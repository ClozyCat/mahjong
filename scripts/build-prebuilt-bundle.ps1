param(
    [string]$Tag = "latest",
    [string]$Platform = "linux/amd64",
    [string]$OutputRoot = "output/deploy"
)

$ErrorActionPreference = "Stop"

if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
    throw "Docker CLI not found. Please install Docker Desktop or Docker Engine first."
}

function Invoke-Docker {
    param(
        [Parameter(ValueFromRemainingArguments = $true)]
        [string[]]$Args
    )

    & docker @Args

    if ($LASTEXITCODE -ne 0) {
        throw "docker $($Args -join ' ') failed with exit code $LASTEXITCODE"
    }
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$bundleDir = Join-Path $repoRoot $OutputRoot
$bundleDir = Join-Path $bundleDir $Tag
$archivePath = Join-Path $bundleDir "mahjong-images.tar"
$backendImage = "mahjong-backend:$Tag"
$frontendImage = "mahjong-frontend:$Tag"

New-Item -ItemType Directory -Force -Path $bundleDir | Out-Null

Write-Host "Checking Docker daemon"
Invoke-Docker version --format "{{.Server.Os}}/{{.Server.Arch}}"

Write-Host "Checking Docker Buildx"
Invoke-Docker buildx version

Write-Host "Building backend image: $backendImage ($Platform)"
Invoke-Docker buildx build `
    --platform $Platform `
    --target backend-runtime `
    --tag $backendImage `
    --load `
    $repoRoot

Write-Host "Building frontend image: $frontendImage ($Platform)"
Invoke-Docker buildx build `
    --platform $Platform `
    --target frontend-runtime `
    --tag $frontendImage `
    --load `
    $repoRoot

Write-Host "Saving images to $archivePath"
Invoke-Docker save `
    --output $archivePath `
    $backendImage `
    $frontendImage

Copy-Item (Join-Path $repoRoot "docker-compose.prebuilt.yml") (Join-Path $bundleDir "docker-compose.yml") -Force
$bundleEnvExamplePath = Join-Path $bundleDir ".env.example"
Copy-Item (Join-Path $repoRoot ".env.example") $bundleEnvExamplePath -Force

$bundleEnvExample = Get-Content $bundleEnvExamplePath -Raw
$bundleEnvExample = $bundleEnvExample.Replace("BACKEND_IMAGE=mahjong-backend:latest", "BACKEND_IMAGE=$backendImage")
$bundleEnvExample = $bundleEnvExample.Replace("FRONTEND_IMAGE=mahjong-frontend:latest", "FRONTEND_IMAGE=$frontendImage")
Set-Content -Path $bundleEnvExamplePath -Value $bundleEnvExample

$instructions = @"
Bundle ready:
  $bundleDir

Server deploy steps:
  1. Upload this folder to the server.
  2. Copy .env.example to .env and adjust APP_PORT / MAHJONG_DATABASE_URL if needed.
  3. Run:
     docker load -i mahjong-images.tar
     docker compose up -d
"@

Set-Content -Path (Join-Path $bundleDir "README.txt") -Value $instructions

Write-Host ""
Write-Host $instructions
