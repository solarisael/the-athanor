#Requires -Version 7.0
[CmdletBinding()]
param(
  [switch]$SkipTests,
  [string]$Build = "",
  [switch]$PrepareOnly,
  [string]$PreparedRelease = "",
  [string]$HouseConfigFile = "",
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
if ($PSBoundParameters.ContainsKey("HouseConfigFile")) {
  if ($PrepareOnly -or -not [string]::IsNullOrWhiteSpace($LinuxHost)) {
    throw "-HouseConfigFile requires Windows activation, not preparation or Linux deployment"
  }
  $HouseConfigFile = (Resolve-Path -LiteralPath $HouseConfigFile -ErrorAction Stop).Path
  if (-not (Test-Path -LiteralPath $HouseConfigFile -PathType Leaf)) {
    throw "House configuration is not a file: $HouseConfigFile"
  }
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
$DeployClock = [System.Diagnostics.Stopwatch]::StartNew()

function Format-Elapsed {
  param([Parameter(Mandatory = $true)][TimeSpan]$Span)
  "{0:00}:{1:00}" -f [math]::Floor($Span.TotalMinutes), $Span.Seconds
}


function Invoke-Checked {
  param(
    [Parameter(Mandatory = $true)][string]$Label,
    [Parameter(Mandatory = $true)][string]$FilePath,
    [string[]]$ArgumentList = @(),
    [string]$WorkingDirectory = $Root
  )
  Write-Host "==> [$(Format-Elapsed $DeployClock.Elapsed)] $Label"

  # The child shares this console, so its own output still streams live.
  # The heartbeat only fills the silence of a long compile or test run.
  $Start = [System.Diagnostics.ProcessStartInfo]::new($FilePath)
  foreach ($Argument in $ArgumentList) { $Start.ArgumentList.Add($Argument) }
  $Start.WorkingDirectory = $WorkingDirectory
  $Start.UseShellExecute = $false

  $StepClock = [System.Diagnostics.Stopwatch]::StartNew()
  $Child = [System.Diagnostics.Process]::Start($Start)
  try {
    while (-not $Child.WaitForExit(10000)) {
      Write-Host "    ... $Label still running ($(Format-Elapsed $StepClock.Elapsed))"
    }

    if ($Child.ExitCode -ne 0) {
      throw "$Label failed (exit code $($Child.ExitCode)) after $(Format-Elapsed $StepClock.Elapsed)"
    }
  } finally {
    $Child.Dispose()
  }

  Write-Host "<== $Label done in $(Format-Elapsed $StepClock.Elapsed)"
}

# The cutover is quiet when the Host is down, every keeper process has
# acknowledged the marker (crates/omp-keeper/src/cutover.rs), and no installed
# substrate child runs. A keeper polls the marker every 200 ms. A keeper that
# never acknowledges predates the pause and would start substrate children
# across the migration, so it refuses the deployment by PID.
function Wait-CutoverQuiet {
  param(
    [Parameter(Mandatory = $true)][string]$CutoverDir,
    [Parameter(Mandatory = $true)][string]$CutoverId,
    [Parameter(Mandatory = $true)][string]$ProgramPrefix,
    [Parameter(Mandatory = $true)][int]$HostPort,
    [int]$TimeoutSeconds = 15
  )
  $Deadline = (Get-Date).AddSeconds($TimeoutSeconds)
  while ($true) {
    $Athanor = @(Get-CimInstance Win32_Process -Filter "Name = 'athanor.exe'")
    $Unidentified = @($Athanor | Where-Object { [string]::IsNullOrWhiteSpace($_.CommandLine) })
    $Keepers = @($Athanor | Where-Object { $_.CommandLine -match '^\s*("[^"]*"|\S+)\s+keeper(\s|$)' })
    $Silent = @($Keepers | Where-Object {
      $Ack = Join-Path $CutoverDir "keeper-$($_.ProcessId).json"
      $Acknowledged = $false
      if (Test-Path -LiteralPath $Ack -PathType Leaf) {
        try {
          $Acknowledged = (Get-Content -LiteralPath $Ack -Raw | ConvertFrom-Json).cutoverId -ceq $CutoverId
        } catch {
          Write-Warning "unreadable keeper acknowledgement $Ack counts as silent: $_"
        }
      }
      -not $Acknowledged
    })
    $Children = @(Get-CimInstance Win32_Process -Filter "Name = 'athanor-substrate.exe'" |
      Where-Object { $_.ExecutablePath -and [System.IO.Path]::GetFullPath($_.ExecutablePath).StartsWith($ProgramPrefix, [System.StringComparison]::OrdinalIgnoreCase) })
    $HostListener = Get-NetTCPConnection -State Listen -LocalPort $HostPort -ErrorAction SilentlyContinue
    if ($Silent.Count -eq 0 -and $Unidentified.Count -eq 0 -and $Children.Count -eq 0 -and -not $HostListener) {
      Write-Host "==> cutover $CutoverId is quiet: $($Keepers.Count) keeper(s) paused, no substrate child, Host down"
      return
    }
    if ((Get-Date) -ge $Deadline) {
      $Reasons = @()
      if ($HostListener) { $Reasons += "the Athanor Host listens on port $HostPort" }
      if ($Silent.Count) { $Reasons += "keeper PID(s) $($Silent.ProcessId -join ', ') did not acknowledge the cutover; a keeper started before the cutover pause ignores it, so quit the OMP session that keeper runs" }
      if ($Unidentified.Count) { $Reasons += "athanor.exe PID(s) $($Unidentified.ProcessId -join ', ') show no command line, so their mode is unknown" }
      if ($Children.Count) { $Reasons += "installed substrate child PID(s) $($Children.ProcessId -join ', ') still run" }
      throw "Deployment refused: $($Reasons -join '; ')."
    }
    Start-Sleep -Milliseconds 250
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
# The installed state root holds the cutover marker and receives the health
# proof's mutable state. This shell rarely carries ATHANOR_STATE_DIR; the
# installed runtime config already knows.
$StateRoot = [string]$RuntimeConfig.operatorStateRoot
if ([string]::IsNullOrWhiteSpace($StateRoot)) { throw "runtime.json has no operatorStateRoot; the cutover and the health proof cannot resolve the state root" }
$ProgramPrefix = [System.IO.Path]::GetFullPath($ProgramRoot).TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar

# --- cutover pause: OMP stays open, and nothing of the House runs ---
# The marker comes first: a Host that stops while it exists releases the
# keepers it started (crates/athanor-install/src/harness/owner.rs) instead of
# stopping them with their OMP sessions. CreateNew makes the acquisition
# exclusive, so a second deployment refuses instead of replacing this one.
$CutoverDir = Join-Path $StateRoot "cutover"
$CutoverMarker = Join-Path $CutoverDir "cutover.json"
New-Item -ItemType Directory -Force -Path $CutoverDir | Out-Null
$CutoverId = [guid]::NewGuid().ToString()
$MarkerBytes = [System.Text.UTF8Encoding]::new($false).GetBytes(
  ([ordered]@{ format = 1; cutoverId = $CutoverId; release = $Release } | ConvertTo-Json -Compress)
)
try {
  $MarkerStream = [System.IO.File]::Open($CutoverMarker, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write, [System.IO.FileShare]::Read)
} catch {
  if (-not (Test-Path -LiteralPath $CutoverMarker)) { throw }
  throw "A cutover marker already exists: $CutoverMarker. Another deployment holds it, or one ended without cleanup. Confirm that no deployment runs, then remove the marker."
}
try {
  $MarkerStream.Write($MarkerBytes, 0, $MarkerBytes.Length)
} finally {
  $MarkerStream.Dispose()
}
try {

  if (Get-NetTCPConnection -State Listen -LocalPort $RuntimeConfig.hostPort -ErrorAction SilentlyContinue) {
    Write-Warning "An older Host can close its OMP sessions when it stops. Close those sessions before stopping that Host."
    Write-Host "==> cutover $CutoverId marker is active. Stop the Athanor Host now. Cutover-aware sessions stay open. Waiting up to 10 minutes."
    $HostDeadline = (Get-Date).AddMinutes(10)
    while (Get-NetTCPConnection -State Listen -LocalPort $RuntimeConfig.hostPort -ErrorAction SilentlyContinue) {
      if ((Get-Date) -ge $HostDeadline) { throw "Deployment refused: the Athanor Host still listens on port $($RuntimeConfig.hostPort)." }
      Start-Sleep -Seconds 1
    }
  }
  Wait-CutoverQuiet -CutoverDir $CutoverDir -CutoverId $CutoverId -ProgramPrefix $ProgramPrefix -HostPort $RuntimeConfig.hostPort

  # --- install through the staged manager: it carries the new installer code
  # and is not the image the service runs, so it can retire the stable one ---
  $InstallArguments = @(
    "update", "--staging", $Payload, "--manifest", $Manifest,
    "--omp-config", $OmpConfig, "--client-config", $ClientConfig,
    "--operator-principal", $OperatorPrincipal
  )
  if ($PSBoundParameters.ContainsKey("HouseConfigFile")) {
    $InstallArguments += @("--house-config-file", $HouseConfigFile)
  }
  Invoke-Checked -Label "install release $Release" -FilePath $StagedManager -ArgumentList $InstallArguments

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
  $InstalledConfig = Get-Content -LiteralPath $RuntimePath -Raw | ConvertFrom-Json
  $InstalledStateRoot = [string]$InstalledConfig.operatorStateRoot
  # The substrate refuses to guess where mutable state lives.
  $SavedDatabaseUrl = $env:DATABASE_URL
  $SavedStateDir = $env:ATHANOR_STATE_DIR
  try {
    $env:DATABASE_URL = [string]$Secrets.externalDatabaseUrl
    $env:ATHANOR_STATE_DIR = $InstalledStateRoot
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
} finally {
  # Keepers resume and OMP sessions may reconnect only once the marker is gone.
  # Only the deployment that owns this cutover id removes the marker.
  try {
    $Held = Get-Content -LiteralPath $CutoverMarker -Raw | ConvertFrom-Json
    if ([string]$Held.cutoverId -cne $CutoverId) {
      Write-Warning "Cutover marker $CutoverMarker names $($Held.cutoverId), not $CutoverId; it stays in place."
    } else {
      # Keep the marker while clearing acknowledgements so a later cutover cannot lose its own.
      Get-ChildItem -LiteralPath $CutoverDir -Filter "keeper-*" | Remove-Item -Force
      Remove-Item -LiteralPath $CutoverMarker -Force
    }
  } catch {
    Write-Warning "Cutover $CutoverId could not be released: $_. Keepers and OMP sessions stay paused until $CutoverMarker is removed."
  }
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

Write-Host "==> deployment complete in $(Format-Elapsed $DeployClock.Elapsed)"
Write-Host "release: $Release (previous: $($Current.previousVersion))"
Write-Host "start the Host; open OMP sessions reconnect on their own when the installed adapter is compatible, and otherwise name the OMP restart they need"
Write-Host "restart OMP to load the new adapter code and tool schemas; a reconnected session keeps the code it loaded"
