#Requires -Version 7.0
[CmdletBinding()]
param(
  [switch]$SkipTests,
  [string]$Build = "",
  [switch]$PrepareOnly,
  [string]$PreparedRelease = "",
  [string]$LinuxHost = "",
  [string]$SshIdentity = "",
  # The Linux Host deploy script lives with the Max client (solarisael/wda-max), not here.
  [string]$LinuxDeployScript = (Join-Path $PSScriptRoot "../../../wda-max/installer/deploy-linux-host.ps1")
)
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ($PrepareOnly -and $PSBoundParameters.ContainsKey("PreparedRelease")) {
  throw "-PrepareOnly and -PreparedRelease exclude each other"
}
if ($PSBoundParameters.ContainsKey("PreparedRelease") -and
    @("Build", "SkipTests", "LinuxHost", "SshIdentity", "LinuxDeployScript").Where({ $PSBoundParameters.ContainsKey($_) }).Count -gt 0) {
  throw "-PreparedRelease excludes -Build, -SkipTests, and Linux arguments"
}
if (-not [string]::IsNullOrWhiteSpace($LinuxHost) -and ($PrepareOnly -or $PSBoundParameters.ContainsKey("PreparedRelease"))) {
  throw "Windows preparation flags cannot be forwarded to Linux"
}

if (-not [string]::IsNullOrWhiteSpace($LinuxHost)) {
  if ([string]::IsNullOrWhiteSpace($SshIdentity)) {
    throw "Linux Host deployment requires -SshIdentity."
  }
  if (-not (Test-Path -LiteralPath $LinuxDeployScript -PathType Leaf)) {
    throw "Linux Host deploy script not found: $LinuxDeployScript (checkout solarisael/wda-max beside the-athanor-dev, or pass -LinuxDeployScript)."
  }
  & $LinuxDeployScript -RemoteHost $LinuxHost -IdentityFile $SshIdentity -AthanorRoot (Resolve-Path (Join-Path $PSScriptRoot "..")).Path -SkipTests:$SkipTests
  exit 0
}

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
. (Join-Path $Root "installer/prepared-release.ps1")


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

if ($PSBoundParameters.ContainsKey("PreparedRelease")) {
  $Prepared = Read-PreparedRelease -Path $PreparedRelease
  $Receipt = [IO.Path]::GetFullPath($PreparedRelease)
} else {
  $SavedBuildJobs = [Environment]::GetEnvironmentVariable("CARGO_BUILD_JOBS", "Process")
  $OwnedOutput = $null

  try {
    if ([string]::IsNullOrWhiteSpace($SavedBuildJobs)) { $env:CARGO_BUILD_JOBS = "1" }

    if ([string]::IsNullOrWhiteSpace($Build)) {
      $Stamp = (Get-Date).ToUniversalTime().ToString("yyyyMMddHHmmss")
      $Commit = (& git -C $Root rev-parse --short HEAD).Trim()

      if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($Commit)) { throw "git could not name HEAD" }

      $Dirty = if ((& git -C $Root status --porcelain --untracked-files=no) | Where-Object { $_ }) { ".dirty" } else { "" }
      $Build = "dev.$Stamp.$Commit$Dirty"
    }

    $Authority = Get-NativeReleaseVersion -RepositoryRoot $Root
    $Release = Get-NativeReleaseVersion -RepositoryRoot $Root -Requested "$Authority+$Build"
    Write-Host "release: $Release"
    $Out = Join-Path $Root "target/releases/$Release"

    if (Test-Path -LiteralPath $Out) { throw "release output already exists: $Out" }

    # Reserve this output without replacing another preparation.
    $null = [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($Out))
    $null = New-Item -Path $Out -ItemType Directory -ErrorAction Stop
    $OwnedOutput = $Out

    if (-not $SkipTests) {
      Invoke-Checked -Label "workspace tests" -FilePath "cargo" -ArgumentList @("test", "--workspace")
      Invoke-Checked -Label "OMP adapter tests" -FilePath "pwsh" -ArgumentList @(
        "-NoProfile", "-File", (Join-Path $Root "adapters/omp/deploy-local.ps1"), "-TestsOnly"
      )
    } else {
      Write-Host "==> tests skipped (-SkipTests); this release has no verified test gate"
    }

    # The native build requires the Visual Studio x64 developer shell.
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

    Invoke-Checked -Label "native release payload" -FilePath "pwsh" -ArgumentList @(
      "-NoProfile", "-File", (Join-Path $Root "installer/build-native-release.ps1"),
      "-Version", $Release, "-OutDir", $Out
    )
    $Payload = Join-Path $Out "payload"
    $Receipt = Write-PreparedRelease -Payload $Payload -TestsPassed (-not $SkipTests.IsPresent)
    $Prepared = Read-PreparedRelease -Path $Receipt
    Write-Host "prepared release receipt: $Receipt"
  } catch {
    $PreparationFailure = $_

    if ($null -ne $OwnedOutput -and -not (Test-Path -LiteralPath (Join-Path $OwnedOutput "prepared-release.json"))) {
      try {
        Remove-Item -LiteralPath $OwnedOutput -Recurse -Force
      } catch {
        Write-Warning "Incomplete preparation remains at ${OwnedOutput}: $($_.Exception.Message)"
      }
    }
    throw $PreparationFailure
  } finally {
    if ($null -eq $SavedBuildJobs) {
      Remove-Item Env:CARGO_BUILD_JOBS -ErrorAction SilentlyContinue
    } else {
      $env:CARGO_BUILD_JOBS = $SavedBuildJobs
    }
  }
}

