#Requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$RoomDir,
    [Parameter(Mandatory = $true)][string]$OmpProgram,
    [string[]]$OmpArgs = @(),
    [string]$Workspace,
    [string]$ProgramRoot = "$env:ProgramFiles/Solarisael/Athanor",
    [string]$StateRoot,
    [ValidateRange(0, 3600)][int]$WatchIntervalSecs = 30,
    [string]$SubstrateEnv = "C:/Solarisael/Obsidian/obsidian/house/state/substrate/.env",
    [string]$CapabilityScript,
    # A keeper holder file whose secret works, for example another room's
    # .omp/runtime/restart-capability. Every room's keeper presents the one
    # shared omp-keeper secret, so each later room copies it from here. Only
    # the first room of a House omits this, and that room mints the secret.
    [string]$SourceSecretPath
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$room = [IO.Path]::GetFileName([IO.Path]::GetFullPath($RoomDir).TrimEnd('\', '/'))
if ($room -notmatch '^[a-z0-9]+(-[a-z0-9]+)*$') {
    throw "RoomDir must end in a lowercase room key: $RoomDir"
}
foreach ($path in @($RoomDir, $OmpProgram, $ProgramRoot, $SubstrateEnv)) {
    if (-not [IO.Path]::IsPathRooted($path)) {
        throw "Every keeper path must be absolute: $path"
    }
}
if (-not (Test-Path $RoomDir -PathType Container)) {
    throw "RoomDir does not exist: $RoomDir"
}
if (-not (Test-Path $OmpProgram -PathType Leaf)) {
    throw "OMP program does not exist: $OmpProgram"
}
$hasSource = -not [string]::IsNullOrWhiteSpace($SourceSecretPath)
if ($hasSource -and -not [IO.Path]::IsPathRooted($SourceSecretPath)) {
    throw "Every keeper path must be absolute: $SourceSecretPath"
}
if ($hasSource -and -not (Test-Path -LiteralPath $SourceSecretPath -PathType Leaf)) {
    throw "SourceSecretPath does not exist: $SourceSecretPath"
}
foreach ($argument in $OmpArgs) {
    if ($argument -in @("--continue", "-c", "--resume", "-r") -or $argument.StartsWith("--resume=")) {
        throw "OmpArgs must not select a session; the keeper applies resume or fresh mode"
    }
}
$keeper = Join-Path $ProgramRoot "bin/athanor.exe"
if (-not (Test-Path $keeper -PathType Leaf)) {
    $currentPath = Join-Path $ProgramRoot "current.json"
    if (-not (Test-Path $currentPath -PathType Leaf)) {
        throw "Installed release pointer does not exist: $currentPath"
    }
    $current = Get-Content $currentPath -Raw | ConvertFrom-Json
    $keeper = Join-Path $ProgramRoot "versions/$([string]$current.version)/bin/athanor.exe"
}
if (-not (Test-Path $keeper -PathType Leaf)) {
    throw "Installed athanor.exe does not exist: $keeper"
}
if ([string]::IsNullOrWhiteSpace($Workspace)) {
    $Workspace = [IO.Path]::GetFullPath($RoomDir)
} elseif (-not [IO.Path]::IsPathRooted($Workspace)) {
    throw "Workspace must be absolute: $Workspace"
} else {
    $Workspace = [IO.Path]::GetFullPath($Workspace)
}
if ([string]::IsNullOrWhiteSpace($StateRoot)) {
    $StateRoot = [IO.Path]::GetDirectoryName(
        [IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($SubstrateEnv))
    )
} elseif (-not [IO.Path]::IsPathRooted($StateRoot)) {
    throw "StateRoot must be absolute: $StateRoot"
} else {
    $StateRoot = [IO.Path]::GetFullPath($StateRoot)
}
if (-not (Test-Path $StateRoot -PathType Container)) {
    throw "Athanor state root does not exist: $StateRoot"
}

$runtime = Join-Path ([IO.Path]::GetFullPath($RoomDir)) ".omp/runtime"
New-Item -ItemType Directory -Force -Path $runtime | Out-Null
if ([string]::IsNullOrWhiteSpace($CapabilityScript)) {
    $installedCapabilityScript = Join-Path $PSScriptRoot "provision-restart-capability.ps1"
    if (Test-Path $installedCapabilityScript -PathType Leaf) {
        $CapabilityScript = $installedCapabilityScript
    } else {
        $repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "../../..")).Path
        $CapabilityScript = Join-Path $repositoryRoot "substrate/provision-restart-capability.ps1"
    }
}
if (-not (Test-Path $CapabilityScript -PathType Leaf)) {
    throw "Restart capability provisioner does not exist: $CapabilityScript"
}

