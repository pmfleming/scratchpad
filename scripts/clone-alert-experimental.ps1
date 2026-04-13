Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$python = Join-Path $repoRoot ".venv\Scripts\python.exe"

if (-not (Test-Path $python)) {
    throw "Python virtual environment not found at '$python'."
}

Push-Location $repoRoot
try {
    & $python "scripts/clone_alert.py" --paths src --engine all @args
    if ($LASTEXITCODE -ne 0) {
        throw "Experimental clone analysis failed with exit code $LASTEXITCODE."
    }
}
finally {
    Pop-Location
}
