param(
    [int]$Port = 8000,
    [switch]$Flamegraph,
    [switch]$FullUpdate,
    [switch]$FlamegraphOnly,
    [switch]$SearchSpeedOnly,
    [switch]$CloneOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptRoot
$isWindowsPlatform = $false
if (Get-Variable -Name IsWindows -ErrorAction SilentlyContinue) {
    $isWindowsPlatform = [bool]$IsWindows
} elseif ($env:OS -eq "Windows_NT") {
    $isWindowsPlatform = $true
}

if ($isWindowsPlatform) {
    $python = Join-Path $repoRoot ".venv\Scripts\python.exe"
} else {
    $python = Join-Path $repoRoot ".venv/bin/python"
}
$activePort = $Port

function Write-Step {
    param(
        [int]$Number,
        [int]$Total,
        [string]$Title
    )

    $percent = [math]::Floor((($Number - 1) / $Total) * 100)
    Write-Progress -Id 1 -Activity "Preparing overview" -Status $Title -PercentComplete $percent
    Write-Host ""
    Write-Host "[$Number/$Total] $Title" -ForegroundColor Cyan
}

function Ensure-Python {
    $venvDir = Join-Path $repoRoot ".venv"

    if (-not (Test-Path $python)) {
        Write-Host "Creating Python virtual environment..." -ForegroundColor Cyan
        & python -m venv $venvDir
    }

    $pythonHealthy = $false
    if (Test-Path $python) {
        try {
            & $python --version *> $null
            $pythonHealthy = ($LASTEXITCODE -eq 0)
        }
        catch {
            $pythonHealthy = $false
        }
    }

    if (-not $pythonHealthy) {
        Write-Host "Recreating broken Python virtual environment..." -ForegroundColor Yellow
        if (Test-Path $venvDir) {
            Remove-Item -Recurse -Force -LiteralPath $venvDir
        }
        & python -m venv $venvDir
        & $python --version *> $null
        if ($LASTEXITCODE -ne 0) {
            throw "Python virtual environment at '$python' could not be created successfully."
        }
    }
}

function Invoke-StepCommand {
    param(
        [string]$Label,
        [string[]]$Arguments
    )

    Write-Host "Running: $Label" -ForegroundColor DarkGray
    & $python @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "'$Label' failed with exit code $LASTEXITCODE."
    }
}

function Test-PortListening {
    param(
        [int]$TestPort
    )

    $connections = Get-NetTCPConnection -LocalPort $TestPort -State Listen -ErrorAction SilentlyContinue
    return $null -ne $connections
}

function Get-AvailablePort {
    param(
        [int]$StartPort,
        [int]$MaxAttempts = 20
    )

    for ($candidate = $StartPort; $candidate -lt ($StartPort + $MaxAttempts); $candidate++) {
        if (-not (Test-PortListening -TestPort $candidate)) {
            return $candidate
        }
    }

    throw "Could not find an available port starting at $StartPort."
}

function Wait-ForServer {
    param(
        [string]$Url,
        [int]$Attempts = 20,
        [int]$DelayMilliseconds = 500
    )

    for ($attempt = 1; $attempt -le $Attempts; $attempt++) {
        try {
            Invoke-WebRequest -Uri $Url -UseBasicParsing -TimeoutSec 2 | Out-Null
            return $true
        }
        catch {
            Start-Sleep -Milliseconds $DelayMilliseconds
        }
    }

    return $false
}

function New-OverviewTask {
    param(
        [string]$Title,
        [string]$Label,
        [string[]]$Arguments,
        [string]$ParallelGroup = ""
    )

    return [pscustomobject]@{
        Title = $Title
        Label = $Label
        Arguments = $Arguments
        ParallelGroup = $ParallelGroup
    }
}

function Get-ParallelGroupTitle {
    param(
        [string]$ParallelGroup,
        [int]$TaskCount
    )

    switch ($ParallelGroup) {
        "static-analysis" {
            if ($TaskCount -eq 1) {
                return "Generating static analysis data"
            }

            return "Generating static analysis data in parallel"
        }
        default {
            if ($TaskCount -eq 1) {
                return "Running parallelizable overview task"
            }

            return "Running parallelizable overview tasks"
        }
    }
}

