#Requires -Version 7.0
[CmdletBinding()]
param(
    [switch]$SkipTests,
    [switch]$SkipBackup,
    [ValidateRange(1, 365)]
    [int]$BackupKeep = 14,
    [string]$Cargo = "cargo"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $IsWindows) {
    throw "deploy-local.ps1 supports the canonical Windows + WSL deployment path only"
}

$substrateRoot = [IO.Path]::GetFullPath($PSScriptRoot)
$athanorRoot = [IO.Path]::GetFullPath((Join-Path $substrateRoot ".."))
# Mutable state lives outside the immutable product tree. ATHANOR_STATE_DIR
# wins, then an adjacent installed state root, then the installed runtime
# contract, then a development checkout.
$runtimeConfigPath = Join-Path ([Environment]::GetFolderPath("CommonApplicationData")) "Solarisael\Athanor\config\runtime.json"
$operatorClientProjectionPath = Join-Path ([Environment]::GetFolderPath("UserProfile")) ".omp\agent\athanor\client.json"
$configuredStateRoot = [string]$env:ATHANOR_STATE_DIR
$installedStateRoot = Join-Path ([IO.Path]::GetDirectoryName($athanorRoot)) "state"
$runtimeStateRoot = if (Test-Path $runtimeConfigPath -PathType Leaf) {
    $runtime = Get-Content $runtimeConfigPath -Raw | ConvertFrom-Json
    $candidate = [string]$runtime.operatorStateRoot
    if (-not [string]::IsNullOrWhiteSpace($candidate) -and
        [IO.Path]::IsPathRooted($candidate) -and
        (Test-Path $candidate -PathType Container)) {
        $candidate
    }
}
$stateRoot = if (-not [string]::IsNullOrWhiteSpace($configuredStateRoot)) {
    if (-not [IO.Path]::IsPathRooted($configuredStateRoot)) {
        throw "ATHANOR_STATE_DIR must be an absolute path (got $configuredStateRoot)"
    }
    $configuredStateRoot
} elseif (Test-Path $installedStateRoot -PathType Container) {
    $installedStateRoot
} elseif (-not [string]::IsNullOrWhiteSpace($runtimeStateRoot)) {
    $runtimeStateRoot
} elseif ((Test-Path (Join-Path $athanorRoot "Cargo.toml") -PathType Leaf) -and
          (Test-Path (Join-Path $athanorRoot "crates") -PathType Container)) {
    Join-Path $athanorRoot "state"
} else {
    throw "ATHANOR_STATE_DIR is not set and no state root could be resolved; set it to the absolute path of <install-root>/state"
}
$env:ATHANOR_STATE_DIR = [IO.Path]::GetFullPath($stateRoot)
$substrateStateDir = [IO.Path]::GetFullPath((Join-Path $stateRoot "substrate"))
$stageTarget = Join-Path $athanorRoot "target\deploy"
$releaseDependenciesPath = Join-Path $athanorRoot "installer\dependencies.json"
$adapterDeployPath = Join-Path $athanorRoot "adapters\omp\deploy-local.ps1"
# The loader is product-owned payload, not adapter component content: one source
# feeds both the active version's bin copy and the stable Program Files copy.
$sourceOmpLoader = Join-Path $athanorRoot "adapters\omp\installed-loader.ts"
 $stagedExe = Join-Path $stageTarget "release\athanor-substrate.exe"
 $stagedPdb = [IO.Path]::ChangeExtension($stagedExe, ".pdb")
 $stagedManagerExe = Join-Path $stageTarget "release\athanor-manage.exe"
 $stagedAppExe = Join-Path $stageTarget "release\athanor.exe"
 $stagedKeeperExe = Join-Path $stageTarget "release\omp-keeper.exe"
 $stagedKeeperPdb = [IO.Path]::ChangeExtension($stagedKeeperExe, ".pdb")
