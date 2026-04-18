[CmdletBinding()]
param(
    [int]$FrontendPort = 5173,
    [int]$BackendPort = 8000,
    [switch]$SkipFrontendInstall,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Assert-CommandExists {
    param(
        [Parameter(Mandatory = $true)]
        [string]$CommandName
    )

    if (-not (Get-Command $CommandName -ErrorAction SilentlyContinue)) {
        throw "$CommandName not found. Please install it first."
    }
}

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

function Convert-ToSingleQuotedPowerShellString {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Value
    )

    return "'" + $Value.Replace("'", "''") + "'"
}

function Test-PortAvailable {
    param(
        [Parameter(Mandatory = $true)]
        [int]$Port
    )

    $listener = $null
    try {
        $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, $Port)
        $listener.Start()
        return $true
    } catch [System.Net.Sockets.SocketException] {
        return $false
    } finally {
        if ($null -ne $listener) {
            $listener.Stop()
        }
    }
}

function Wait-PortReady {
    param(
        [Parameter(Mandatory = $true)]
        [string]$HostName,
        [Parameter(Mandatory = $true)]
        [int]$Port,
        [int]$TimeoutSeconds = 30
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        $client = New-Object System.Net.Sockets.TcpClient
        try {
            $asyncResult = $client.BeginConnect($HostName, $Port, $null, $null)
            if ($asyncResult.AsyncWaitHandle.WaitOne(500) -and $client.Connected) {
                $client.EndConnect($asyncResult)
                return $true
            }
        } catch {
            Start-Sleep -Milliseconds 500
        } finally {
            $client.Dispose()
        }

        Start-Sleep -Milliseconds 500
    }

    return $false
}

function Start-DebugWindow {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Title,
        [Parameter(Mandatory = $true)]
        [string]$WorkingDirectory,
        [Parameter(Mandatory = $true)]
        [string]$CommandText
    )

    $titleLiteral = Convert-ToSingleQuotedPowerShellString -Value $Title
    $command = @"
`$Host.UI.RawUI.WindowTitle = $titleLiteral
$CommandText
"@

    Start-Process -FilePath "powershell.exe" `
        -WorkingDirectory $WorkingDirectory `
        -ArgumentList @(
            "-NoLogo",
            "-NoExit",
            "-ExecutionPolicy", "Bypass",
            "-Command", $command
        ) | Out-Null
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$frontendDir = Join-Path $repoRoot "frontend"
$backendDir = Join-Path $repoRoot "backend"
$devDataDir = Join-Path $repoRoot "output\dev"
$databasePath = [System.IO.Path]::GetFullPath((Join-Path $devDataDir "mahjong-dev.db")).Replace('\', '/')
$frontendHost = "127.0.0.1"
$backendHost = "127.0.0.1"
$frontendUrl = "http://${frontendHost}:${FrontendPort}"
$backendUrl = "http://${backendHost}:${BackendPort}"

Assert-CommandExists -CommandName "node"
Assert-CommandExists -CommandName "npm"
Assert-CommandExists -CommandName "cargo"

if (-not (Test-Path $frontendDir)) {
    throw "Frontend directory not found at $frontendDir"
}

if (-not (Test-Path $backendDir)) {
    throw "Backend directory not found at $backendDir"
}

if (-not (Test-PortAvailable -Port $FrontendPort)) {
    throw "Frontend port $FrontendPort is already in use."
}

if (-not (Test-PortAvailable -Port $BackendPort)) {
    throw "Backend port $BackendPort is already in use."
}

New-Item -ItemType Directory -Force -Path $devDataDir | Out-Null

$frontendNodeModules = Join-Path $frontendDir "node_modules"
if (-not $SkipFrontendInstall -and -not (Test-Path $frontendNodeModules)) {
    $packageLock = Join-Path $frontendDir "package-lock.json"
    if (Test-Path $packageLock) {
        Write-Host "Installing frontend dependencies with npm ci..."
        Invoke-CheckedCommand -FilePath "npm" -Arguments "ci" -WorkingDirectory $frontendDir
    } else {
        Write-Host "Installing frontend dependencies with npm install..."
        Invoke-CheckedCommand -FilePath "npm" -Arguments "install" -WorkingDirectory $frontendDir
    }
}

$backendCommand = @"
Set-Location -LiteralPath $(Convert-ToSingleQuotedPowerShellString -Value $backendDir)
`$env:MAHJONG_BIND_ADDR = $(Convert-ToSingleQuotedPowerShellString -Value "${backendHost}:${BackendPort}")
`$env:MAHJONG_DATABASE_URL = $(Convert-ToSingleQuotedPowerShellString -Value $databasePath)
Write-Host "Backend starting on $backendUrl" -ForegroundColor Cyan
Write-Host "SQLite database: $databasePath" -ForegroundColor DarkGray
cargo run
if (`$LASTEXITCODE -ne 0) {
    Write-Host ""
    Write-Host "Backend exited with code `$LASTEXITCODE" -ForegroundColor Red
}
"@

$frontendCommand = @"
Set-Location -LiteralPath $(Convert-ToSingleQuotedPowerShellString -Value $frontendDir)
`$env:VITE_API_BASE_URL = $(Convert-ToSingleQuotedPowerShellString -Value $backendUrl)
`$env:VITE_WS_BASE_URL = $(Convert-ToSingleQuotedPowerShellString -Value ("ws://${backendHost}:${BackendPort}"))
Write-Host "Frontend starting on $frontendUrl" -ForegroundColor Cyan
Write-Host "API base URL: $backendUrl" -ForegroundColor DarkGray
npm run dev -- --host $frontendHost --port $FrontendPort --strictPort
if (`$LASTEXITCODE -ne 0) {
    Write-Host ""
    Write-Host "Frontend exited with code `$LASTEXITCODE" -ForegroundColor Red
}
"@

if ($DryRun) {
    Write-Host "Dry run only. No windows will be launched."
    Write-Host "Frontend directory: $frontendDir"
    Write-Host "Backend directory:  $backendDir"
    Write-Host "Frontend URL:       $frontendUrl"
    Write-Host "Backend URL:        $backendUrl"
    Write-Host "Database path:      $databasePath"
    Write-Host "Frontend install:   $([bool](-not $SkipFrontendInstall))"
    exit 0
}

Write-Host "Launching backend window..."
Start-DebugWindow -Title "Mahjong Backend" -WorkingDirectory $backendDir -CommandText $backendCommand

Write-Host "Launching frontend window..."
Start-DebugWindow -Title "Mahjong Frontend" -WorkingDirectory $frontendDir -CommandText $frontendCommand

Write-Host "Waiting for frontend to become ready..."
if (Wait-PortReady -HostName $frontendHost -Port $FrontendPort -TimeoutSeconds 30) {
    Start-Process $frontendUrl | Out-Null
    Write-Host "Browser opened: $frontendUrl"
} else {
    Write-Warning "Frontend did not become ready within 30 seconds. Check the frontend window logs."
}

Write-Host ""
Write-Host "Dev environment started."
Write-Host "Frontend: $frontendUrl"
Write-Host "Backend:  $backendUrl"
