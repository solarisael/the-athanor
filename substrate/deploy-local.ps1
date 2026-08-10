#Requires -Version 7.0
[CmdletBinding()]
param(
    [switch]$SkipTests,
    [switch]$SkipBackup,
    [ValidateRange(1, 365)]
    [int]$BackupKeep = 14,
    [string]$Cargo = "cargo",
    [string]$Python = "python"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $IsWindows) {
    throw "deploy-local.ps1 supports the canonical Windows + WSL deployment path only"
}

$substrateRoot = [IO.Path]::GetFullPath($PSScriptRoot)
$athanorRoot = [IO.Path]::GetFullPath((Join-Path $substrateRoot ".."))
# Mutable state lives outside the immutable product tree. Mirrors
# state_paths.py, state_paths.sh, and crates/house-substrate/src/state.rs:
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
$previousExe = Join-Path $stageTarget "previous\athanor-substrate.exe"
$previousPdb = [IO.Path]::ChangeExtension($previousExe, ".pdb")

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

function Get-LiveSubstrateWorkers {
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

if (-not (Test-Path (Join-Path $athanorRoot "Cargo.toml") -PathType Leaf)) {
    throw "The Athanor workspace Cargo.toml is missing at $athanorRoot"
}
if (-not (Test-Path (Join-Path $athanorRoot "crates\house-substrate\Cargo.toml") -PathType Leaf)) {
    throw "substrate crate Cargo.toml is missing at $athanorRoot\crates\house-substrate"
}

New-Item -ItemType Directory -Force -Path $stageTarget | Out-Null

if (-not $SkipTests) {
    Invoke-Checked -Label "Athanor core and protocol tests" -FilePath $Cargo -ArgumentList @(
        "test", "--manifest-path", (Join-Path $athanorRoot "Cargo.toml"),
        "-p", "house-core", "-p", "house-protocol"
    )
    Invoke-Checked -Label "substrate regression tests" -FilePath $Cargo -ArgumentList @(
        "test", "--manifest-path", (Join-Path $athanorRoot "Cargo.toml"),
        "-p", "athanor-substrate", "--release", "--target-dir", $stageTarget
    )
}

Invoke-Checked -Label "staged release build" -FilePath $Cargo -ArgumentList @(
    "build", "--manifest-path", (Join-Path $athanorRoot "Cargo.toml"),
    "-p", "athanor-substrate", "--release", "--target-dir", $stageTarget
)
if (-not (Test-Path $stagedExe -PathType Leaf)) {
    throw "staged executable was not produced at $stagedExe"
}

if (-not $SkipBackup) {
    $priorPgWsl = $env:SOLARISAEL_PG_WSL
    try {
        $env:SOLARISAEL_PG_WSL = "1"
        Invoke-Checked -Label "pre-deploy PostgreSQL backup" -FilePath $stagedExe -ArgumentList @(
            "backup", "--output-dir", (Join-Path $substrateStateDir "backups"),
            "--keep", [string]$BackupKeep
        )
    } finally {
        if ($null -eq $priorPgWsl) {
            Remove-Item Env:SOLARISAEL_PG_WSL -ErrorAction SilentlyContinue
        } else {
            $env:SOLARISAEL_PG_WSL = $priorPgWsl
        }
    }
}

$workers = @(Get-LiveSubstrateWorkers -ExecutablePath $liveExe)
if ($workers.Count -gt 0) {
    $workerSummary = ($workers | ForEach-Object { "PID=$($_.ProcessId) parent=$($_.ParentProcessId)" }) -join ", "
    Write-Host "==> stopping exact-path substrate workers: $workerSummary"
    foreach ($worker in $workers) {
        Stop-Process -Id $worker.ProcessId -Force -ErrorAction Stop
    }

    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        $remainingWorkers = @(Get-LiveSubstrateWorkers -ExecutablePath $liveExe)
        if ($remainingWorkers.Count -eq 0) {
            break
        }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)

    if ($remainingWorkers.Count -gt 0) {
        throw "substrate workers did not stop within 10 seconds"
    }
}

New-Item -ItemType Directory -Force -Path ([IO.Path]::GetDirectoryName($liveExe)) | Out-Null
New-Item -ItemType Directory -Force -Path ([IO.Path]::GetDirectoryName($previousExe)) | Out-Null
Remove-Item $previousExe, $previousPdb -Force -ErrorAction SilentlyContinue
if (Test-Path $liveExe -PathType Leaf) {
    Move-Item $liveExe $previousExe -Force
}
if (Test-Path $livePdb -PathType Leaf) {
    Move-Item $livePdb $previousPdb -Force
}

try {
    Copy-Item $stagedExe $liveExe -Force
    if (Test-Path $stagedPdb -PathType Leaf) {
        Copy-Item $stagedPdb $livePdb -Force
    }
    Invoke-Checked -Label "ordered database migrations" -FilePath $Python -ArgumentList @(
        (Join-Path $substrateRoot "run_migrations.py")
    )
    Invoke-Checked -Label "semantic vocabulary refresh" -FilePath $liveExe -ArgumentList @(
        "semantic-vocabulary-refresh"
    )
} catch {
    Remove-Item $liveExe, $livePdb -Force -ErrorAction SilentlyContinue
    if (Test-Path $previousExe -PathType Leaf) {
        Move-Item $previousExe $liveExe -Force
    }
    if (Test-Path $previousPdb -PathType Leaf) {
        Move-Item $previousPdb $livePdb -Force
    }
    throw
}

Remove-Item $previousExe, $previousPdb -Force -ErrorAction SilentlyContinue
Invoke-Checked -Label "Full-mode health proof" -FilePath $Python -ArgumentList @(
    (Join-Path $substrateRoot "health.py")
)

Write-Host "==> deployment complete"
Write-Host "live executable: $liveExe"
Write-Host "restart OMP once before the next Athanor tool call so its transport and TypeScript tool schemas reload"