$configuredLiveExe = [string]$env:ATHANOR_SUBSTRATE_EXE
$programRoot = Join-Path $env:ProgramFiles "Solarisael\Athanor"
$currentPointer = Join-Path $programRoot "current.json"
$installedLiveExe = if (Test-Path $currentPointer -PathType Leaf) {
    $current = Get-Content $currentPointer -Raw | ConvertFrom-Json
    $version = [string]$current.version
    if ($version -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]*$') {
        throw "installed Athanor current.json carries an invalid version"
    }
    $candidate = Join-Path $programRoot "versions\$version\bin\athanor-substrate.exe"
    if (-not (Test-Path $candidate -PathType Leaf)) {
        throw "installed Athanor current version is missing its substrate executable at $candidate"
    }
    $candidate
}
$liveExe = if (-not [string]::IsNullOrWhiteSpace($configuredLiveExe)) {
    if ([IO.Path]::IsPathRooted($configuredLiveExe)) {
        $configuredLiveExe
    } else {
        Join-Path $athanorRoot $configuredLiveExe
    }
} elseif (-not [string]::IsNullOrWhiteSpace($installedLiveExe)) {
    $installedLiveExe
} else {
    Join-Path $athanorRoot "target\release\athanor-substrate.exe"
}
$liveExe = [IO.Path]::GetFullPath($liveExe)
$livePdb = [IO.Path]::ChangeExtension($liveExe, ".pdb")
 $liveManagerExe = Join-Path ([IO.Path]::GetDirectoryName($liveExe)) "athanor-manage.exe"
 $liveAppExe = Join-Path ([IO.Path]::GetDirectoryName($liveExe)) "athanor.exe"
 $liveKeeperExe = Join-Path ([IO.Path]::GetDirectoryName($liveExe)) "omp-keeper.exe"
 $liveKeeperPdb = [IO.Path]::ChangeExtension($liveKeeperExe, ".pdb")
 $liveOmpLoader = Join-Path ([IO.Path]::GetDirectoryName($liveExe)) "athanor-omp-loader.ts"
 $stableKeeperExe = Join-Path ([IO.Path]::GetDirectoryName([IO.Path]::GetDirectoryName([IO.Path]::GetDirectoryName([IO.Path]::GetDirectoryName($liveExe))))) "bin\omp-keeper.exe"
 $sourceKeeperProvision = Join-Path $athanorRoot "crates\omp-keeper\scripts\provision-local.ps1"
 $sourceRestartProvision = Join-Path $athanorRoot "substrate\provision-restart-capability.ps1"
 $stableKeeperProvision = Join-Path ([IO.Path]::GetDirectoryName($stableKeeperExe)) "provision-omp-keeper.ps1"
 $stableRestartProvision = Join-Path ([IO.Path]::GetDirectoryName($stableKeeperExe)) "provision-restart-capability.ps1"
 $stableOmpLoader = Join-Path ([IO.Path]::GetDirectoryName($stableKeeperExe)) "athanor-omp-loader.ts"
$stableManagerExe = Join-Path ([IO.Path]::GetDirectoryName([IO.Path]::GetDirectoryName([IO.Path]::GetDirectoryName([IO.Path]::GetDirectoryName($liveExe))))) "bin\athanor-manage.exe"
$stableAppExe = Join-Path ([IO.Path]::GetDirectoryName([IO.Path]::GetDirectoryName([IO.Path]::GetDirectoryName([IO.Path]::GetDirectoryName($liveExe))))) "bin\athanor.exe"
 $previousExe = Join-Path $stageTarget "previous\athanor-substrate.exe"
 $previousPdb = [IO.Path]::ChangeExtension($previousExe, ".pdb")
 $previousManagerExe = Join-Path $stageTarget "previous\athanor-manage.exe"
 $previousStableManagerExe = Join-Path $stageTarget "previous\athanor-manage-stable.exe"
 $previousAppExe = Join-Path $stageTarget "previous\athanor.exe"
 $previousStableAppExe = Join-Path $stageTarget "previous\athanor-stable.exe"
 $previousKeeperExe = Join-Path $stageTarget "previous\omp-keeper.exe"
 $previousKeeperPdb = Join-Path $stageTarget "previous\omp-keeper.pdb"
 $previousStableKeeperExe = Join-Path $stageTarget "previous\omp-keeper-stable.exe"
 $previousKeeperProvision = Join-Path $stageTarget "previous\provision-omp-keeper.ps1"
 $previousRestartProvision = Join-Path $stageTarget "previous\provision-restart-capability.ps1"
 $previousOmpLoader = Join-Path $stageTarget "previous\athanor-omp-loader.ts"
 $previousStableOmpLoader = Join-Path $stageTarget "previous\athanor-omp-loader-stable.ts"
 $liveManifest = Join-Path ([IO.Path]::GetDirectoryName([IO.Path]::GetDirectoryName($liveExe))) "release-manifest.json"
 $previousManifest = Join-Path $stageTarget "previous\release-manifest.json"
$nativeServiceName = "SolarisaelAthanor"
$restartNativeService = $false


function Invoke-Checked {
    param(
        [Parameter(Mandatory)] [string]$Label,
        [Parameter(Mandatory)] [string]$FilePath,
        [Parameter(Mandatory)] [string[]]$ArgumentList
    )

    Write-Host "==> $Label"
    & $FilePath @ArgumentList
    if ($LASTEXITCODE -ne 0) {
        throw "$Label failed with exit code $LASTEXITCODE"
    }
}

