param(
    [int]$Matches = 200,
    [int]$Seed = 20260429,
    [string]$OutputDir = "backend/bot_trainer/v2/arena_runs",
    [int]$ProgressEvery = 10,
    [int]$Jobs = 0,
    [string]$SeatOrder = "default"
)

$ErrorActionPreference = "Stop"

function New-PolicyConfig {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Id
    )

    switch ($Id) {
        "heuristic" { return @{ id = "heuristic"; mode = "heuristic"; neural_weight = 0; model_path = $null } }
        "neural" { return @{ id = "neural"; mode = "neural"; neural_weight = 0; model_path = "backend/assets/models/mahjong_policy_net.onnx" } }
        default { throw "Unknown policy id: $Id" }
    }
}

function Resolve-SeatPolicies {
    param(
        [Parameter(Mandatory = $true)]
        [string]$SeatOrder
    )

    $presets = @{
        "default" = @("heuristic", "neural", "heuristic", "neural")
        "current" = @("heuristic", "neural", "heuristic", "neural")
        "rotate1" = @("neural", "heuristic", "neural", "heuristic")
    }

    $normalized = $SeatOrder.Trim()
    $presetKey = $normalized.ToLowerInvariant()
    if ($presets.ContainsKey($presetKey)) {
        $policyIds = $presets[$presetKey]
    }
    else {
        $policyIds = @($normalized -split "," | ForEach-Object { $_.Trim() } | Where-Object { $_ })
    }

    if ($policyIds.Count -ne 4) {
        throw "SeatOrder must resolve to exactly 4 policy ids. Presets: $($presets.Keys -join ', '). Or use custom form: heuristic,neural,heuristic,neural"
    }

    $known = @("heuristic", "neural")
    foreach ($policyId in $policyIds) {
        if ($known -notcontains $policyId) {
            throw "Unknown policy id '$policyId'. Known policy ids: $($known -join ', ')"
        }
    }

    return @($policyIds | ForEach-Object { New-PolicyConfig -Id $_ })
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

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
$seatPolicies = @(Resolve-SeatPolicies -SeatOrder $SeatOrder)
$seatOrderIds = @($seatPolicies | ForEach-Object { $_.id })

$config = @{
    matches = $Matches
    seed = $Seed
    max_actions_per_match = 2400
    report_trajectories = $false
    policies = $seatPolicies
}

$configPath = Join-Path $OutputDir "arena_config.json"
$outputPath = Join-Path $OutputDir "arena_results.jsonl"
$config | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 $configPath

Write-Host "Seat order: $($seatOrderIds -join ', ')"

$arenaArgs = @(
    "run",
    "--manifest-path", "backend/Cargo.toml",
    "--release",
    "--bin", "bot_arena",
    "--",
    "--config", $configPath,
    "--output", $outputPath,
    "--progress-every", "$ProgressEvery",
    "--jobs", "$Jobs"
)

& cargo @arenaArgs
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

Write-ArenaSummary -Path $outputPath
