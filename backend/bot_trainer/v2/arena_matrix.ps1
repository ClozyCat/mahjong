param(
    [int]$Matches = 200,
    [int]$Seed = 20260429,
    [ValidateSet(0, 1)]
    [int]$RandomSeed = 0,
    [string]$PolicyPool = "",
    [string]$OutputDir = "",
    [int]$ProgressEvery = 10,
    [int]$Jobs = 0,
    [string]$CargoExe = "cargo"
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $ScriptDir "..\..\..")).Path
$BackendManifest = Join-Path $RepoRoot "backend\Cargo.toml"
$OriginalLocation = (Get-Location).Path

function Resolve-UserPath {
    param(
        [string]$Path,
        [string]$DefaultPath
    )

    $candidate = if ([string]::IsNullOrWhiteSpace($Path)) { $DefaultPath } else { $Path }
    if ([System.IO.Path]::IsPathRooted($candidate)) {
        return $candidate
    }
    return (Join-Path $OriginalLocation $candidate)
}

function Test-JsonProperty {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Object,
        [Parameter(Mandatory = $true)]
        [string]$Name
    )

    return $null -ne ($Object.PSObject.Properties | Where-Object { $_.Name -eq $Name } | Select-Object -First 1)
}

function ConvertTo-ArenaPolicy {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Policy,
        [Parameter(Mandatory = $true)]
        [int]$Index
    )

    foreach ($required in @("id", "mode")) {
        if (-not (Test-JsonProperty -Object $Policy -Name $required) -or [string]::IsNullOrWhiteSpace([string]$Policy.$required)) {
            throw "Policy at index $Index must define '$required'."
        }
    }

    $mode = ([string]$Policy.mode).Trim().ToLowerInvariant()
    if (@("heuristic", "neural") -notcontains $mode) {
        throw "Policy '$($Policy.id)' has unsupported mode '$($Policy.mode)'. Expected heuristic or neural."
    }

    $arenaPolicy = [ordered]@{
        id = [string]$Policy.id
        mode = $mode
        model_path = if (Test-JsonProperty -Object $Policy -Name "model_path") { $Policy.model_path } else { $null }
    }

    if (Test-JsonProperty -Object $Policy -Name "sample_actions") {
        $arenaPolicy.sample_actions = [bool]$Policy.sample_actions
    }
    if (Test-JsonProperty -Object $Policy -Name "temperature") {
        $arenaPolicy.temperature = [double]$Policy.temperature
    }

    return $arenaPolicy
}

function Read-ArenaPolicies {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Policy pool was not found: $Path"
    }

    $pool = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
    if (Test-JsonProperty -Object $pool -Name "policies") {
        $rawPolicies = @($pool.policies)
    }
    elseif ((Test-JsonProperty -Object $pool -Name "learner") -and (Test-JsonProperty -Object $pool -Name "opponents")) {
        $rawPolicies = @($pool.learner) + @($pool.opponents)
    }
    else {
        throw "Policy pool must contain either 'policies' or 'learner' plus 'opponents': $Path"
    }

    if ($rawPolicies.Count -ne 4) {
        throw "Policy pool must define exactly 4 arena models, but found $($rawPolicies.Count): $Path"
    }

    $index = 0
    return @($rawPolicies | ForEach-Object {
            $policy = ConvertTo-ArenaPolicy -Policy $_ -Index $index
            $index += 1
            $policy
        })
}

function Write-Utf8NoBom {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,
        [Parameter(Mandatory = $true)]
        [string]$Content
    )

    $encoding = New-Object System.Text.UTF8Encoding $false
    [System.IO.File]::WriteAllText($Path, $Content, $encoding)
}

function Add-Utf8NoBomFile {
    param(
        [Parameter(Mandatory = $true)]
        [string]$SourcePath,
        [Parameter(Mandatory = $true)]
        [string]$TargetPath
    )

    if (-not (Test-Path -LiteralPath $SourcePath -PathType Leaf)) {
        throw "Arena chunk output was not found: $SourcePath"
    }

    $encoding = New-Object System.Text.UTF8Encoding $false
    [System.IO.File]::AppendAllText($TargetPath, [System.IO.File]::ReadAllText($SourcePath), $encoding)
}

function Format-SeatPolicyIds {
    param(
        [Parameter(Mandatory = $true)]
        [object[]]$Policies
    )

    return (($Policies | ForEach-Object { $_.id }) -join ", ")
}

function Switch-SeatPolicies {
    param(
        [Parameter(Mandatory = $true)]
        [object[]]$Policies,
        [Parameter(Mandatory = $true)]
        [int]$RotationStep
    )

    switch ($RotationStep % 3) {
        0 { return @($Policies[2], $Policies[3], $Policies[0], $Policies[1]) }
        1 { return @($Policies[1], $Policies[0], $Policies[3], $Policies[2]) }
        default { return @($Policies[3], $Policies[1], $Policies[2], $Policies[0]) }
    }
}

function New-ArenaSeed {
    return (Get-Random -Minimum 1 -Maximum ([int]::MaxValue))
}

