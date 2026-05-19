param(
    [Parameter(Mandatory = $true)]
    [string] $InstallerPath
)

$ErrorActionPreference = "Stop"

$installer = (Resolve-Path $InstallerPath).Path
$checksum = "$installer.sha256"
$hash = (Get-FileHash $installer -Algorithm SHA256).Hash.ToLowerInvariant()
"$hash  $(Split-Path -Leaf $installer)" | Set-Content -Path $checksum -Encoding ascii

Get-Item $installer, $checksum | Select-Object FullName, Length
