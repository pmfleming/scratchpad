param(
    [int]$Port = 8000
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptRoot
$python = Join-Path $repoRoot ".venv\Scripts\python.exe"
$viewerUrl = "http://localhost:$Port/viewer/"

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
    if (-not (Test-Path $python)) {
        throw "Python virtual environment not found at '$python'."
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
    $totalSteps = 6

    Write-Step -Number 1 -Total $totalSteps -Title "Checking Python environment"
    Ensure-Python
    Write-Host "Using Python: $python" -ForegroundColor Green

    Write-Step -Number 2 -Total $totalSteps -Title "Generating slowspots data"
    Invoke-StepCommand -Label "slowspots" -Arguments @("scripts/slowspots.py", "--mode", "visibility")

    Write-Step -Number 3 -Total $totalSteps -Title "Generating hotspots data"
    Invoke-StepCommand -Label "hotspots" -Arguments @("scripts/hotspots.py", "--mode", "visibility", "--paths", "src", "--scope", "all")

    Write-Step -Number 4 -Total $totalSteps -Title "Generating architecture map data"
    Invoke-StepCommand -Label "map" -Arguments @("scripts/map.py", "--mode", "visibility")

    Write-Step -Number 5 -Total $totalSteps -Title "Starting Python web server"
    if (Test-PortListening -TestPort $Port) {
        Write-Host "Port $Port is already in use. Reusing the existing server." -ForegroundColor Yellow
    }
    else {
        $serverProcess = Start-Process -FilePath $python `
            -ArgumentList @("-m", "http.server", "$Port") `
            -WorkingDirectory $repoRoot `
            -PassThru
        Write-Host "Started server with PID $($serverProcess.Id)." -ForegroundColor Green
    }

    if (-not (Wait-ForServer -Url $viewerUrl)) {
        throw "The local web server did not become ready at $viewerUrl."
    }

    Write-Step -Number 6 -Total $totalSteps -Title "Opening overview in your default browser"
    Start-Process $viewerUrl | Out-Null
    Write-Host "Opened $viewerUrl" -ForegroundColor Green

    Write-Progress -Id 1 -Activity "Preparing overview" -Completed
}
finally {
    Pop-Location
}
