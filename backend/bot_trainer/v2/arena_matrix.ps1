param(
    [int]$MatchCount = 200,
    [int]$Seed = 20260429,
    [ValidateSet(0, 1)]
    [int]$RandomSeed = 0,
    [string]$Config = "",
    [string]$OutputDir = "",
    [int]$ProgressEvery = 100,
    [int]$Jobs = 1,
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

function Read-EvaluationConfigTemplate {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Arena evaluation config was not found: $Path"
    }

    $template = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
    if ((Test-JsonProperty -Object $template -Name "policies") -or (Test-JsonProperty -Object $template -Name "learner")) {
        throw "Arena matrix now accepts only evaluation configs with 'subjects' and exactly three 'opponents': $Path"
    }
    if (-not (Test-JsonProperty -Object $template -Name "subjects")) {
        throw "Arena evaluation config must define 'subjects': $Path"
    }
    if (-not (Test-JsonProperty -Object $template -Name "opponents")) {
        throw "Arena evaluation config must define 'opponents': $Path"
    }
    $subjects = @($template.subjects)
    $opponents = @($template.opponents)
    if ($subjects.Count -lt 1) {
        throw "Arena evaluation config must define at least one subject: $Path"
    }
    if ($opponents.Count -ne 3) {
        throw "Arena evaluation config must define exactly three opponents, found $($opponents.Count): $Path"
    }
    $index = 0
    foreach ($subject in $subjects) {
        foreach ($required in @("id", "display_name", "model_path")) {
            if (-not (Test-JsonProperty -Object $subject -Name $required) -or [string]::IsNullOrWhiteSpace([string]$subject.$required)) {
                throw "Arena evaluation config subject at index $index must define '$required': $Path"
            }
        }
        $index += 1
    }
    $index = 0
    foreach ($opponent in $opponents) {
        foreach ($required in @("id", "model_path")) {
            if (-not (Test-JsonProperty -Object $opponent -Name $required) -or [string]::IsNullOrWhiteSpace([string]$opponent.$required)) {
                throw "Arena evaluation config opponent at index $index must define '$required': $Path"
            }
        }
        $index += 1
    }

    return $template
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

function Format-PolicyIds {
    param(
        [Parameter(Mandatory = $true)]
        [object[]]$Policies
    )

    return (($Policies | ForEach-Object { $_.id }) -join ", ")
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
            $modelLoaded = @($rows | Where-Object { $_.model_loaded }).Count
            $neuralActions = ($rows | ForEach-Object { if (Test-JsonProperty -Object $_ -Name "neural_action_count") { [int64]$_.neural_action_count } else { 0 } } | Measure-Object -Sum).Sum
            $avgScore = if ($rows.Count -gt 0) { $scoreSum / $rows.Count } else { 0 }
            $avgLatency = if ($decisions -gt 0) { $latencySum / $decisions } else { 0 }
            $tenpai = @($rows | Where-Object { $_.final_tenpai }).Count
            Write-Host ("  {0,-10} seats={1,4} wins={2,3} dealt_in={3,3} score_sum={4,7} avg_score={5,7:N1} decisions={6,6} avg_latency_ms={7,6:N1} final_tenpai={8,3} model_loaded={9,4} neural_actions={10,5}" -f `
                $_.Name, $rows.Count, $wins, $dealtIn, $scoreSum, $avgScore, $decisions, $avgLatency, $tenpai, $modelLoaded, $neuralActions)
        }
}

$Config = Resolve-UserPath -Path $Config -DefaultPath (Join-Path $ScriptDir "arena_policy_pool.json")
$OutputDir = Resolve-UserPath -Path $OutputDir -DefaultPath (Join-Path $ScriptDir "arena_runs")

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
$configTemplate = Read-EvaluationConfigTemplate -Path $Config
$subjects = @($configTemplate.subjects)
$opponents = @($configTemplate.opponents)

$outputPath = Join-Path $OutputDir "arena_results.jsonl"
if (Test-Path -LiteralPath $outputPath) {
    Remove-Item -LiteralPath $outputPath -Force
}

if ($ProgressEvery -le 0) {
    throw "ProgressEvery must be greater than 0."
}

Write-Host "Subjects: $(Format-PolicyIds -Policies $subjects)"
Write-Host "Opponents: $(Format-PolicyIds -Policies $opponents)"
Write-Host "Arena config: $Config"
Write-Host "Output: $OutputDir"
Write-Host "Random seed: $RandomSeed"

Push-Location $RepoRoot
try {
    $completedMatches = 0
    $chunkIndex = 0

    while ($completedMatches -lt $MatchCount) {
        $chunkMatches = [Math]::Min($ProgressEvery, $MatchCount - $completedMatches)
        $chunkSeed = if ($RandomSeed -eq 1) { New-ArenaSeed } else { $Seed + $completedMatches }
        $chunkConfigPath = Join-Path $OutputDir ("arena_config_{0:D3}.json" -f $chunkIndex)
        $chunkOutputPath = Join-Path $OutputDir ("arena_results_{0:D3}.jsonl" -f $chunkIndex)
        if (Test-Path -LiteralPath $chunkOutputPath) {
            Remove-Item -LiteralPath $chunkOutputPath -Force
        }

        $maxActionsPerMatch = if (Test-JsonProperty -Object $configTemplate -Name "max_actions_per_match") { [int]$configTemplate.max_actions_per_match } else { 2400 }
        $reportTrajectories = if (Test-JsonProperty -Object $configTemplate -Name "report_trajectories") { [bool]$configTemplate.report_trajectories } else { $false }
        $chunkArenaConfig = [PSCustomObject][ordered]@{
            matches = $chunkMatches
            seed = $chunkSeed
            max_actions_per_match = $maxActionsPerMatch
            report_trajectories = $reportTrajectories
            subjects = $subjects
            opponents = $opponents
        }
        Write-Utf8NoBom -Path $chunkConfigPath -Content (($chunkArenaConfig | ConvertTo-Json -Depth 8) + "`n")

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
        Write-Host ("Arena progress: completed {0}/{1} chunk={2} seed={3} subjects={4}" -f `
            $completedMatches, $MatchCount, ($chunkIndex + 1), $chunkSeed, (Format-PolicyIds -Policies $subjects))

        $chunkIndex += 1
    }
}
finally {
    Pop-Location
}

Write-ArenaSummary -Path $outputPath
