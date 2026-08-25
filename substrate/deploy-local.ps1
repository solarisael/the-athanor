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
# Mutable state lives outside the immutable product tree. This mirrors
# state_paths.sh and crates/house-substrate/src/state.rs:
# ATHANOR_STATE_DIR wins and must be absolute, then an existing
# <install-root>/state, then a development checkout. Never a guess.
$configuredStateRoot = [string]$env:ATHANOR_STATE_DIR
$installedStateRoot = Join-Path ([IO.Path]::GetDirectoryName($athanorRoot)) "state"
$stateRoot = if (-not [string]::IsNullOrWhiteSpace($configuredStateRoot)) {
    if (-not [IO.Path]::IsPathRooted($configuredStateRoot)) {
        throw "ATHANOR_STATE_DIR must be an absolute path (got $configuredStateRoot)"
    }
    $configuredStateRoot
} elseif (Test-Path $installedStateRoot -PathType Container) {
    $installedStateRoot
} elseif ((Test-Path (Join-Path $athanorRoot "Cargo.toml") -PathType Leaf) -and
          (Test-Path (Join-Path $athanorRoot "crates") -PathType Container)) {
    Join-Path $athanorRoot "state"
} else {
    throw "ATHANOR_STATE_DIR is not set and no state root could be resolved; set it to the absolute path of <install-root>/state"
}
$substrateStateDir = [IO.Path]::GetFullPath((Join-Path $stateRoot "substrate"))
$stageTarget = Join-Path $athanorRoot "target\deploy"
 $stagedExe = Join-Path $stageTarget "release\athanor-substrate.exe"
 $stagedPdb = [IO.Path]::ChangeExtension($stagedExe, ".pdb")
 $stagedHostExe = Join-Path $stageTarget "release\house-host.exe"
 $stagedHostPdb = [IO.Path]::ChangeExtension($stagedHostExe, ".pdb")
 $stagedManagerExe = Join-Path $stageTarget "release\athanor-manage.exe"
 $stagedKeeperExe = Join-Path $stageTarget "release\omp-keeper.exe"
 $stagedKeeperPdb = [IO.Path]::ChangeExtension($stagedKeeperExe, ".pdb")
$configuredLiveExe = [string]$env:ATHANOR_SUBSTRATE_EXE
$liveExe = if ([string]::IsNullOrWhiteSpace($configuredLiveExe)) {
    Join-Path $athanorRoot "target\release\athanor-substrate.exe"
} elseif ([IO.Path]::IsPathRooted($configuredLiveExe)) {
    $configuredLiveExe
} else {
    Join-Path $athanorRoot $configuredLiveExe
}
$liveExe = [IO.Path]::GetFullPath($liveExe)
$livePdb = [IO.Path]::ChangeExtension($liveExe, ".pdb")
$liveHostExe = Join-Path ([IO.Path]::GetDirectoryName($liveExe)) "house-host.exe"
$liveHostPdb = [IO.Path]::ChangeExtension($liveHostExe, ".pdb")
 $liveManagerExe = Join-Path ([IO.Path]::GetDirectoryName($liveExe)) "athanor-manage.exe"
 $liveKeeperExe = Join-Path ([IO.Path]::GetDirectoryName($liveExe)) "omp-keeper.exe"
 $liveKeeperPdb = [IO.Path]::ChangeExtension($liveKeeperExe, ".pdb")
 $stableKeeperExe = Join-Path ([IO.Path]::GetDirectoryName([IO.Path]::GetDirectoryName([IO.Path]::GetDirectoryName([IO.Path]::GetDirectoryName($liveExe))))) "bin\omp-keeper.exe"
 $sourceKeeperProvision = Join-Path $athanorRoot "crates\omp-keeper\scripts\provision-local.ps1"
 $sourceRestartProvision = Join-Path $athanorRoot "substrate\provision-restart-capability.ps1"
 $stableKeeperProvision = Join-Path ([IO.Path]::GetDirectoryName($stableKeeperExe)) "provision-omp-keeper.ps1"
 $stableRestartProvision = Join-Path ([IO.Path]::GetDirectoryName($stableKeeperExe)) "provision-restart-capability.ps1"
