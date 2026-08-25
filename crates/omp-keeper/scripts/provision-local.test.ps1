#Requires -Version 7.0
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

$root = Join-Path ([IO.Path]::GetTempPath()) "omp-keeper-provision-$PID-$([Guid]::NewGuid().ToString('N'))"
try {
    $room = Join-Path $root "kintsu"
    $runtime = Join-Path $room ".omp/runtime"
    $programRoot = Join-Path $root "program"
    $omp = Join-Path $root "omp.exe"
    $stateRoot = Join-Path $root "state"
    $substrateEnv = Join-Path $stateRoot "substrate/.env"
    $mock = Join-Path $root "mock-capability.ps1"
    $log = Join-Path $root "calls.log"
    New-Item $runtime, (Join-Path $programRoot "bin"), (Split-Path $substrateEnv -Parent) -ItemType Directory -Force | Out-Null
    Set-Content (Join-Path $programRoot "bin/omp-keeper.exe") "keeper"
    Set-Content $omp "omp"
    Set-Content $substrateEnv "fixture"
    $env:OMP_KEEPER_PROVISION_TEST_LOG = $log
    $env:OMP_KEEPER_PROVISION_TEST_FAIL_OPERATION = "restart_exit"

    @'
param(
    [string]$Principal,
    [string]$OperationClass,
    [string]$HolderDir,
    [string]$SubstrateEnv,
    [switch]$Remove
)
$file = if ($OperationClass -eq "restart_claim") { "restart-capability" } else { ($OperationClass -replace "_", "-") + "-capability" }
$verb = if ($Remove) { "remove" } else { "add" }
Add-Content $env:OMP_KEEPER_PROVISION_TEST_LOG "$verb $Principal $OperationClass"
if ($Remove) {
    Remove-Item (Join-Path $HolderDir $file) -Force -ErrorAction SilentlyContinue
    return
}
if ($OperationClass -eq $env:OMP_KEEPER_PROVISION_TEST_FAIL_OPERATION) { throw "fixture third grant failure" }
Set-Content (Join-Path $HolderDir $file) "fixture-secret"
'@ | Set-Content $mock

    $provision = Join-Path $PSScriptRoot "provision-local.ps1"
    $failure = $null
    try {
        & $provision -RoomDir $room -OmpProgram $omp -ProgramRoot $programRoot -SubstrateEnv $substrateEnv -CapabilityScript $mock
    } catch {
        $failure = $_.Exception.Message
    }
    Assert-True (-not [string]::IsNullOrWhiteSpace($failure)) "the third grant failure must refuse provisioning"
    Assert-True ($failure -like "*fixture third grant failure*") "the original provision failure must stay visible"
    foreach ($file in "restart-capability", "restart-request-capability", "restart-exit-capability", "restart-verify-capability", "omp-keeper.json") {
        Assert-True (-not (Test-Path (Join-Path $runtime $file))) "rollback must remove $file"
    }
    $calls = Get-Content $log
    foreach ($operation in "restart_claim", "restart_request", "restart_exit", "restart_verify") {
        Assert-True ($calls -contains "remove $($(if ($operation -eq 'restart_claim') { 'omp-keeper' } else { 'kintsu' })) $operation") "rollback must remove $operation"
    }

    $env:OMP_KEEPER_PROVISION_TEST_FAIL_OPERATION = ""
    & $provision -RoomDir $room -OmpProgram $omp -ProgramRoot $programRoot -SubstrateEnv $substrateEnv -CapabilityScript $mock
    $config = Get-Content (Join-Path $runtime "omp-keeper.json") -Raw | ConvertFrom-Json
    Assert-True ([string]$config.stateRoot -eq [IO.Path]::GetFullPath($stateRoot)) "config must carry the derived Athanor state root"
    Assert-True ([string]$config.workspace -eq [IO.Path]::GetFullPath($room)) "config must carry the room workspace"
    foreach ($file in "restart-capability", "restart-request-capability", "restart-exit-capability", "restart-verify-capability") {
        Assert-True (Test-Path (Join-Path $runtime $file) -PathType Leaf) "successful provisioning must write $file"
    }
    Write-Host "keeper provision success contract passed"
    Write-Host "keeper provision rollback contract passed"
} finally {
    Remove-Item Env:OMP_KEEPER_PROVISION_TEST_LOG -ErrorAction SilentlyContinue
    Remove-Item Env:OMP_KEEPER_PROVISION_TEST_FAIL_OPERATION -ErrorAction SilentlyContinue
    Remove-Item $root -Recurse -Force -ErrorAction SilentlyContinue
}
