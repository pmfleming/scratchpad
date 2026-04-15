param(
    [ValidateSet("debug", "release")]
    [string]$Profile = "release",
    [string]$OutputDir = (Join-Path $PSScriptRoot "..\dist"),
    [string]$Version = "",
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$resolvedOutputDir = [System.IO.Path]::GetFullPath($OutputDir)
$cargoProfileDir = if ($Profile -eq "release") { "release" } else { "debug" }
$packageVersion = if ($Version) {
    $Version
} else {
    (cargo metadata --no-deps --format-version 1 | ConvertFrom-Json).packages[0].version
}
$packageName = "scratchpad-v$packageVersion-windows-x64"
$stagingDir = Join-Path $resolvedOutputDir $packageName
$archivePath = Join-Path $resolvedOutputDir "$packageName.zip"
$checksumPath = "$archivePath.sha256"

function Assert-PathInsideDirectory {
    param(
        [string]$Path,
        [string]$Directory
    )

    $resolvedPath = [System.IO.Path]::GetFullPath($Path)
    $resolvedDirectory = [System.IO.Path]::GetFullPath($Directory).TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar

    if (-not $resolvedPath.StartsWith($resolvedDirectory, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to clean path outside output directory: '$resolvedPath'."
    }
}

Push-Location $repoRoot
try {
    if (-not $SkipBuild) {
        if ($Profile -eq "release") {
            cargo build --release --locked
        } else {
            cargo build --locked
        }
    }

    $exePath = Join-Path $repoRoot "target\$cargoProfileDir\scratchpad.exe"
    if (-not (Test-Path $exePath)) {
        throw "Scratchpad executable not found at '$exePath'."
    }

    New-Item -ItemType Directory -Force -Path $resolvedOutputDir | Out-Null
    Assert-PathInsideDirectory -Path $stagingDir -Directory $resolvedOutputDir
    if (Test-Path $stagingDir) {
        Remove-Item -LiteralPath $stagingDir -Recurse -Force
    }
    if (Test-Path $archivePath) {
        Remove-Item -LiteralPath $archivePath -Force
    }
    if (Test-Path $checksumPath) {
        Remove-Item -LiteralPath $checksumPath -Force
    }

    New-Item -ItemType Directory -Force -Path $stagingDir | Out-Null
    Copy-Item -LiteralPath $exePath -Destination (Join-Path $stagingDir "scratchpad.exe")
    Copy-Item -LiteralPath (Join-Path $repoRoot "README.md") -Destination (Join-Path $stagingDir "README.md")
    Copy-Item -LiteralPath (Join-Path $repoRoot "docs\user-manual.md") -Destination (Join-Path $stagingDir "user-manual.md")
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot "register-open-with.ps1") -Destination (Join-Path $stagingDir "register-open-with.ps1")

    $licensePath = Join-Path $repoRoot "LICENSE"
    if (Test-Path $licensePath) {
        Copy-Item -LiteralPath $licensePath -Destination (Join-Path $stagingDir "LICENSE")
    }

    Compress-Archive -Path (Join-Path $stagingDir "*") -DestinationPath $archivePath -CompressionLevel Optimal
    $hash = (Get-FileHash $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $(Split-Path -Leaf $archivePath)" | Set-Content -Path $checksumPath -Encoding ascii

    Write-Host "Created $archivePath"
    Write-Host "Created $checksumPath"
}
finally {
    Pop-Location
}