$stableManagerExe = Join-Path ([IO.Path]::GetDirectoryName([IO.Path]::GetDirectoryName([IO.Path]::GetDirectoryName([IO.Path]::GetDirectoryName($liveExe))))) "bin\athanor-manage.exe"
 $previousExe = Join-Path $stageTarget "previous\athanor-substrate.exe"
 $previousPdb = [IO.Path]::ChangeExtension($previousExe, ".pdb")
 $previousHostExe = Join-Path $stageTarget "previous\house-host.exe"
 $previousHostPdb = [IO.Path]::ChangeExtension($previousHostExe, ".pdb")
 $previousManagerExe = Join-Path $stageTarget "previous\athanor-manage.exe"
 $previousStableManagerExe = Join-Path $stageTarget "previous\athanor-manage-stable.exe"
 $previousKeeperExe = Join-Path $stageTarget "previous\omp-keeper.exe"
 $previousKeeperPdb = Join-Path $stageTarget "previous\omp-keeper.pdb"
 $previousStableKeeperExe = Join-Path $stageTarget "previous\omp-keeper-stable.exe"
 $previousKeeperProvision = Join-Path $stageTarget "previous\provision-omp-keeper.ps1"
 $previousRestartProvision = Join-Path $stageTarget "previous\provision-restart-capability.ps1"
 $liveManifest = Join-Path ([IO.Path]::GetDirectoryName([IO.Path]::GetDirectoryName($liveExe))) "release-manifest.json"
 $previousManifest = Join-Path $stageTarget "previous\release-manifest.json"
$nativeServiceName = "SolarisaelAthanor"
$runtimeConfigPath = Join-Path ([Environment]::GetFolderPath("CommonApplicationData")) "Solarisael\Athanor\config\runtime.json"
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

function Start-NativeRuntimeAndWaitForHosts {
    param(
        [Parameter(Mandatory)] [string]$ServiceName,
        [Parameter(Mandatory)] [string]$RuntimeConfigPath
    )

    Write-Host "==> starting native Athanor service"
    $service = Get-Service -Name $ServiceName -ErrorAction Stop
    if ($service.Status -ne [System.ServiceProcess.ServiceControllerStatus]::Running) {
        Start-Service -Name $ServiceName -ErrorAction Stop
        $service.WaitForStatus(
            [System.ServiceProcess.ServiceControllerStatus]::Running,
            [TimeSpan]::FromSeconds(30)
        )
    }
    if (-not (Test-Path $RuntimeConfigPath -PathType Leaf)) {
        throw "native runtime configuration is missing at $RuntimeConfigPath"
    }
    $runtime = Get-Content $RuntimeConfigPath -Raw | ConvertFrom-Json
    $rooms = @($runtime.rooms)
    if ($rooms.Count -eq 0) {
        throw "native runtime configuration contains no room Hosts"
    }

    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        $pending = @()
        foreach ($room in $rooms) {
            $roomName = [string]$room.room
            $port = [int]$room.port
            try {
                $health = Invoke-RestMethod -Uri "http://127.0.0.1:$port/health" -TimeoutSec 2
                if ([string]$health.status -ne "ok") {
                    $pending += "$roomName@$port status=$($health.status)"
                }
            } catch {
                $pending += "$roomName@$port unavailable"
            }
        }
        if ($pending.Count -eq 0) {
            Write-Host "==> room Host health proof: $($rooms.Count) healthy"
            return
        }
        Start-Sleep -Milliseconds 250
    } while ([DateTime]::UtcNow -lt $deadline)

    throw "room Hosts did not recover within 30 seconds: $($pending -join ', ')"
}

