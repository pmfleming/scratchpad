param(
    [int]$Port = 8000,
    [switch]$Refresh,
    [switch]$CloneCheck
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptRoot
if ($IsWindows) {
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

Push-Location $repoRoot
try {
    if ($Refresh -and $CloneCheck) {
        throw "Use either -Refresh or -CloneCheck, not both."
    }

    $rebuildMode = if ($CloneCheck) {
        "clonecheck"
    } elseif ($Refresh) {
        "refresh"
    } else {
        "fast"
    }

    $totalSteps = if ($rebuildMode -eq "fast") { 2 } else { 7 }
    $stepNumber = 1

    Ensure-Python

    if ($rebuildMode -ne "fast") {
        Write-Step -Number $stepNumber -Total $totalSteps -Title "Checking Python environment"
        Write-Host "Using Python: $python" -ForegroundColor Green
        $stepNumber++

        Write-Step -Number $stepNumber -Total $totalSteps -Title "Generating slowspots data"
        Invoke-StepCommand -Label "slowspots" -Arguments @("scripts/slowspots.py", "--mode", "visibility")
        $stepNumber++

        Write-Step -Number $stepNumber -Total $totalSteps -Title "Generating hotspots data"
        Invoke-StepCommand -Label "hotspots" -Arguments @("scripts/hotspots.py", "--mode", "visibility", "--paths", "src", "--scope", "all")
        $stepNumber++

        Write-Step -Number $stepNumber -Total $totalSteps -Title "Generating clone alert data"
        $cloneArguments = @("scripts/clone_alert.py", "--mode", "visibility", "--paths", "src")
        if ($rebuildMode -eq "clonecheck") {
            $cloneArguments += @("--engine", "all")
        }
        Invoke-StepCommand -Label "clone_alert" -Arguments $cloneArguments
        $stepNumber++

        Write-Step -Number $stepNumber -Total $totalSteps -Title "Generating architecture map data"
        Invoke-StepCommand -Label "map" -Arguments @("scripts/map.py", "--mode", "visibility")
        $stepNumber++
    }

    $startTitle = if ($rebuildMode -eq "fast") {
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
