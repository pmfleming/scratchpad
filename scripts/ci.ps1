param(
    [switch]$FixFormatting
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
}
finally {
    Pop-Location
}