$configPath = Join-Path $runtime "omp-keeper.json"
$grants = @(
    [pscustomobject]@{ Principal = "omp-keeper"; Operation = "restart_claim"; File = "restart-capability" },
    [pscustomobject]@{ Principal = $room; Operation = "restart_request"; File = "restart-request-capability" },
    [pscustomobject]@{ Principal = $room; Operation = "restart_exit"; File = "restart-exit-capability" },
    [pscustomobject]@{ Principal = $room; Operation = "restart_verify"; File = "restart-verify-capability" }
)
$existing = @($grants | Where-Object { Test-Path (Join-Path $runtime $_.File) -PathType Leaf })
if ($existing.Count -gt 0 -or (Test-Path $configPath -PathType Leaf)) {
    throw "keeper runtime is already provisioned; repair one capability at a time with provision-restart-capability.ps1 (-SourceSecretPath shares the keeper secret, -Rotate replaces a room key)"
}

$temporary = "$configPath.new-$PID"
# Set only when this run created the shared keeper authority. Nobody else can
# hold that secret yet, so rollback may delete the row.
$keeperMinted = $false
try {
    foreach ($grant in $grants) {
        $grantArguments = @{
            Principal = $grant.Principal
            OperationClass = $grant.Operation
            HolderDir = $runtime
            SubstrateEnv = $SubstrateEnv
        }
        if ($grant.Operation -ne "restart_claim") {
            # This room is the only holder of its own keys, and its runtime has
            # none of them yet. A leftover row has no holder, so it is replaced.
            $grantArguments.Rotate = $true
        } elseif ($hasSource) {
            $grantArguments.SourceSecretPath = $SourceSecretPath
        }
        & $CapabilityScript @grantArguments
        if ($grant.Operation -eq "restart_claim") {
            # This runtime had no keeper secret. With no source, the claim
            # succeeds only when it mints the first authority.
            $keeperMinted = -not $hasSource
        }
    }
    $config = [ordered]@{
        ompLaunch = @([IO.Path]::GetFullPath($OmpProgram)) + @($OmpArgs)
        workspace = $Workspace
        programRoot = [IO.Path]::GetFullPath($ProgramRoot)
        stateRoot = $StateRoot
        capabilityPath = Join-Path $runtime "restart-capability"
        claimant = "omp-keeper"
        watchIntervalSecs = $WatchIntervalSecs
    }
    $config | ConvertTo-Json -Depth 4 | Set-Content $temporary -Encoding utf8NoBOM
    Move-Item $temporary $configPath -Force
} catch {
    $provisionFailure = $_
    $rollbackFailures = @()
    foreach ($grant in $grants) {
        try {
            if ($grant.Operation -eq "restart_claim" -and -not $keeperMinted) {
                # Every other room's keeper still presents the shared secret.
                # Remove only this room's copy and leave the authority row alone.
                $claimSecret = Join-Path $runtime $grant.File
                if (Test-Path $claimSecret -PathType Leaf) {
                    Remove-Item $claimSecret -Force -ErrorAction Stop
                }
            } else {
                & $CapabilityScript -Principal $grant.Principal -OperationClass $grant.Operation -HolderDir $runtime -SubstrateEnv $SubstrateEnv -Remove
            }
        } catch {
            $rollbackFailures += "$($grant.Principal)/$($grant.Operation): $($_.Exception.Message)"
        }
    }
    Remove-Item $temporary, $configPath -Force -ErrorAction SilentlyContinue
    if ($rollbackFailures.Count -gt 0) {
        throw "$provisionFailure; capability rollback also failed: $($rollbackFailures -join '; ')"
    }
    throw $provisionFailure
}

Write-Host "keeper configured for room '$room': $configPath"
Write-Host "start this room through:"
Write-Host "& '$keeper' keeper --config '$configPath'"
