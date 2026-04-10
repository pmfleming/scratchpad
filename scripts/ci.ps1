param(
    [switch]$FixFormatting,
    [switch]$SkipComplexity,
    [switch]$SkipSlowspots
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot

function Ensure-PythonTooling {
    param(
        [string]$RepoRoot,
        [string]$ScriptRoot
    )

    $venvDir = Join-Path $RepoRoot ".venv"
    $python = Join-Path $venvDir "Scripts\python.exe"

    if (-not (Test-Path $python)) {
        Write-Host "Creating Python virtual environment..." -ForegroundColor Cyan
        & python -m venv $venvDir
    }

    return $python
}

Push-Location $repoRoot
try {
    if ($FixFormatting) {
        cargo fmt
    }

    cargo fmt --check
    cargo clippy --all-targets --all-features -- -D warnings
    cargo test

    $needsPythonTooling = (-not $SkipComplexity) -or (-not $SkipSlowspots)
    if ($needsPythonTooling) {
        $python = Ensure-PythonTooling -RepoRoot $repoRoot -ScriptRoot $PSScriptRoot
        $analysisDir = Join-Path $repoRoot "target\analysis"
        New-Item -ItemType Directory -Force -Path $analysisDir | Out-Null
    }

    if (-not $SkipComplexity) {
        & $python (Join-Path $PSScriptRoot "hotspots.py") --paths src --scope all --output (Join-Path $analysisDir "hotspots.json")
    }

    if (-not $SkipSlowspots) {
        & $python (Join-Path $PSScriptRoot "slowspots.py") --output (Join-Path $analysisDir "slowspots.json") --fail-on-slow
    }
}
finally {
    Pop-Location
}
