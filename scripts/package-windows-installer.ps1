param(
    [Parameter(Mandatory = $true)]
    [string] $Version
)

$ErrorActionPreference = "Stop"

$repo = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$dist = Join-Path $repo "dist"
$installer = Join-Path $dist "scratchpad-v$Version-windows-x64.msi"
$checksum = "$installer.sha256"

New-Item -ItemType Directory -Force -Path $dist | Out-Null

$distResolved = (Resolve-Path $dist).Path
foreach ($target in @($installer, $checksum)) {
    $full = [System.IO.Path]::GetFullPath($target)
    if (-not $full.StartsWith($distResolved, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to remove outside dist: $full"
    }
}

if (Test-Path $installer) { Remove-Item -LiteralPath $installer -Force }
if (Test-Path $checksum) { Remove-Item -LiteralPath $checksum -Force }

dotnet tool run wix build `
    (Join-Path $repo "packaging\windows\scratchpad.wxs") `
    -arch x64 `
    -d "AppVersion=$Version" `
    -d "SourceRoot=$repo" `
    -o $installer `
    -pdbtype none
if ($LASTEXITCODE -ne 0) {
    throw "WiX failed with exit code $LASTEXITCODE."
}

& (Join-Path $PSScriptRoot "update-windows-installer-checksum.ps1") -InstallerPath $installer