function Write-ArenaSummary {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        Write-Warning "Arena output was not found: $Path"
        return
    }

    $reports = @(Get-Content -LiteralPath $Path | Where-Object { $_.Trim() } | ConvertFrom-Json)
    if ($reports.Count -eq 0) {
        Write-Warning "Arena output is empty: $Path"
        return
    }

    $completed = @($reports | Where-Object { $_.completed }).Count
    $totalActions = ($reports | Measure-Object -Property action_count -Sum).Sum
    $avgActions = if ($reports.Count -gt 0) { $totalActions / $reports.Count } else { 0 }

    Write-Host ""
    Write-Host "Arena summary"
    Write-Host "Output: $Path"
    Write-Host ("Matches: {0} completed={1} incomplete={2} avg_actions={3:N1}" -f `
        $reports.Count, $completed, ($reports.Count - $completed), $avgActions)
    Write-Host ""
    Write-Host "Policy summary:"

    $seatRows = @($reports | ForEach-Object { $_.seats })
    $seatRows |
        Group-Object -Property policy_id |
        Sort-Object -Property Name |
        ForEach-Object {
            $rows = @($_.Group)
            $scoreSum = ($rows | Measure-Object -Property score_delta -Sum).Sum
            $wins = ($rows | Measure-Object -Property wins -Sum).Sum
            $dealtIn = ($rows | Measure-Object -Property dealt_in -Sum).Sum
            $decisions = ($rows | Measure-Object -Property decision_count -Sum).Sum
            $latencySum = ($rows | Measure-Object -Property decision_latency_ms_sum -Sum).Sum
            $avgScore = if ($rows.Count -gt 0) { $scoreSum / $rows.Count } else { 0 }
            $avgLatency = if ($decisions -gt 0) { $latencySum / $decisions } else { 0 }
            $tenpai = @($rows | Where-Object { $_.final_tenpai }).Count
            Write-Host ("  {0,-10} seats={1,4} wins={2,3} dealt_in={3,3} score_sum={4,7} avg_score={5,7:N1} decisions={6,6} avg_latency_ms={7,6:N1} final_tenpai={8,3}" -f `
                $_.Name, $rows.Count, $wins, $dealtIn, $scoreSum, $avgScore, $decisions, $avgLatency, $tenpai)
        }
}

$PolicyPool = Resolve-UserPath -Path $PolicyPool -DefaultPath (Join-Path $ScriptDir "arena_policy_pool.json")
$OutputDir = Resolve-UserPath -Path $OutputDir -DefaultPath (Join-Path $ScriptDir "arena_runs")

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
$seatPolicies = @(Read-ArenaPolicies -Path $PolicyPool)

$outputPath = Join-Path $OutputDir "arena_results.jsonl"
if (Test-Path -LiteralPath $outputPath) {
    Remove-Item -LiteralPath $outputPath -Force
}

if ($ProgressEvery -le 0) {
    throw "ProgressEvery must be greater than 0 because seat rotation happens on each progress report."
}

Write-Host "Initial seat order: $(Format-SeatPolicyIds -Policies $seatPolicies)"
Write-Host "Policy pool: $PolicyPool"
Write-Host "Output: $OutputDir"
Write-Host "Random seed: $RandomSeed"

Push-Location $RepoRoot
try {
    $completedMatches = 0
    $chunkIndex = 0
    $rotationStep = 0
    $currentPolicies = @($seatPolicies)

    while ($completedMatches -lt $Matches) {
        $chunkMatches = [Math]::Min($ProgressEvery, $Matches - $completedMatches)
        $chunkSeed = if ($RandomSeed -eq 1) { New-ArenaSeed } else { $Seed + $completedMatches }
        $chunkConfigPath = Join-Path $OutputDir ("arena_config_{0:D3}.json" -f $chunkIndex)
        $chunkOutputPath = Join-Path $OutputDir ("arena_results_{0:D3}.jsonl" -f $chunkIndex)
        if (Test-Path -LiteralPath $chunkOutputPath) {
            Remove-Item -LiteralPath $chunkOutputPath -Force
        }

        $config = @{
            matches = $chunkMatches
            seed = $chunkSeed
            max_actions_per_match = 2400
            report_trajectories = $false
            policies = $currentPolicies
        }
        Write-Utf8NoBom -Path $chunkConfigPath -Content (($config | ConvertTo-Json -Depth 8) + "`n")

        $arenaArgs = @(
            "run",
            "--manifest-path", $BackendManifest,
            "--release",
            "--bin", "bot_arena",
            "--",
            "--config", $chunkConfigPath,
            "--output", $chunkOutputPath,
            "--jobs", "$Jobs"
        )

        & $CargoExe @arenaArgs
        if ($LASTEXITCODE -ne 0) {
            exit $LASTEXITCODE
        }

        Add-Utf8NoBomFile -SourcePath $chunkOutputPath -TargetPath $outputPath
        $completedMatches += $chunkMatches
        Write-Host ("Arena progress: completed {0}/{1} chunk={2} seed={3} seats={4}" -f `
            $completedMatches, $Matches, ($chunkIndex + 1), $chunkSeed, (Format-SeatPolicyIds -Policies $currentPolicies))

        if ($completedMatches -lt $Matches) {
            $currentPolicies = @(Switch-SeatPolicies -Policies $currentPolicies -RotationStep $rotationStep)
            $swapName = switch ($rotationStep % 3) {
                0 { "1<->3, 2<->4" }
                1 { "1<->2, 3<->4" }
                default { "1<->4" }
            }
            Write-Host "Next seat order after swap $swapName`: $(Format-SeatPolicyIds -Policies $currentPolicies)"
            $rotationStep += 1
        }

        $chunkIndex += 1
    }
}
finally {
    Pop-Location
}

Write-ArenaSummary -Path $outputPath
