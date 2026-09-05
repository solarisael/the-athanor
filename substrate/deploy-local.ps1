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
# The native build compiles pgvector with the Visual Studio toolset, and its
# preflight refuses a shell that is not an x64 developer shell. Enter one here
# when the caller did not; the child build inherits it.
if ([string]::IsNullOrWhiteSpace($env:VCToolsInstallDir) -or ([string]$env:VSCMD_ARG_TGT_ARCH) -ne "x64") {
  $VsWhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio/Installer/vswhere.exe"
  if (-not (Test-Path -LiteralPath $VsWhere -PathType Leaf)) { throw "Visual Studio is required for the native build; vswhere is missing at $VsWhere" }
  $VsRoot = (& $VsWhere -latest -products "*" -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath | Select-Object -First 1)
  if ([string]::IsNullOrWhiteSpace($VsRoot)) { throw "no Visual Studio with the x64 C++ toolset is installed" }
  $VsRoot = $VsRoot.Trim()
  Write-Host "==> Visual Studio x64 developer shell ($VsRoot)"
  Import-Module (Join-Path $VsRoot "Common7/Tools/Microsoft.VisualStudio.DevShell.dll")
  Enter-VsDevShell -VsInstallPath $VsRoot -SkipAutomaticLocation -DevCmdArguments "-arch=x64 -host_arch=x64" | Out-Null
}
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

# --- the adapter component belongs to the same release: without this step
# a native deploy leaves the installed TypeScript behind, and every adapter
# cut waits for somebody to remember the other driver. Its tests ran above.
Invoke-Checked -Label "install OMP adapter component" -FilePath "pwsh" -ArgumentList @(
  "-NoProfile", "-File", (Join-Path $Root "adapters/omp/deploy-local.ps1"), "-SkipTests"
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
  # The proof asks the embedder for one real embedding. A cold Ollama model
  # can refuse the first request while it loads, which is not an install
  # failure. Three tries, ten seconds apart, before this counts as red.
  $HealthTries = 3
  for ($Try = 1; $Try -le $HealthTries; $Try += 1) {
    Write-Host "==> Full-mode health proof (try $Try of $HealthTries)"
    & $Substrate health --substrate-dir (Join-Path $Root "substrate")
    if ($LASTEXITCODE -eq 0) { break }
    if ($Try -eq $HealthTries) { throw "Full-mode health proof failed (exit code $LASTEXITCODE)" }
    Start-Sleep -Seconds 10
  }
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
