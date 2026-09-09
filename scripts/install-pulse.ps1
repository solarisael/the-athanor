param(
    [switch]$NoBuild,
    [switch]$NoDesktop,
    [string]$Profile
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$localData = $env:LOCALAPPDATA
$programs = [Environment]::GetFolderPath('Programs')
$desktop = [Environment]::GetFolderPath('DesktopDirectory')
if ($Profile) {
    $Profile = (Resolve-Path -LiteralPath $Profile).Path
    $localData = Join-Path $Profile 'AppData/Local'
    $programs = Join-Path $Profile 'AppData/Roaming/Microsoft/Windows/Start Menu/Programs'
    $desktop = Join-Path $Profile 'Desktop'
}
if (-not $NoBuild) {
    Push-Location $repo
    try {
        & cargo build --release -p athanor-pulse
        if ($LASTEXITCODE -ne 0) { throw 'Pulse release build failed.' }
    } finally {
        Pop-Location
    }
}

Push-Location $repo
try {
    $metadata = & cargo metadata --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Cannot locate the Cargo output directory.' }
    $targetRoot = ($metadata | ConvertFrom-Json).target_directory
} finally {
    Pop-Location
}
$source = Join-Path $targetRoot 'release/pulse.exe'
if (-not (Test-Path $source -PathType Leaf)) { throw "Pulse binary not found: $source" }
$installDir = Join-Path $localData 'Solarisael/Pulse'
$shimDir = Join-Path $localData 'Microsoft/WindowsApps'
New-Item -ItemType Directory -Force -Path $installDir, $shimDir | Out-Null
$exe = Join-Path $installDir 'pulse.exe'
Copy-Item -LiteralPath $source -Destination $exe -Force
$shim = "@echo off`r`n`"$exe`" %*`r`n"
[IO.File]::WriteAllText((Join-Path $shimDir 'pulse.cmd'), $shim, [Text.Encoding]::Default)

$shell = New-Object -ComObject WScript.Shell
$shortcutDirs = @($programs)
if (-not $NoDesktop) { $shortcutDirs += $desktop }
foreach ($directory in $shortcutDirs) {
    New-Item -ItemType Directory -Force -Path $directory | Out-Null
    $path = Join-Path $directory 'The Athanor — Pulse.lnk'
    $shortcut = $shell.CreateShortcut($path)
    $shortcut.TargetPath = $exe
    $shortcut.WorkingDirectory = $installDir
    $shortcut.IconLocation = "$exe,0"
    $shortcut.Description = 'Open The Athanor Pulse desktop app.'
    $shortcut.Save()
    Write-Output "Shortcut: $path"
}
Write-Output "Installed: $exe"
Write-Output "CLI: $(Join-Path $shimDir 'pulse.cmd')"
