#Requires -Version 7.0
[CmdletBinding()]
param(
  [switch]$SkipTests,
  [string]$Build = ""
)
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# Native deployment for this workstation: run the tests, build one release
# payload, and hand it to the staged Athanor manager. The manager stages the
# payload under versions/<release>, backs the database up, migrates, flips
# current.json, and restarts the service. Running sessions keep the version
# they loaded; only new sessions read the new pointer. This script never
# writes under the program root itself.
#
# The release identity is the product version plus one '+build' suffix, so
# every deployment lands in its own directory and the manager refuses to
# land the same build twice.

if (-not $IsWindows) {
  throw "deploy-local.ps1 supports the canonical Windows + WSL deployment path only"
}

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
. (Join-Path $Root "installer/native-release-contract.ps1")

$ProgramRoot = Join-Path $env:ProgramFiles "Solarisael/Athanor"
$InstalledManager = Join-Path $ProgramRoot "bin/athanor-manage.exe"
if (-not (Test-Path -LiteralPath $InstalledManager -PathType Leaf)) {
  throw "The installed Athanor manager is missing: $InstalledManager"
}
$Service = Get-Service -Name "SolarisaelAthanor" -ErrorAction SilentlyContinue
if ($null -ne $Service -and $Service.Status -notin @("Running", "Stopped")) {
  throw "native Athanor service is $($Service.Status); recover it to Running or Stopped before deployment"
}

function Invoke-Checked {
  param(
    [Parameter(Mandatory = $true)][string]$Label,
    [Parameter(Mandatory = $true)][string]$FilePath,
    [string[]]$ArgumentList = @(),
    [string]$WorkingDirectory = $Root
  )
  Write-Host "==> $Label"
  Push-Location $WorkingDirectory
  try {
    & $FilePath @ArgumentList
    if ($LASTEXITCODE -ne 0) { throw "$Label failed (exit code $LASTEXITCODE)" }
  } finally {
    Pop-Location
  }
}

# --- release identity ---
if ([string]::IsNullOrWhiteSpace($Build)) {
  $Stamp = (Get-Date).ToUniversalTime().ToString("yyyyMMddHHmm")
  $Commit = (& git -C $Root rev-parse --short HEAD).Trim()
  if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($Commit)) { throw "git could not name HEAD" }
  $Dirty = if ((& git -C $Root status --porcelain --untracked-files=no) | Where-Object { $_ }) { ".dirty" } else { "" }
  $Build = "dev.$Stamp.$Commit$Dirty"
}
$Authority = Get-NativeReleaseVersion -RepositoryRoot $Root
$Release = Get-NativeReleaseVersion -RepositoryRoot $Root -Requested "$Authority+$Build"
Write-Host "release: $Release"

# --- tests ---
if (-not $SkipTests) {
  Invoke-Checked -Label "workspace tests" -FilePath "cargo" -ArgumentList @("test", "--workspace")
  Invoke-Checked -Label "OMP adapter tests" -FilePath "pwsh" -ArgumentList @(
    "-NoProfile", "-File", (Join-Path $Root "adapters/omp/deploy-local.ps1"), "-TestsOnly"
  )
}

# --- payload ---
$Out = Join-Path $Root "target/deploy/$Release"
Invoke-Checked -Label "native release payload" -FilePath "pwsh" -ArgumentList @(
  "-NoProfile", "-File", (Join-Path $Root "installer/build-native-release.ps1"),
  "-Version", $Release, "-OutDir", $Out
)
$Payload = Join-Path $Out "payload"
$Manifest = Join-Path $Payload "release-manifest.json"
$StagedManager = Join-Path $Payload "bin/athanor-manage.exe"
foreach ($Required in @($Manifest, $StagedManager)) {
  if (-not (Test-Path -LiteralPath $Required -PathType Leaf)) { throw "payload is missing $Required" }
}

# --- install through the staged manager: it carries the new installer code
# and is not the image the service runs, so it can retire the stable one ---
Invoke-Checked -Label "install release $Release" -FilePath $StagedManager -ArgumentList @(
  "update", "--staging", $Payload, "--manifest", $Manifest
)

# --- proof on the installed tree ---
Invoke-Checked -Label "native release manifest proof" -FilePath $InstalledManager -ArgumentList @("doctor")
$Current = Get-Content (Join-Path $ProgramRoot "current.json") -Raw | ConvertFrom-Json
if ([string]$Current.version -cne $Release) {
  throw "current.json names $($Current.version) after installing $Release"
}
$Substrate = Join-Path $ProgramRoot "versions/$Release/bin/athanor-substrate.exe"
$Secrets = Get-Content (Join-Path $env:ProgramData "Solarisael/Athanor/secrets/runtime-secrets.json") -Raw | ConvertFrom-Json
$SavedDatabaseUrl = $env:DATABASE_URL
try {
  $env:DATABASE_URL = [string]$Secrets.externalDatabaseUrl
  Invoke-Checked -Label "Full-mode health proof" -FilePath $Substrate -ArgumentList @(
    "health", "--substrate-dir", (Join-Path $Root "substrate")
  )
} finally {
  $env:DATABASE_URL = $SavedDatabaseUrl
}

# Payload trees are disposable after installation; keep this release's for
# inspection and drop older ones.
Get-ChildItem (Join-Path $Root "target/deploy") -Directory -ErrorAction SilentlyContinue |
  Where-Object { $_.Name -cne $Release } |
  Remove-Item -Recurse -Force -ErrorAction SilentlyContinue

Write-Host "==> deployment complete"
Write-Host "release: $Release (previous: $($Current.previousVersion))"
Write-Host "restart OMP once before the next Athanor tool call so its transport and TypeScript tool schemas reload"