function Invoke-StepCommandsParallel {
    param(
        [string]$BatchLabel,
        [object[]]$Tasks
    )

    if ($Tasks.Count -eq 0) {
        return
    }

    $labels = $Tasks | ForEach-Object { $_.Label }
    Write-Host "Running in parallel: $($labels -join ', ')" -ForegroundColor DarkGray

    $jobs = @()
    foreach ($task in $Tasks) {
        Write-Host "Starting: $($task.Label)" -ForegroundColor DarkGray
        $jobs += Start-Job -Name $task.Label -ScriptBlock {
            param(
                [string]$PythonPath,
                [string]$RepoPath,
                [string]$TaskLabel,
                [string[]]$TaskArguments
            )

            Set-StrictMode -Version Latest
            $ErrorActionPreference = "Stop"

            Push-Location $RepoPath
            try {
                & $PythonPath @TaskArguments 2>&1
                if ($LASTEXITCODE -ne 0) {
                    throw "'$TaskLabel' failed with exit code $LASTEXITCODE."
                }
            }
            finally {
                Pop-Location
            }
        } -ArgumentList $python, $repoRoot, $task.Label, $task.Arguments
    }

    try {
        Wait-Job -Job $jobs | Out-Null

        $failures = @()
        foreach ($job in $jobs) {
            $jobOutput = @(Receive-Job -Job $job)
            if ($jobOutput.Count -gt 0) {
                Write-Host ""
                Write-Host "Output from $($job.Name):" -ForegroundColor DarkGray
                $jobOutput | ForEach-Object { Write-Host $_ }
            }

            if ($job.State -ne "Completed") {
                $reason = $job.ChildJobs[0].JobStateInfo.Reason
                if ($null -ne $reason) {
                    $failures += "'$($job.Name)' failed: $reason"
                } else {
                    $failures += "'$($job.Name)' did not complete successfully."
                }
            }
        }

        if ($failures.Count -gt 0) {
            throw ($failures -join [Environment]::NewLine)
        }
    }
    finally {
        $jobs | Remove-Job -Force -ErrorAction SilentlyContinue
    }
}

function Get-RefreshTasks {
    param(
        [switch]$IncludeFlamegraphs
    )

    $tasks = @(
        (New-OverviewTask -Title "Generating slowspots data" -Label "slowspots" -Arguments @("scripts/slowspots.py", "--mode", "visibility")),
        (New-OverviewTask -Title "Generating search speed data" -Label "search_speed" -Arguments @("scripts/search_speed.py", "--mode", "visibility")),
        (New-OverviewTask -Title "Generating capacity data" -Label "capacity_report" -Arguments @("scripts/capacity_report.py", "--mode", "visibility")),
        (New-OverviewTask -Title "Generating resource profile data" -Label "resource_profiles" -Arguments @("scripts/resource_profiles.py", "--mode", "visibility")),
        (New-OverviewTask -Title "Generating hotspots data" -Label "hotspots" -Arguments @("scripts/hotspots.py", "--mode", "visibility", "--paths", "src", "--scope", "all") -ParallelGroup "static-analysis"),
        (New-OverviewTask -Title "Generating clone alert data" -Label "clone_alert" -Arguments @("scripts/clone_alert.py", "--mode", "visibility", "--paths", "src") -ParallelGroup "static-analysis"),
        (New-OverviewTask -Title "Generating architecture map data" -Label "map" -Arguments @("scripts/map.py", "--mode", "visibility") -ParallelGroup "static-analysis")
    )

    if ($IncludeFlamegraphs) {
        $tasks += New-OverviewTask -Title "Generating flamegraph data" -Label "generate_flamegraphs" -Arguments @("scripts/generate_flamegraphs.py", "--mode", "visibility")
    }

    $tasks += New-OverviewTask -Title "Generating coordinated speed-efficiency report" -Label "speed_efficiency_report" -Arguments @("scripts/speed_efficiency_report.py", "--mode", "visibility")

    return $tasks
}

