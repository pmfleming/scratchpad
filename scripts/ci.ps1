param(
    [switch]$FixFormatting,
    [switch]$SkipComplexity
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot

Push-Location $repoRoot
try {
    if ($FixFormatting) {
        cargo fmt
    }

    cargo fmt --check
    cargo clippy --all-targets --all-features -- -D warnings
    cargo test

    if (-not $SkipComplexity) {
        $venvDir = Join-Path $repoRoot ".venv"
        $python = Join-Path $venvDir "Scripts\python.exe"

        if (-not (Test-Path $python)) {
            Write-Host "Creating Python virtual environment..." -ForegroundColor Cyan
            & python -m venv $venvDir
            & $python -m pip install --quiet matplotlib jinja2 pandas
        }

        & $python (Join-Path $PSScriptRoot "hotspots.py") --paths src --top 20 --scope all
    }
}
finally {
    Pop-Location
}
