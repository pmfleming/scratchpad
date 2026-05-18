param(
    [switch]$FixFormatting,
    [switch]$SkipComplexity,
    [switch]$SkipSlowspots,
    [switch]$SkipSearchSpeed,
    [switch]$SkipClones,
    [switch]$SkipTypeHealth,
    [switch]$SkipLocality,
    [switch]$SkipLeverage
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$isWindowsPlatform = $false
if (Get-Variable -Name IsWindows -ErrorAction SilentlyContinue) {
    $isWindowsPlatform = [bool]$IsWindows
}
elseif ($env:OS -eq "Windows_NT") {
    $isWindowsPlatform = $true
}

function Invoke-NativeCommand {
    param(
        [string]$Label,
        [scriptblock]$Command
    )

    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "'$Label' failed with exit code $LASTEXITCODE."
    }
}

function Initialize-PythonTooling {
    param(
        [string]$RepoRoot,
        [string]$ScriptRoot
    )

    $venvDir = Join-Path $RepoRoot ".venv"
    if ($isWindowsPlatform) {
        $python = Join-Path $venvDir "Scripts\python.exe"
    }
    else {
        $python = Join-Path $venvDir "bin/python"
    }

    if (-not (Test-Path $python)) {
        Write-Host "Creating Python virtual environment..." -ForegroundColor Cyan
        Invoke-NativeCommand -Label "python -m venv" -Command { & python -m venv $venvDir }
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
        Invoke-NativeCommand -Label "python -m venv" -Command { & python -m venv $venvDir }
        Invoke-NativeCommand -Label "python --version" -Command { & $python --version *> $null }
    }

    return $python
}

Push-Location $repoRoot
try {
    if ($FixFormatting) {
        Invoke-NativeCommand -Label "cargo fmt" -Command { cargo fmt }
    }

    Invoke-NativeCommand -Label "cargo fmt --check" -Command { cargo fmt --check }
    Invoke-NativeCommand -Label "cargo clippy" -Command { cargo clippy --lib --all-features -- -D warnings }
    Invoke-NativeCommand -Label "cargo test" -Command { cargo test }

    $needsPythonTooling = (-not $SkipComplexity) -or (-not $SkipSlowspots) -or (-not $SkipSearchSpeed) -or (-not $SkipClones) -or (-not $SkipTypeHealth) -or (-not $SkipLocality) -or (-not $SkipLeverage)
    if ($needsPythonTooling) {
        $python = Initialize-PythonTooling -RepoRoot $repoRoot -ScriptRoot $PSScriptRoot
        $analysisDir = Join-Path $repoRoot "target\analysis"
        New-Item -ItemType Directory -Force -Path $analysisDir | Out-Null
    }

    if (-not $SkipComplexity) {
        Invoke-NativeCommand -Label "rust-quality-lens hotspots" -Command {
            & $python (Join-Path $PSScriptRoot "rqlens.py") measure hotspots
        }
    }

    if (-not $SkipSlowspots) {
        Invoke-NativeCommand -Label "scratchpad-performance-lens slowspots" -Command {
            & $python (Join-Path $PSScriptRoot "splens.py") measure slowspots --fail-on-slow
        }
    }

    if (-not $SkipSearchSpeed) {
        Invoke-NativeCommand -Label "scratchpad-performance-lens search" -Command {
            & $python (Join-Path $PSScriptRoot "splens.py") measure search --fail-on-slow
        }
    }

    if (-not $SkipClones) {
        Invoke-NativeCommand -Label "rust-quality-lens clones" -Command {
            & $python (Join-Path $PSScriptRoot "rqlens.py") measure clones
        }
    }

    if (-not $SkipTypeHealth) {
        Invoke-NativeCommand -Label "rust-quality-lens type health" -Command {
            & $python (Join-Path $PSScriptRoot "rqlens.py") measure type-health
        }
    }

    if (-not $SkipLocality) {
        Invoke-NativeCommand -Label "rust-quality-lens locality" -Command {
            & $python (Join-Path $PSScriptRoot "rqlens.py") measure locality
        }
    }

    if (-not $SkipLeverage) {
        Invoke-NativeCommand -Label "rust-quality-lens leverage" -Command {
            & $python (Join-Path $PSScriptRoot "rqlens.py") measure leverage
        }
    }
}
finally {
    Pop-Location
}