function Update-OperatorClientProjection {
    param(
        [Parameter(Mandatory)] [string]$RuntimeConfigPath,
        [Parameter(Mandatory)] [string]$ClientProjectionPath
    )

    if (-not (Test-Path $RuntimeConfigPath -PathType Leaf)) {
        throw "installed runtime configuration is missing at $RuntimeConfigPath"
    }
    if (-not (Test-Path $ClientProjectionPath -PathType Leaf)) {
        throw "operator client projection is missing at $ClientProjectionPath"
    }

    $runtime = Get-Content $RuntimeConfigPath -Raw | ConvertFrom-Json
    $client = Get-Content $ClientProjectionPath -Raw | ConvertFrom-Json
    $houseId = [string]$runtime.houseId
    $stateRoot = [string]$runtime.operatorStateRoot
    $defaultRoom = [string]$runtime.defaultRoom
    $hostToken = [string]$client.hostToken
    $hostPort = [int]$runtime.hostPort
    if ([string]::IsNullOrWhiteSpace($houseId) -or
        [string]::IsNullOrWhiteSpace($hostToken) -or
        -not [IO.Path]::IsPathRooted($stateRoot) -or
        -not (Test-Path $stateRoot -PathType Container) -or
        $hostPort -lt 1 -or $hostPort -gt 65535 -or
        $defaultRoom -eq "house" -or
        $defaultRoom -notmatch '^[a-z0-9]+(?:-[a-z0-9]+)*$') {
        throw "installed runtime configuration or operator client projection has unsafe required fields"
    }

    $rooms = [ordered]@{}
    foreach ($entry in @($runtime.rooms)) {
        $room = [string]$entry.room
        $spirit = [string]$entry.spirit
        if ($room -eq "house" -or
            $room -notmatch '^[a-z0-9]+(?:-[a-z0-9]+)*$' -or
            [string]::IsNullOrWhiteSpace($spirit) -or
            $rooms.Contains($room)) {
            throw "installed runtime configuration has invalid room identities"
        }
        $rooms[$room] = [ordered]@{ spirit = $spirit }
    }
    if ($rooms.Count -eq 0 -or -not $rooms.Contains($defaultRoom)) {
        throw "installed runtime configuration has no default room identity"
    }

    $directory = [IO.Path]::GetDirectoryName($ClientProjectionPath)
    if (-not (Test-Path $directory -PathType Container)) {
        throw "operator client projection directory is missing at $directory"
    }
    $temporary = Join-Path $directory ".$([IO.Path]::GetFileName($ClientProjectionPath)).$([Guid]::NewGuid().ToString('N')).tmp"
    $backup = Join-Path $directory ".$([IO.Path]::GetFileName($ClientProjectionPath)).$([Guid]::NewGuid().ToString('N')).bak"
    $projection = [ordered]@{
        format = 2
        houseId = $houseId
        hostToken = $hostToken
        stateRoot = [IO.Path]::GetFullPath($stateRoot)
        hostUrl = "ws://127.0.0.1:$hostPort"
        defaultRoom = $defaultRoom
        rooms = $rooms
    }
    try {
        [IO.File]::WriteAllText($temporary, ($projection | ConvertTo-Json -Depth 5), [Text.UTF8Encoding]::new($false))
        [IO.File]::Replace($temporary, $ClientProjectionPath, $backup)
    } finally {
        if (Test-Path $temporary -PathType Leaf) {
            Remove-Item $temporary -Force -ErrorAction SilentlyContinue
        }
    }
    [pscustomobject]@{ Live = $ClientProjectionPath; Previous = $backup; Created = $false }
}

function Import-DeploymentDatabaseEnvironment {
    param([Parameter(Mandatory)] [string]$SubstrateStateDir)

    $requiredPg = @("PGHOST", "PGPORT", "PGDATABASE", "PGUSER")
    $hasDatabase = {
        -not [string]::IsNullOrWhiteSpace(
            [Environment]::GetEnvironmentVariable("DATABASE_URL", "Process")
        ) -or -not ($requiredPg | Where-Object {
            [string]::IsNullOrWhiteSpace(
                [Environment]::GetEnvironmentVariable($_, "Process")
            )
        })
    }
    if (& $hasDatabase) {
        return
    }

    $dotenv = Join-Path $SubstrateStateDir ".env"
    if (-not (Test-Path $dotenv -PathType Leaf)) {
        throw "database configuration is absent and the substrate environment file is missing at $dotenv"
    }
    $allowed = @("DATABASE_URL", "PGHOST", "PGPORT", "PGDATABASE", "PGUSER", "PGPASSWORD")
    foreach ($line in Get-Content $dotenv) {
        if ($line -notmatch '^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)\s*$') {
            continue
        }
        $name = $Matches[1]
        if ($name -notin $allowed -or -not [string]::IsNullOrWhiteSpace(
            [Environment]::GetEnvironmentVariable($name, "Process")
        )) {
            continue
        }
        $value = $Matches[2].Trim()
        if ($value.Length -ge 2 -and (
            ($value.StartsWith('"') -and $value.EndsWith('"')) -or
            ($value.StartsWith("'") -and $value.EndsWith("'"))
        )) {
            $value = $value.Substring(1, $value.Length - 2)
        }
        [Environment]::SetEnvironmentVariable($name, $value, "Process")
    }
    if (-not (& $hasDatabase)) {
        throw "database configuration in $dotenv does not provide DATABASE_URL or complete PG* variables"
    }
}



