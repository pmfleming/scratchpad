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
        & (Join-Path $PSScriptRoot "hotspots.ps1") -Paths src -Top 20 -Scope all
    }
}
finally {
    Pop-Location
}
