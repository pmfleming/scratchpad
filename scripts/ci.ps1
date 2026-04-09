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

    $imports = "import jinja2, matplotlib, numpy, pandas, squarify"
    & $python -c $imports *> $null
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Installing Python analysis dependencies..." -ForegroundColor Cyan
        & $python -m pip install --quiet jinja2 matplotlib numpy pandas squarify
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
    }

    if (-not $SkipComplexity) {
        & $python (Join-Path $PSScriptRoot "hotspots.py") --paths src --top 20 --scope all
    }

    if (-not $SkipSlowspots) {
        & $python (Join-Path $PSScriptRoot "slowspots.py") --mode slowspots
    }
}
finally {
    Pop-Location
}
