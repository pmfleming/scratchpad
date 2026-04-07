param(
    [string[]]$Paths = @("src"),
    [int]$Top = 15,
    [ValidateSet("all", "files", "functions")]
    [string]$Scope = "all",
    [switch]$IncludeAnonymous
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot

function Get-MetricValue {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Metrics,
        [Parameter(Mandatory = $true)]
        [string]$Group,
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [double]$Default = 0
    )

    if (-not $Metrics) {
        return $Default
    }

    $groupValue = $Metrics.$Group
    if (-not $groupValue) {
        return $Default
    }

    $value = $groupValue.$Name
    if ($null -eq $value) {
        return $Default
    }

    return [double]$value
}

function Get-HotspotScore {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Node
    )

    $metrics = $Node.metrics
    $cognitive = Get-MetricValue -Metrics $metrics -Group "cognitive" -Name "sum"
    $cyclomatic = Get-MetricValue -Metrics $metrics -Group "cyclomatic" -Name "sum"
    $mi = Get-MetricValue -Metrics $metrics -Group "mi" -Name "mi_visual_studio" -Default 100
    $effort = Get-MetricValue -Metrics $metrics -Group "halstead" -Name "effort"
    $sloc = Get-MetricValue -Metrics $metrics -Group "loc" -Name "sloc"

    $score = 0.0
    $score += $cognitive * 4.0
    $score += $cyclomatic * 2.5
    $score += [Math]::Max(0.0, 70.0 - $mi) * 1.5
    $score += [Math]::Min(30.0, $effort / 1000.0)
    $score += [Math]::Min(20.0, $sloc / 10.0)

    return [Math]::Round($score, 2)
}

function Get-HotspotSignals {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Node
    )

    $metrics = $Node.metrics
    $signals = [System.Collections.Generic.List[string]]::new()

    $cognitive = Get-MetricValue -Metrics $metrics -Group "cognitive" -Name "sum"
    if ($cognitive -ge 8) {
        $signals.Add("high cognitive=$cognitive")
    } elseif ($cognitive -ge 4) {
        $signals.Add("cognitive=$cognitive")
    }

    $cyclomatic = Get-MetricValue -Metrics $metrics -Group "cyclomatic" -Name "sum"
    if ($cyclomatic -ge 12) {
        $signals.Add("high cyclomatic=$cyclomatic")
    } elseif ($cyclomatic -ge 6) {
        $signals.Add("cyclomatic=$cyclomatic")
    }

    $mi = Get-MetricValue -Metrics $metrics -Group "mi" -Name "mi_visual_studio" -Default 100
    if ($mi -lt 20) {
        $signals.Add(("very low MI={0:N1}" -f $mi))
    } elseif ($mi -lt 40) {
        $signals.Add(("low MI={0:N1}" -f $mi))
    } elseif ($mi -lt 60) {
        $signals.Add(("MI={0:N1}" -f $mi))
    }

    $effort = Get-MetricValue -Metrics $metrics -Group "halstead" -Name "effort"
    if ($effort -ge 15000) {
        $signals.Add(("very high effort={0:N0}" -f $effort))
    } elseif ($effort -ge 5000) {
        $signals.Add(("effort={0:N0}" -f $effort))
    }

    $sloc = Get-MetricValue -Metrics $metrics -Group "loc" -Name "sloc"
    if ($sloc -ge 150) {
        $signals.Add("large sloc=$sloc")
    } elseif ($sloc -ge 40) {
        $signals.Add("sloc=$sloc")
    }

    if ($signals.Count -eq 0) {
        $signals.Add("no major thresholds crossed")
    }

    return $signals -join ", "
}

function Add-HotspotNode {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Node,
        [Parameter(Mandatory = $true)]
        [AllowEmptyCollection()]
        [System.Collections.Generic.List[object]]$Results
    )

    $entry = [PSCustomObject]@{
        Name       = $Node.name
        Kind       = $Node.kind
        StartLine  = $Node.start_line
        EndLine    = $Node.end_line
        Score      = Get-HotspotScore -Node $Node
        Signals    = Get-HotspotSignals -Node $Node
        Cognitive  = Get-MetricValue -Metrics $Node.metrics -Group "cognitive" -Name "sum"
        Cyclomatic = Get-MetricValue -Metrics $Node.metrics -Group "cyclomatic" -Name "sum"
        MI         = Get-MetricValue -Metrics $Node.metrics -Group "mi" -Name "mi_visual_studio" -Default 100
        Effort     = Get-MetricValue -Metrics $Node.metrics -Group "halstead" -Name "effort"
        SLOC       = Get-MetricValue -Metrics $Node.metrics -Group "loc" -Name "sloc"
    }
    $Results.Add($entry)

    foreach ($child in @($Node.spaces)) {
        Add-HotspotNode -Node $child -Results $Results
    }
}

Push-Location $repoRoot
try {
    $tool = Get-Command rust-code-analysis-cli -ErrorAction Stop
    $jsonLines = & $tool.Source --metrics --paths $Paths --output-format json
    $nodes = $jsonLines | Where-Object { $_.Trim() } | ForEach-Object {
        $normalized = $_ `
            -replace '"N1"\s*:', '"halstead_N1":' `
            -replace '"N2"\s*:', '"halstead_N2":'
        $normalized | ConvertFrom-Json
    }

    $all = [System.Collections.Generic.List[object]]::new()
    foreach ($node in $nodes) {
        Add-HotspotNode -Node $node -Results $all
    }

    $filtered = switch ($Scope) {
        "files" { $all | Where-Object { $_.Kind -eq "unit" } }
        "functions" { $all | Where-Object { $_.Kind -eq "function" } }
        default { $all | Where-Object { $_.Kind -in @("unit", "function") } }
    }

    if (-not $IncludeAnonymous) {
        $filtered = $filtered | Where-Object { $_.Name -ne "<anonymous>" }
    }

    $ranked = $filtered |
        Sort-Object -Property @{ Expression = "Score"; Descending = $true }, @{ Expression = "Name"; Descending = $false } |
        Select-Object -First $Top

    Write-Host "Hotspots from rust-code-analysis-cli"
    Write-Host "Paths: $($Paths -join ', ')"
    Write-Host "Scope: $Scope"
    Write-Host ""

    $index = 1
    foreach ($item in $ranked) {
        $location = "{0}:{1}" -f $item.Name, $item.StartLine
        Write-Host ("{0,2}. [{1}] {2}" -f $index, $item.Kind, $location)
        Write-Host ("    score={0} | cognitive={1} | cyclomatic={2} | MI={3:N1} | effort={4:N0} | sloc={5}" -f $item.Score, $item.Cognitive, $item.Cyclomatic, $item.MI, $item.Effort, $item.SLOC)
        Write-Host ("    signals: {0}" -f $item.Signals)
        $index++
    }
}
finally {
    Pop-Location
}
