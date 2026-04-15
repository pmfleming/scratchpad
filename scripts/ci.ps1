param(
    [switch]$FixFormatting,
    [switch]$SkipComplexity,
    [switch]$SkipSlowspots,
    [switch]$SkipSearchSpeed,
    [switch]$SkipClones
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot

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

function Ensure-PythonTooling {
    param(
        [string]$RepoRoot,
        [string]$ScriptRoot
    )

    $venvDir = Join-Path $RepoRoot ".venv"
    if ($IsWindows) {
        $python = Join-Path $venvDir "Scripts\python.exe"
    } else {
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
    Invoke-NativeCommand -Label "cargo clippy" -Command { cargo clippy --all-targets --all-features -- -D warnings }
    Invoke-NativeCommand -Label "cargo test" -Command { cargo test }

    $needsPythonTooling = (-not $SkipComplexity) -or (-not $SkipSlowspots) -or (-not $SkipSearchSpeed) -or (-not $SkipClones)
    if ($needsPythonTooling) {
        $python = Ensure-PythonTooling -RepoRoot $repoRoot -ScriptRoot $PSScriptRoot
        $analysisDir = Join-Path $repoRoot "target\analysis"
        New-Item -ItemType Directory -Force -Path $analysisDir | Out-Null
    }

    if (-not $SkipComplexity) {
        Invoke-NativeCommand -Label "hotspots.py" -Command {
            & $python (Join-Path $PSScriptRoot "hotspots.py") --paths src --scope all --output (Join-Path $analysisDir "hotspots.json")
        }
    }

    if (-not $SkipSlowspots) {
        Invoke-NativeCommand -Label "slowspots.py" -Command {
            & $python (Join-Path $PSScriptRoot "slowspots.py") --output (Join-Path $analysisDir "slowspots.json") --fail-on-slow
        }
    }

    if (-not $SkipSearchSpeed) {
        Invoke-NativeCommand -Label "search_speed.py" -Command {
            & $python (Join-Path $PSScriptRoot "search_speed.py") --output (Join-Path $analysisDir "search_speed.json") --fail-on-slow
        }
    }

    if (-not $SkipClones) {
        Invoke-NativeCommand -Label "clone_alert.py" -Command {
            & $python (Join-Path $PSScriptRoot "clone_alert.py") --paths src --output (Join-Path $analysisDir "clones.json")
        }
    }
}
finally {
    Pop-Location
}