Push-Location $repoRoot
try {
    $exclusiveModes = @()
    if ($FullUpdate) { $exclusiveModes += "-FullUpdate" }
    if ($FlamegraphOnly) { $exclusiveModes += "-FlamegraphOnly" }
    if ($SearchSpeedOnly) { $exclusiveModes += "-SearchSpeedOnly" }
    if ($CloneOnly) { $exclusiveModes += "-CloneOnly" }

    if ($exclusiveModes.Count -gt 1) {
        throw "Use only one explicit update mode at a time: $($exclusiveModes -join ', ')."
    }

    if ($Flamegraph -and $exclusiveModes.Count -gt 0) {
        throw "Legacy switch -Flamegraph cannot be combined with the explicit update modes."
    }

    $updateMode = "fast"
    $tasks = @()

    if ($FullUpdate) {
        $updateMode = "full"
        $tasks = @(Get-RefreshTasks -IncludeFlamegraphs)
    } elseif ($FlamegraphOnly) {
        $updateMode = "flamegraph-only"
        $tasks = @(
            New-OverviewTask -Title "Generating flamegraph data" -Label "generate_flamegraphs" -Arguments @("scripts/generate_flamegraphs.py", "--mode", "visibility")
        )
    } elseif ($SearchSpeedOnly) {
        $updateMode = "search-speed-only"
        $tasks = @(
            New-OverviewTask -Title "Generating search speed data" -Label "search_speed" -Arguments @("scripts/search_speed.py", "--mode", "visibility")
        )
    } elseif ($CloneOnly) {
        $updateMode = "clone-only"
        $tasks = @(
            New-OverviewTask -Title "Generating clone alert data" -Label "clone_alert" -Arguments @("scripts/clone_alert.py", "--mode", "visibility", "--paths", "src")
        )
    } elseif ($Flamegraph) {
        $updateMode = "flamegraph-only"
        $tasks = @(
            New-OverviewTask -Title "Generating flamegraph data" -Label "generate_flamegraphs" -Arguments @("scripts/generate_flamegraphs.py", "--mode", "visibility")
        )
    }

    $taskStepCount = 0
    $seenParallelGroups = @{}
    foreach ($task in $tasks) {
        if ([string]::IsNullOrWhiteSpace($task.ParallelGroup)) {
            $taskStepCount++
            continue
        }

        if (-not $seenParallelGroups.ContainsKey($task.ParallelGroup)) {
            $seenParallelGroups[$task.ParallelGroup] = $true
            $taskStepCount++
        }
    }

    $totalSteps = 2
    if ($tasks.Count -gt 0) {
        $totalSteps += 1 + $taskStepCount
    }
    $stepNumber = 1

    Ensure-Python

    if ($tasks.Count -gt 0) {
        Write-Step -Number $stepNumber -Total $totalSteps -Title "Checking Python environment"
        Write-Host "Using Python: $python" -ForegroundColor Green
        Write-Host "Update mode: $updateMode" -ForegroundColor Green
        $stepNumber++

        $taskIndex = 0
        while ($taskIndex -lt $tasks.Count) {
            $task = $tasks[$taskIndex]
            if ([string]::IsNullOrWhiteSpace($task.ParallelGroup)) {
                Write-Step -Number $stepNumber -Total $totalSteps -Title $task.Title
                Invoke-StepCommand -Label $task.Label -Arguments $task.Arguments
                $stepNumber++
                $taskIndex++
                continue
            }

            $parallelTasks = @()
            $parallelGroup = $task.ParallelGroup
            while (
                $taskIndex -lt $tasks.Count -and
                $tasks[$taskIndex].ParallelGroup -eq $parallelGroup
            ) {
                $parallelTasks += $tasks[$taskIndex]
                $taskIndex++
            }

            $parallelTitle = Get-ParallelGroupTitle -ParallelGroup $parallelGroup -TaskCount $parallelTasks.Count
            Write-Step -Number $stepNumber -Total $totalSteps -Title $parallelTitle
            Invoke-StepCommandsParallel -BatchLabel $parallelTitle -Tasks $parallelTasks
            $stepNumber++
        }
    }

    $startTitle = if ($tasks.Count -eq 0) {
        "Starting viewer server"
    } else {
        "Starting Python web server"
    }
    Write-Step -Number $stepNumber -Total $totalSteps -Title $startTitle
    $activePort = Get-AvailablePort -StartPort $Port
    if ($activePort -ne $Port) {
        Write-Host "Port $Port is already in use. Using port $activePort instead." -ForegroundColor Yellow
    }
    $serverProcess = Start-Process -FilePath $python `
        -ArgumentList @("-m", "http.server", "$activePort") `
        -WorkingDirectory $repoRoot `
        -PassThru
    Write-Host "Started server with PID $($serverProcess.Id) on port $activePort." -ForegroundColor Green

    $viewerUrl = "http://localhost:$activePort/viewer/?v=$(Get-Date -Format 'yyyyMMddHHmmss')"

    if (-not (Wait-ForServer -Url $viewerUrl)) {
        throw "The local web server did not become ready at $viewerUrl."
    }
    $stepNumber++

    Write-Step -Number $stepNumber -Total $totalSteps -Title "Opening overview in your default browser"
    Start-Process $viewerUrl | Out-Null
    Write-Host "Opened $viewerUrl" -ForegroundColor Green

    Write-Progress -Id 1 -Activity "Preparing overview" -Completed
}
finally {
    Pop-Location
}
