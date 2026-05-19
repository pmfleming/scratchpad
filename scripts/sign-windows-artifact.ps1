param(
    [Parameter(Mandatory = $true)]
    [string] $FilePath,

    [Parameter(Mandatory = $true)]
    [string] $PfxBase64,

    [Parameter(Mandatory = $true)]
    [string] $PfxPassword,

    [string] $Description = "Scratchpad",

    [string] $DescriptionUrl = "https://github.com/pmfleming/scratchpad",

    [string] $TimestampUrl = "http://timestamp.digicert.com"
)

$ErrorActionPreference = "Stop"

function Get-SignToolPath {
    $candidatePatterns = @(
        "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe",
        "${env:ProgramFiles(x86)}\Microsoft SDKs\ClickOnce\SignTool\signtool.exe"
    )

    foreach ($pattern in $candidatePatterns) {
        $match = Get-ChildItem -Path $pattern -ErrorAction SilentlyContinue |
            Sort-Object FullName -Descending |
            Select-Object -First 1
        if ($match) {
            return $match.FullName
        }
    }

    throw "Could not find signtool.exe. Install the Windows SDK on this runner."
}

$resolvedFile = (Resolve-Path $FilePath).Path
$signTool = Get-SignToolPath
$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid().ToString("N"))
$pfxPath = Join-Path $tempDir "codesign.pfx"

New-Item -ItemType Directory -Force -Path $tempDir | Out-Null

try {
    [System.IO.File]::WriteAllBytes($pfxPath, [System.Convert]::FromBase64String($PfxBase64))

    & $signTool sign `
        /fd SHA256 `
        /tr $TimestampUrl `
        /td SHA256 `
        /f $pfxPath `
        /p $PfxPassword `
        /d $Description `
        /du $DescriptionUrl `
        $resolvedFile
    if ($LASTEXITCODE -ne 0) {
        throw "signtool sign failed with exit code $LASTEXITCODE."
    }

    & $signTool verify /pa /v $resolvedFile
    if ($LASTEXITCODE -ne 0) {
        throw "signtool verify failed with exit code $LASTEXITCODE."
    }
}
finally {
    if (Test-Path $tempDir) {
        Remove-Item -LiteralPath $tempDir -Recurse -Force
    }
}