$Release = $Prepared.Release
$Payload = $Prepared.Payload
$Manifest = $Prepared.Manifest
$StagedManager = $Prepared.StagedManager
Write-Host "release: $Release"
Write-Host "test gate: $(if ($Prepared.TestsPassed) { 'passed' } else { 'skipped (not verified)' })"

if ($PrepareOnly) { return }

$ProgramRoot = Join-Path $env:ProgramFiles "Solarisael/Athanor"
$InstalledManager = Join-Path $ProgramRoot "bin/athanor.exe"
if (-not (Test-Path -LiteralPath $InstalledManager -PathType Leaf)) {
  throw "The installed athanor.exe is missing: $InstalledManager"
}
$Service = Get-Service -Name "SolarisaelAthanor" -ErrorAction SilentlyContinue
if ($null -ne $Service -and $Service.Status -notin @("Running", "Stopped")) {
  throw "native Athanor service is $($Service.Status); recover it to Running or Stopped before deployment"
}

$RuntimePath = Join-Path $env:ProgramData "Solarisael/Athanor/config/runtime.json"
$RuntimeConfig = Get-Content -LiteralPath $RuntimePath -Raw | ConvertFrom-Json
$OmpConfig = if ([string]::IsNullOrWhiteSpace([string]$RuntimeConfig.ompConfigPath)) {
  Join-Path $env:USERPROFILE ".omp/agent/config.yml"
} else { [string]$RuntimeConfig.ompConfigPath }
$ClientConfig = if ([string]::IsNullOrWhiteSpace([string]$RuntimeConfig.clientConfigPath)) {
  Join-Path $env:USERPROFILE ".omp/agent/athanor/client.json"
} else { [string]$RuntimeConfig.clientConfigPath }
$OperatorPrincipal = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name
if (-not (Test-Path -LiteralPath $OmpConfig -PathType Leaf)) {
  throw "OMP configuration is missing: $OmpConfig"
}
$HostListener = Get-NetTCPConnection -State Listen -LocalPort $RuntimeConfig.hostPort -ErrorAction SilentlyContinue
$ProgramPrefix = [System.IO.Path]::GetFullPath($ProgramRoot).TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
$InstalledChildren = Get-CimInstance Win32_Process -Filter "Name = 'athanor-substrate.exe'" |
  Where-Object { $_.ExecutablePath -and [System.IO.Path]::GetFullPath($_.ExecutablePath).StartsWith($ProgramPrefix, [System.StringComparison]::OrdinalIgnoreCase) }