function Get-LiveWorkers {
    param([Parameter(Mandatory)] [string]$ExecutablePath)

    $target = [IO.Path]::GetFullPath($ExecutablePath)
    Get-CimInstance Win32_Process | Where-Object {
        if ([string]::IsNullOrWhiteSpace([string]$_.ExecutablePath)) {
            return $false
        }
        [StringComparer]::OrdinalIgnoreCase.Equals(
            [IO.Path]::GetFullPath([string]$_.ExecutablePath),
            $target
        )
    }
}

function Start-NativeService {
    param([Parameter(Mandatory)] [string]$ServiceName)

    Write-Host "==> starting native Athanor service"
    $service = Get-Service -Name $ServiceName -ErrorAction Stop
    if ($service.Status -ne [System.ServiceProcess.ServiceControllerStatus]::Running) {
        Start-Service -Name $ServiceName -ErrorAction Stop
        $service.WaitForStatus(
            [System.ServiceProcess.ServiceControllerStatus]::Running,
            [TimeSpan]::FromSeconds(30)
        )
    }
}
# [runtime/deploy/app] [host/routing] [proof/live]
function Start-AthanorAppAndWaitForRooms {
    param(
        [Parameter(Mandatory)] [string]$AppPath,
        [Parameter(Mandatory)] [string]$RuntimeConfigPath
    )

    $runtime = Get-Content $RuntimeConfigPath -Raw | ConvertFrom-Json
    $hostPort = [int]$runtime.hostPort
    $roomNames = @($runtime.rooms | ForEach-Object { [string]$_.room })
    if ($hostPort -le 0 -or $roomNames.Count -eq 0) {
        throw "native runtime configuration has no Host listener or rooms"
    }

    $app = Start-Process -FilePath $AppPath -PassThru
    try {
        $deadline = [DateTime]::UtcNow.AddSeconds(30)
        do {
            if ($app.HasExited) {
                throw "athanor.exe exited with code $($app.ExitCode) before Host readiness"
            }
            $pending = @()
            foreach ($roomName in $roomNames) {
                try {
                    $path = "/room/$roomName/athanor/v1/ws"
                    $response = Invoke-WebRequest `
                        -Uri "http://127.0.0.1:$hostPort/room/$roomName/health" `
                        -SkipHttpErrorCheck `
                        -TimeoutSec 2
                    $health = $response.Content | ConvertFrom-Json
                    if ($response.StatusCode -ne 200 -or
                        [string]$health.status -ne "ok" -or
                        [string]$health.websocket_path -ne $path) {
                        $pending += $roomName
                    }
                } catch {
                    $pending += $roomName
                }
            }
            if ($pending.Count -eq 0) {
                try {
                    $root = Invoke-WebRequest `
                        -Uri "http://127.0.0.1:$hostPort/health" `
                        -SkipHttpErrorCheck `
                        -TimeoutSec 2
                    $unknown = Invoke-WebRequest `
                        -Uri "http://127.0.0.1:$hostPort/room/__missing__/health" `
                        -SkipHttpErrorCheck `
                        -TimeoutSec 2
                    if ($root.StatusCode -eq 404 -and $unknown.StatusCode -eq 404) {
                        Write-Host "==> athanor.exe PID $($app.Id): listener :$hostPort, $($roomNames.Count) room paths healthy"
                        return $app
                    }
                } catch {}
            }
            Start-Sleep -Milliseconds 250
        } while ([DateTime]::UtcNow -lt $deadline)

        throw "athanor.exe Host paths did not become ready within 30 seconds"
    } catch {
        if (-not $app.HasExited) {
            Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
            $app.WaitForExit()
        }
        throw
    }
}

if (-not (Test-Path (Join-Path $athanorRoot "Cargo.toml") -PathType Leaf)) {
    throw "The Athanor workspace Cargo.toml is missing at $athanorRoot"
}
if (-not (Test-Path (Join-Path $athanorRoot "crates\akasha\Cargo.toml") -PathType Leaf)) {
    throw "substrate crate Cargo.toml is missing at $athanorRoot\crates\akasha"
}

New-Item -ItemType Directory -Force -Path $stageTarget | Out-Null
$nativeService = Get-Service -Name $nativeServiceName -ErrorAction SilentlyContinue
if ($null -ne $nativeService -and
    $nativeService.Status -ne [System.ServiceProcess.ServiceControllerStatus]::Running -and
    $nativeService.Status -ne [System.ServiceProcess.ServiceControllerStatus]::Stopped) {
    throw "native Athanor service is $($nativeService.Status); recover it to Running or Stopped before deployment"
}
if (Test-Path $liveManifest -PathType Leaf) {
    $liveVersionsRoot = [IO.Path]::GetDirectoryName([IO.Path]::GetDirectoryName($liveManifest))
    if (-not [StringComparer]::OrdinalIgnoreCase.Equals([IO.Path]::GetFileName($liveVersionsRoot), "versions") -or
        -not (Test-Path $liveManagerExe -PathType Leaf) -or
        -not (Test-Path $stableManagerExe -PathType Leaf)) {
        throw "installed manager paths are missing or ambiguous; recover both version and stable manager paths before deployment"
    }
}

$keeperWorkers = @()
foreach ($keeperPath in @($liveKeeperExe, $stableKeeperExe) | Select-Object -Unique) {
    $keeperWorkers += @(Get-LiveWorkers -ExecutablePath $keeperPath)
}
if ($keeperWorkers.Count -gt 0) {
    $keeperSummary = ($keeperWorkers | ForEach-Object { "PID=$($_.ProcessId) parent=$($_.ParentProcessId) path=$($_.ExecutablePath)" }) -join ", "
    throw "an installed omp-keeper is running ($keeperSummary); exit its child OMP and keeper before deployment so the terminal owner is never replaced underneath a live session"
}

# `athanor.exe` owns Host and control state; replacing it under a live process
# would split the installed identity from the running one.
$appWorkers = @()
foreach ($appPath in @($liveAppExe, $stableAppExe) | Select-Object -Unique) {
    $appWorkers += @(Get-LiveWorkers -ExecutablePath $appPath)
}
if ($appWorkers.Count -gt 0) {
    $appSummary = ($appWorkers | ForEach-Object { "PID=$($_.ProcessId) parent=$($_.ParentProcessId) path=$($_.ExecutablePath)" }) -join ", "
    throw "an installed athanor.exe is running ($appSummary); exit the app before deployment so its executable is never replaced underneath a live session"
}



if (-not $SkipTests) {
    Invoke-Checked -Label "Athanor Hearth, protocol, substrate, and Host tests" -FilePath $Cargo -ArgumentList @(
        "test", "--manifest-path", (Join-Path $athanorRoot "Cargo.toml"),
        "-p", "hearth", "-p", "protocol", "-p", "akasha", "-p", "host", "-p", "athanor-install", "-p", "omp-keeper", "--release", "--target-dir", $stageTarget
    )
}

Invoke-Checked -Label "staged release build" -FilePath $Cargo -ArgumentList @(
    "build", "--manifest-path", (Join-Path $athanorRoot "Cargo.toml"),
    "-p", "hearth", "-p", "protocol", "-p", "akasha", "-p", "host", "-p", "athanor-install", "-p", "omp-keeper", "--release", "--target-dir", $stageTarget
)
if (-not (Test-Path $stagedExe -PathType Leaf)) {
    throw "staged executable was not produced at $stagedExe"
}
if (-not (Test-Path $stagedManagerExe -PathType Leaf)) {
    throw "staged manager executable was not produced at $stagedManagerExe"
}
if (-not (Test-Path $stagedKeeperExe -PathType Leaf)) {
    throw "staged keeper executable was not produced at $stagedKeeperExe"
}
if (-not (Test-Path $stagedAppExe -PathType Leaf)) {
    throw "staged app executable was not produced at $stagedAppExe"
}
if (-not (Test-Path $sourceOmpLoader -PathType Leaf)) {
    throw "product-owned OMP loader source is missing at $sourceOmpLoader"
}
foreach ($provisioner in @($sourceKeeperProvision, $sourceRestartProvision)) {
    if (-not (Test-Path $provisioner -PathType Leaf)) {
        throw "keeper provisioner is missing: $provisioner"
    }
}

Import-DeploymentDatabaseEnvironment -SubstrateStateDir $substrateStateDir

if (-not $SkipBackup) {
    $priorPgWsl = $env:ATHANOR_PG_WSL
    try {
        $env:ATHANOR_PG_WSL = "1"
        # The idempotent backup gets one bounded retry for transient WSL startup failure.
        try {
            Invoke-Checked -Label "pre-deploy PostgreSQL backup" -FilePath $stagedExe -ArgumentList @(
                "backup", "--output-dir", (Join-Path $substrateStateDir "backups"),
                "--keep", [string]$BackupKeep
            )
        } catch {
            Write-Warning "pre-deploy backup failed once ($($_.Exception.Message)); retrying after 5 seconds"
            Start-Sleep -Seconds 5
            Invoke-Checked -Label "pre-deploy PostgreSQL backup (retry)" -FilePath $stagedExe -ArgumentList @(
                "backup", "--output-dir", (Join-Path $substrateStateDir "backups"),
                "--keep", [string]$BackupKeep
            )
        }
    } finally {
        if ($null -eq $priorPgWsl) {
            Remove-Item Env:ATHANOR_PG_WSL -ErrorAction SilentlyContinue
        } else {
            $env:ATHANOR_PG_WSL = $priorPgWsl
        }
    }
}

$nativeService = Get-Service -Name $nativeServiceName -ErrorAction SilentlyContinue
if ($null -ne $nativeService -and
    $nativeService.Status -ne [System.ServiceProcess.ServiceControllerStatus]::Running -and
    $nativeService.Status -ne [System.ServiceProcess.ServiceControllerStatus]::Stopped) {
    throw "native Athanor service is $($nativeService.Status); recover it to Running or Stopped before deployment"
}

if ($null -ne $nativeService -and
    $nativeService.Status -eq [System.ServiceProcess.ServiceControllerStatus]::Running) {
    Write-Host "==> stopping native Athanor service"
    Stop-Service -Name $nativeServiceName -Force -ErrorAction Stop
    $nativeService.WaitForStatus(
        [System.ServiceProcess.ServiceControllerStatus]::Stopped,
        [TimeSpan]::FromSeconds(30)
    )
    $restartNativeService = $true
}

foreach ($workerPath in @($liveExe)) {
    $workers = @(Get-LiveWorkers -ExecutablePath $workerPath)
    if ($workers.Count -eq 0) {
        continue
    }
    $workerName = [IO.Path]::GetFileName($workerPath)
    $workerSummary = ($workers | ForEach-Object { "PID=$($_.ProcessId) parent=$($_.ParentProcessId)" }) -join ", "
    Write-Host "==> stopping exact-path $workerName workers: $workerSummary"
    foreach ($worker in $workers) {
        Stop-Process -Id $worker.ProcessId -Force -ErrorAction Stop
    }

    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        $remainingWorkers = @(Get-LiveWorkers -ExecutablePath $workerPath)
        if ($remainingWorkers.Count -eq 0) {
            break
        }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)

    if ($remainingWorkers.Count -gt 0) {
        throw "$workerName workers did not stop within 10 seconds"
    }
}

$startedApp = $null
$clientProjection = $null
try {
    $copiedExe = $false
    $copiedPdb = $false
    $copiedManager = $false
    $copiedStableManager = $false
    $copiedAppExe = $false
    $copiedStableApp = $false
    $copiedKeeperExe = $false
    $copiedKeeperPdb = $false
    $copiedStableKeeper = $false
    $copiedKeeperProvision = $false
    $copiedRestartProvision = $false
    $copiedOmpLoader = $false
    $copiedStableOmpLoader = $false
New-Item -ItemType Directory -Force -Path ([IO.Path]::GetDirectoryName($liveExe)) | Out-Null
New-Item -ItemType Directory -Force -Path ([IO.Path]::GetDirectoryName($previousExe)) | Out-Null
foreach ($priorPath in @($previousExe, $previousPdb, $previousManagerExe, $previousStableManagerExe, $previousAppExe, $previousStableAppExe, $previousKeeperExe, $previousKeeperPdb, $previousStableKeeperExe, $previousKeeperProvision, $previousRestartProvision, $previousOmpLoader, $previousStableOmpLoader, $previousManifest)) {
    if (Test-Path $priorPath -PathType Leaf) {
        Remove-Item $priorPath -Force -ErrorAction Stop
    }
}
if (Test-Path $liveExe -PathType Leaf) {
    Move-Item $liveExe $previousExe -Force
}
if (Test-Path $livePdb -PathType Leaf) {
    Move-Item $livePdb $previousPdb -Force
}
if (Test-Path $liveKeeperExe -PathType Leaf) {
    Move-Item $liveKeeperExe $previousKeeperExe -Force
}
if (Test-Path $liveKeeperPdb -PathType Leaf) {
    Move-Item $liveKeeperPdb $previousKeeperPdb -Force
}
if (Test-Path $liveManifest -PathType Leaf) {
    if (Test-Path $stableKeeperExe -PathType Leaf) {
        Move-Item $stableKeeperExe $previousStableKeeperExe -Force
    }
    if (Test-Path $stableKeeperProvision -PathType Leaf) {
        Move-Item $stableKeeperProvision $previousKeeperProvision -Force
    }
    if (Test-Path $stableRestartProvision -PathType Leaf) {
        Move-Item $stableRestartProvision $previousRestartProvision -Force
    }
    if (Test-Path $liveOmpLoader -PathType Leaf) {
        Move-Item $liveOmpLoader $previousOmpLoader -Force
    }
    if (Test-Path $stableOmpLoader -PathType Leaf) {
        Move-Item $stableOmpLoader $previousStableOmpLoader -Force
    }
    if (-not (Test-Path $liveManagerExe -PathType Leaf) -or -not (Test-Path $stableManagerExe -PathType Leaf)) {
        throw "installed manager paths are missing or ambiguous; recover both version and stable manager paths before deployment"
    }
    Move-Item $liveManagerExe $previousManagerExe -Force
    Move-Item $stableManagerExe $previousStableManagerExe -Force
    if (Test-Path $liveAppExe -PathType Leaf) {
        Move-Item $liveAppExe $previousAppExe -Force
    }
    if (Test-Path $stableAppExe -PathType Leaf) {
        Move-Item $stableAppExe $previousStableAppExe -Force
    }
}
if (Test-Path $liveManifest -PathType Leaf) {
    Copy-Item $liveManifest $previousManifest -Force
}

    Copy-Item $stagedExe $liveExe -Force
    $copiedExe = $true
    if (Test-Path $stagedPdb -PathType Leaf) {
        Copy-Item $stagedPdb $livePdb -Force
        $copiedPdb = $true
    }
    Copy-Item $stagedKeeperExe $liveKeeperExe -Force
    $copiedKeeperExe = $true
    if (Test-Path $stagedKeeperPdb -PathType Leaf) {
        Copy-Item $stagedKeeperPdb $liveKeeperPdb -Force
        $copiedKeeperPdb = $true
    }
    if (Test-Path $liveManifest -PathType Leaf) {
        Copy-Item $stagedManagerExe $liveManagerExe -Force
        $copiedManager = $true
        Copy-Item $stagedManagerExe $stableManagerExe -Force
        $copiedStableManager = $true
        Copy-Item $stagedAppExe $liveAppExe -Force
        $copiedAppExe = $true
        Copy-Item $stagedAppExe $stableAppExe -Force
        $copiedStableApp = $true
        Copy-Item $stagedKeeperExe $stableKeeperExe -Force
        $copiedStableKeeper = $true
        Copy-Item $sourceKeeperProvision $stableKeeperProvision -Force
        $copiedKeeperProvision = $true
        Copy-Item $sourceRestartProvision $stableRestartProvision -Force
        $copiedRestartProvision = $true
        Copy-Item $sourceOmpLoader $liveOmpLoader -Force
        $copiedOmpLoader = $true
        Copy-Item $sourceOmpLoader $stableOmpLoader -Force
        $copiedStableOmpLoader = $true
    }
    if (Test-Path $liveManifest -PathType Leaf) {
        $manifest = Get-Content $liveManifest -Raw | ConvertFrom-Json
        $releaseDependencies = Get-Content $releaseDependenciesPath -Raw | ConvertFrom-Json
        $manifest.schemaVersion = [int]$releaseDependencies.schemaVersion
        $keeperEntries = @($manifest.artifacts | Where-Object { [string]$_.path -eq "bin/omp-keeper.exe" })
        if ($keeperEntries.Count -eq 0) {
            $manifest.artifacts = @($manifest.artifacts) + [pscustomobject][ordered]@{
                component = "omp-keeper"
                path = "bin/omp-keeper.exe"
                sha256 = ""
                size = 0
                executable = $true
            }
        }
        $appEntries = @($manifest.artifacts | Where-Object { [string]$_.path -eq "bin/athanor.exe" })
        if ($appEntries.Count -eq 0) {
            $manifest.artifacts = @($manifest.artifacts) + [pscustomobject][ordered]@{
                component = "app"
                path = "bin/athanor.exe"
                sha256 = ""
                size = 0
                executable = $true
            }
        }
        foreach ($binary in @(
            @{ Path = "bin/athanor-substrate.exe"; Source = $liveExe },
            @{ Path = "bin/athanor-manage.exe"; Source = $liveManagerExe },
            @{ Path = "bin/omp-keeper.exe"; Source = $liveKeeperExe },
            @{ Path = "bin/athanor.exe"; Source = $liveAppExe },
            @{ Path = "bin/athanor-omp-loader.ts"; Source = $liveOmpLoader }
        )) {
            $entries = @($manifest.artifacts | Where-Object { [string]$_.path -eq $binary.Path })
            if ($entries.Count -ne 1) {
                throw "release manifest must contain exactly one $($binary.Path) artifact"
            }
            $entries[0].sha256 = (Get-FileHash $binary.Source -Algorithm SHA256).Hash.ToLowerInvariant()
            $entries[0].size = (Get-Item $binary.Source).Length
        }
        $manifestTemporary = "$liveManifest.new-$PID"
        $manifest | ConvertTo-Json -Depth 8 | Set-Content $manifestTemporary -Encoding utf8NoBOM
        Move-Item $manifestTemporary $liveManifest -Force
    }
    Invoke-Checked -Label "ordered database migrations" -FilePath $liveExe -ArgumentList @(
        "migrations"
    )
    Invoke-Checked -Label "semantic vocabulary refresh" -FilePath $liveExe -ArgumentList @(
        "semantic-vocabulary-refresh"
    )
    if ($restartNativeService) {
        Start-NativeService -ServiceName $nativeServiceName
    }
    Invoke-Checked -Label "OMP adapter deployment" -FilePath "pwsh" -ArgumentList @(
        "-NoProfile", "-File", $adapterDeployPath
    )
    $clientProjection = Update-OperatorClientProjection `
        -RuntimeConfigPath $runtimeConfigPath `
        -ClientProjectionPath $operatorClientProjectionPath
    Invoke-Checked -Label "Full-mode health proof" -FilePath $liveExe -ArgumentList @(
        "health", "--substrate-dir", $substrateRoot
    )
    if (Test-Path $liveManifest -PathType Leaf) {
        Invoke-Checked -Label "native release manifest proof" -FilePath $liveManagerExe -ArgumentList @(
            "doctor"
        )
    }
    $startedApp = Start-AthanorAppAndWaitForRooms `
        -AppPath $stableAppExe `
        -RuntimeConfigPath $runtimeConfigPath
} catch {
    if ($null -ne $startedApp -and -not $startedApp.HasExited) {
        Stop-Process -Id $startedApp.Id -Force -ErrorAction SilentlyContinue
        $startedApp.WaitForExit()
    }
    $deploymentFailure = $_
    $restoreFailures = @()
    $rollbackServiceStopped = $true
    try {
        $rollbackService = Get-Service -Name $nativeServiceName -ErrorAction SilentlyContinue
        if ($null -ne $rollbackService -and
            $rollbackService.Status -ne [System.ServiceProcess.ServiceControllerStatus]::Stopped) {
            Stop-Service -Name $nativeServiceName -Force -ErrorAction Stop
            $rollbackService.WaitForStatus(
                [System.ServiceProcess.ServiceControllerStatus]::Stopped,
                [TimeSpan]::FromSeconds(30)
            )
        }
    } catch {
        $rollbackServiceStopped = $false
        $restoreFailures += "native service stop before rollback failed: $($_.Exception.Message)"
    }
    if ($rollbackServiceStopped) {
    foreach ($artifact in @(
        @{ Live = $liveExe; Previous = $previousExe; Created = $copiedExe },
        @{ Live = $livePdb; Previous = $previousPdb; Created = $copiedPdb },
        @{ Live = $liveManagerExe; Previous = $previousManagerExe; Created = $copiedManager },
        @{ Live = $stableManagerExe; Previous = $previousStableManagerExe; Created = $copiedStableManager },
        @{ Live = $liveAppExe; Previous = $previousAppExe; Created = $copiedAppExe },
        @{ Live = $stableAppExe; Previous = $previousStableAppExe; Created = $copiedStableApp },
        @{ Live = $liveKeeperExe; Previous = $previousKeeperExe; Created = $copiedKeeperExe },
        @{ Live = $liveKeeperPdb; Previous = $previousKeeperPdb; Created = $copiedKeeperPdb },
        @{ Live = $stableKeeperExe; Previous = $previousStableKeeperExe; Created = $copiedStableKeeper },
        @{ Live = $stableKeeperProvision; Previous = $previousKeeperProvision; Created = $copiedKeeperProvision },
        @{ Live = $stableRestartProvision; Previous = $previousRestartProvision; Created = $copiedRestartProvision },
        @{ Live = $liveOmpLoader; Previous = $previousOmpLoader; Created = $copiedOmpLoader },
        @{ Live = $stableOmpLoader; Previous = $previousStableOmpLoader; Created = $copiedStableOmpLoader },
        @{ Live = $liveManifest; Previous = $previousManifest; Created = $false }
        if ($null -ne $clientProjection) {
            $clientProjection
        }
    )) {
        try {
            if (Test-Path $artifact.Previous -PathType Leaf) {
                if (Test-Path $artifact.Live -PathType Leaf) {
                    Remove-Item $artifact.Live -Force -ErrorAction Stop
                }
                Move-Item $artifact.Previous $artifact.Live -Force
            } elseif ($artifact.Created -and (Test-Path $artifact.Live -PathType Leaf)) {
                Remove-Item $artifact.Live -Force -ErrorAction Stop
            }
        } catch {
            $restoreFailures += $_.Exception.Message
        }
    }
    if ($restartNativeService) {
        try {
            Start-NativeService -ServiceName $nativeServiceName
        } catch {
            $restoreFailures += "native service recovery failed: $($_.Exception.Message)"
        }
    }
    }
    if ($restoreFailures.Count -gt 0) {
        throw "$deploymentFailure; rollback also failed: $($restoreFailures -join '; ')"
    }
    throw $deploymentFailure
}

Remove-Item $previousExe, $previousPdb, $previousManagerExe, $previousStableManagerExe, $previousAppExe, $previousStableAppExe, $previousKeeperExe, $previousKeeperPdb, $previousStableKeeperExe, $previousKeeperProvision, $previousRestartProvision, $previousOmpLoader, $previousStableOmpLoader, $previousManifest -Force -ErrorAction SilentlyContinue
if ($null -ne $clientProjection) {
    Remove-Item $clientProjection.Previous -Force -ErrorAction SilentlyContinue
}

# Dev build trees are disposable after deployment; target\deploy remains the
# bounded cache. Never prune a tree that contains the live binaries.
Write-Host "==> pruning dev build trees (target\debug, target\release)"
foreach ($devTree in @(
    (Join-Path $athanorRoot "target\debug"),
    (Join-Path $athanorRoot "target\release")
)) {
    $treePrefix = [IO.Path]::GetFullPath($devTree) + [IO.Path]::DirectorySeparatorChar
    if ($liveExe.StartsWith($treePrefix)) {
        Write-Host "    keeping $devTree (holds the live executable)"
        continue
    }
    if (Test-Path $devTree -PathType Container) {
        Remove-Item $devTree -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Write-Host "==> deployment complete"
Write-Host "live executable: $liveExe"
Write-Host "restart OMP once before the next Athanor tool call so its transport and TypeScript tool schemas reload"