if (-not (Test-Path (Join-Path $athanorRoot "Cargo.toml") -PathType Leaf)) {
    throw "The Athanor workspace Cargo.toml is missing at $athanorRoot"
}
if (-not (Test-Path (Join-Path $athanorRoot "crates\house-substrate\Cargo.toml") -PathType Leaf)) {
    throw "substrate crate Cargo.toml is missing at $athanorRoot\crates\house-substrate"
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



if (-not $SkipTests) {
    Invoke-Checked -Label "Athanor core, protocol, substrate, and Host tests" -FilePath $Cargo -ArgumentList @(
        "test", "--manifest-path", (Join-Path $athanorRoot "Cargo.toml"),
        "-p", "house-core", "-p", "house-protocol", "-p", "athanor-substrate", "-p", "house-host", "-p", "athanor-install", "-p", "omp-keeper", "--release", "--target-dir", $stageTarget
    )
}

Invoke-Checked -Label "staged release build" -FilePath $Cargo -ArgumentList @(
    "build", "--manifest-path", (Join-Path $athanorRoot "Cargo.toml"),
    "-p", "house-core", "-p", "house-protocol", "-p", "athanor-substrate", "-p", "house-host", "-p", "athanor-install", "-p", "omp-keeper", "--release", "--target-dir", $stageTarget
)
if (-not (Test-Path $stagedExe -PathType Leaf)) {
    throw "staged executable was not produced at $stagedExe"
}
if (-not (Test-Path $stagedHostExe -PathType Leaf)) {
    throw "staged Host executable was not produced at $stagedHostExe"
}
if (-not (Test-Path $stagedManagerExe -PathType Leaf)) {
    throw "staged manager executable was not produced at $stagedManagerExe"
}
if (-not (Test-Path $stagedKeeperExe -PathType Leaf)) {
    throw "staged keeper executable was not produced at $stagedKeeperExe"
}
foreach ($provisioner in @($sourceKeeperProvision, $sourceRestartProvision)) {
    if (-not (Test-Path $provisioner -PathType Leaf)) {
        throw "keeper provisioner is missing: $provisioner"
    }
}

if (-not $SkipBackup) {
    $priorPgWsl = $env:SOLARISAEL_PG_WSL
    try {
        $env:SOLARISAEL_PG_WSL = "1"
        # The WSL pg_dump path fails intermittently on a cold WSL VM (observed
        # twice on 2026-08-22, both times immediately after other WSL work;
        # every manual re-run succeeds). A backup is idempotent, so retry once
        # and loudly rather than letting the deploy die at its first step.
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
            Remove-Item Env:SOLARISAEL_PG_WSL -ErrorAction SilentlyContinue
        } else {
            $env:SOLARISAEL_PG_WSL = $priorPgWsl
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

foreach ($workerPath in @($liveExe, $liveHostExe)) {
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

try {
    $copiedExe = $false
    $copiedPdb = $false
    $copiedHostExe = $false
    $copiedHostPdb = $false
    $copiedManager = $false
    $copiedStableManager = $false
    $copiedKeeperExe = $false
    $copiedKeeperPdb = $false
    $copiedStableKeeper = $false
    $copiedKeeperProvision = $false
    $copiedRestartProvision = $false
New-Item -ItemType Directory -Force -Path ([IO.Path]::GetDirectoryName($liveExe)) | Out-Null
New-Item -ItemType Directory -Force -Path ([IO.Path]::GetDirectoryName($previousExe)) | Out-Null
foreach ($priorPath in @($previousExe, $previousPdb, $previousHostExe, $previousHostPdb, $previousManagerExe, $previousStableManagerExe, $previousKeeperExe, $previousKeeperPdb, $previousStableKeeperExe, $previousKeeperProvision, $previousRestartProvision, $previousManifest)) {
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
if (Test-Path $liveHostExe -PathType Leaf) {
    Move-Item $liveHostExe $previousHostExe -Force
}
if (Test-Path $liveHostPdb -PathType Leaf) {
    Move-Item $liveHostPdb $previousHostPdb -Force
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
    if (-not (Test-Path $liveManagerExe -PathType Leaf) -or -not (Test-Path $stableManagerExe -PathType Leaf)) {
        throw "installed manager paths are missing or ambiguous; recover both version and stable manager paths before deployment"
    }
    Move-Item $liveManagerExe $previousManagerExe -Force
    Move-Item $stableManagerExe $previousStableManagerExe -Force
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
    Copy-Item $stagedHostExe $liveHostExe -Force
    $copiedHostExe = $true
    if (Test-Path $stagedHostPdb -PathType Leaf) {
        Copy-Item $stagedHostPdb $liveHostPdb -Force
        $copiedHostPdb = $true
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
        Copy-Item $stagedKeeperExe $stableKeeperExe -Force
        $copiedStableKeeper = $true
        Copy-Item $sourceKeeperProvision $stableKeeperProvision -Force
        $copiedKeeperProvision = $true
        Copy-Item $sourceRestartProvision $stableRestartProvision -Force
        $copiedRestartProvision = $true
    }
    if (Test-Path $liveManifest -PathType Leaf) {
        $manifest = Get-Content $liveManifest -Raw | ConvertFrom-Json
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
        foreach ($binary in @(
            @{ Path = "bin/athanor-substrate.exe"; Source = $liveExe },
            @{ Path = "bin/house-host.exe"; Source = $liveHostExe },
            @{ Path = "bin/athanor-manage.exe"; Source = $liveManagerExe },
            @{ Path = "bin/omp-keeper.exe"; Source = $liveKeeperExe }
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
        Start-NativeRuntimeAndWaitForHosts `
            -ServiceName $nativeServiceName `
            -RuntimeConfigPath $runtimeConfigPath
    }
    Invoke-Checked -Label "Full-mode health proof" -FilePath $liveExe -ArgumentList @(
        "health", "--substrate-dir", $substrateRoot
    )
    if (Test-Path $liveManifest -PathType Leaf) {
        Invoke-Checked -Label "native release manifest proof" -FilePath $liveManagerExe -ArgumentList @(
            "doctor"
        )
    }
} catch {
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
        @{ Live = $liveHostExe; Previous = $previousHostExe; Created = $copiedHostExe },
        @{ Live = $liveHostPdb; Previous = $previousHostPdb; Created = $copiedHostPdb },
        @{ Live = $liveManagerExe; Previous = $previousManagerExe; Created = $copiedManager },
        @{ Live = $stableManagerExe; Previous = $previousStableManagerExe; Created = $copiedStableManager },
        @{ Live = $liveKeeperExe; Previous = $previousKeeperExe; Created = $copiedKeeperExe },
        @{ Live = $liveKeeperPdb; Previous = $previousKeeperPdb; Created = $copiedKeeperPdb },
        @{ Live = $stableKeeperExe; Previous = $previousStableKeeperExe; Created = $copiedStableKeeper },
        @{ Live = $stableKeeperProvision; Previous = $previousKeeperProvision; Created = $copiedKeeperProvision },
        @{ Live = $stableRestartProvision; Previous = $previousRestartProvision; Created = $copiedRestartProvision },
        @{ Live = $liveManifest; Previous = $previousManifest; Created = $false }
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
            Start-NativeRuntimeAndWaitForHosts `
                -ServiceName $nativeServiceName `
                -RuntimeConfigPath $runtimeConfigPath
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

Remove-Item $previousExe, $previousPdb, $previousHostExe, $previousHostPdb, $previousManagerExe, $previousStableManagerExe, $previousKeeperExe, $previousKeeperPdb, $previousStableKeeperExe, $previousKeeperProvision, $previousRestartProvision, $previousManifest -Force -ErrorAction SilentlyContinue

# Sol's standing rule (2026-08-22): do not leave compile output behind.
# Remove the dev build trees after a good deploy. Keep target\deploy:
# it is the ritual's build cache and makes the next deploy fast.
# WARNING: on a dev-default install the live executable lives inside
# target\release. Never prune a tree that holds the live binaries.
Write-Host "==> pruning dev build trees (target\debug, target\release)"
foreach ($devTree in @(
    (Join-Path $athanorRoot "target\debug"),
    (Join-Path $athanorRoot "target\release")
)) {
    $treePrefix = [IO.Path]::GetFullPath($devTree) + [IO.Path]::DirectorySeparatorChar
    if ($liveExe.StartsWith($treePrefix) -or $liveHostExe.StartsWith($treePrefix)) {
        Write-Host "    keeping $devTree (holds the live executable)"
        continue
    }
    if (Test-Path $devTree -PathType Container) {
        Remove-Item $devTree -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Write-Host "==> deployment complete"
Write-Host "live executable: $liveExe"
Write-Host "live Host: $liveHostExe"
Write-Host "restart OMP once before the next Athanor tool call so its transport and TypeScript tool schemas reload"