if ($HostListener -or $InstalledChildren) {
  throw "Close the House Host and every OMP session before deployment; NATS credential activation cannot run behind old clients."
}

# --- install through the staged manager: it carries the new installer code
# and is not the image the service runs, so it can retire the stable one ---
Invoke-Checked -Label "install release $Release" -FilePath $StagedManager -ArgumentList @(
  "update", "--staging", $Payload, "--manifest", $Manifest,
  "--omp-config", $OmpConfig, "--client-config", $ClientConfig,
  "--operator-principal", $OperatorPrincipal
)

# Install the adapter from the same verified payload, never from the checkout.
Invoke-Checked -Label "install OMP adapter component" -FilePath $InstalledManager -ArgumentList @(
  "install-omp-adapter", "--source", $Prepared.AdapterSource
)

# --- proof on the installed tree ---
Invoke-Checked -Label "native release manifest proof" -FilePath $InstalledManager -ArgumentList @("doctor")
$Current = Get-Content (Join-Path $ProgramRoot "current.json") -Raw | ConvertFrom-Json
if ([string]$Current.version -cne $Release) {
  throw "current.json names $($Current.version) after installing $Release"
}
$Substrate = Join-Path $ProgramRoot "versions/$Release/bin/athanor-substrate.exe"
$Secrets = Get-Content (Join-Path $env:ProgramData "Solarisael/Athanor/secrets/runtime-secrets.json") -Raw | ConvertFrom-Json
# The substrate refuses to guess where mutable state lives, and this shell
# rarely carries ATHANOR_STATE_DIR. The installed runtime config already knows.
$StateRoot = [string]$RuntimeConfig.operatorStateRoot
if ([string]::IsNullOrWhiteSpace($StateRoot)) { throw "runtime.json has no operatorStateRoot; the health proof cannot resolve the state root" }
$SavedDatabaseUrl = $env:DATABASE_URL
$SavedStateDir = $env:ATHANOR_STATE_DIR
try {
  $env:DATABASE_URL = [string]$Secrets.externalDatabaseUrl
  $env:ATHANOR_STATE_DIR = $StateRoot
  # The proof asks the embedder for one real embedding. A cold Ollama model
  # can refuse the first request while it loads, which is not an install
  # failure. Three tries, ten seconds apart, before this counts as red.
  $HealthTries = 3
  for ($Try = 1; $Try -le $HealthTries; $Try += 1) {
    Write-Host "==> Full-mode health proof (try $Try of $HealthTries)"
    & $Substrate health
    if ($LASTEXITCODE -eq 0) { break }
    if ($Try -eq $HealthTries) { throw "Full-mode health proof failed (exit code $LASTEXITCODE)" }
    Start-Sleep -Seconds 10
  }
} finally {
  $env:DATABASE_URL = $SavedDatabaseUrl
  $env:ATHANOR_STATE_DIR = $SavedStateDir
}
Remove-Item -LiteralPath $Receipt -Force
Write-Host "==> preparation consumed; retained payload: $Payload"


# Only a completed previous installation grants ownership of an obsolete payload.
if (-not [string]::IsNullOrWhiteSpace([string]$Current.previousVersion)) {
  Assert-NativeReleaseProductVersion -Version $Current.previousVersion -Description "previous installed release"
  $PreviousOutput = Join-Path $Root "target/releases/$($Current.previousVersion)"

  if ($Current.previousVersion -cne $Release -and (Test-Path -LiteralPath $PreviousOutput) -and
      -not (Test-Path -LiteralPath (Join-Path $PreviousOutput "prepared-release.json"))) {
    Remove-Item -LiteralPath $PreviousOutput -Recurse -Force
  }
}

Write-Host "==> deployment complete"
Write-Host "release: $Release (previous: $($Current.previousVersion))"
Write-Host "restart OMP once before the next Athanor tool call so its transport and TypeScript tool schemas reload"
