param(
    [string]$ExecutablePath = (Join-Path $PSScriptRoot "..\target\release\scratchpad.exe"),
    [string[]]$Extensions = @('.txt', '.log', '.md', '.rs', '.json', '.toml'),
    [switch]$Remove
)

$resolvedExecutable = [System.IO.Path]::GetFullPath($ExecutablePath)
$appKey = 'HKCU:\Software\Classes\Applications\scratchpad.exe'
$commandKey = Join-Path $appKey 'shell\open\command'
$supportedTypesKey = Join-Path $appKey 'SupportedTypes'

if ($Remove) {
    Remove-Item $appKey -Recurse -Force -ErrorAction SilentlyContinue
    Write-Host "Removed Scratchpad Open With registration from HKCU."
    exit 0
}

if (-not (Test-Path $resolvedExecutable)) {
    throw "Scratchpad executable not found at '$resolvedExecutable'. Build the app first or pass -ExecutablePath explicitly."
}

New-Item -Path $commandKey -Force | Out-Null
Set-Item -Path $commandKey -Value ('"{0}" "%1"' -f $resolvedExecutable)

New-Item -Path $supportedTypesKey -Force | Out-Null
foreach ($extension in $Extensions) {
    New-ItemProperty -Path $supportedTypesKey -Name $extension -PropertyType String -Value '' -Force | Out-Null
}

Write-Host "Registered Scratchpad as an Open With target for: $($Extensions -join ', ')"
Write-Host "Command: `"$resolvedExecutable`" `"%1`""