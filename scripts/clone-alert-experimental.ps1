Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
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

Push-Location $repoRoot
try {
    Ensure-Python
    & $python "scripts/clone_alert.py" --paths src --engine all @args
    if ($LASTEXITCODE -ne 0) {
        throw "Experimental clone analysis failed with exit code $LASTEXITCODE."
    }
}
finally {
    Pop-Location
}
