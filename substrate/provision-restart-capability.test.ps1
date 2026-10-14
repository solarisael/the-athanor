#Requires -Version 7.0
# Real PostgreSQL smoke for provision-restart-capability.ps1 and for the keeper
# room provisioner that calls it. Nothing here mocks psql. The database needs
# migration 0026 applied. Every row this smoke writes belongs to a disposable
# principal named smoke-<run>-..., and the smoke deletes those rows at the end.
# It never writes a row for omp-keeper or for a live room.
#
# Usage:
#   pwsh substrate/provision-restart-capability.test.ps1 `
#     -SubstrateEnv <env file whose PGPASSWORD reaches the database> `
#     -DatabaseName <scratch database>
param(
    [Parameter(Mandatory = $true)][string]$SubstrateEnv,
    [Parameter(Mandatory = $true)][string]$DatabaseName
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

if ($DatabaseName -cnotmatch '^[a-z_][a-z0-9_]*$') {
    throw "DatabaseName must be a plain lowercase PostgreSQL name: $DatabaseName"
}
$SubstrateEnv = (Resolve-Path -LiteralPath $SubstrateEnv).ProviderPath
$provision = Join-Path $PSScriptRoot "provision-restart-capability.ps1"
$keeperProvision = Join-Path $PSScriptRoot "../crates/omp-keeper/scripts/provision-local.ps1"
$envPathWsl = (wsl -e wslpath -a ($SubstrateEnv -replace "\\", "/")).Trim()
$run = "smoke-" + [Guid]::NewGuid().ToString("N").Substring(0, 10)
$root = Join-Path ([IO.Path]::GetTempPath()) "restart-capability-$run"
$database = @{ SubstrateEnv = $SubstrateEnv; DatabaseName = $DatabaseName }

function Invoke-SmokeSql([string]$Sql) {
    $command = "PGPASSWORD=`$(grep '^PGPASSWORD=' '$envPathWsl' | cut -d= -f2 | tr -d '\r') psql -X -q -A -t -h 127.0.0.1 -p 5432 -U solarisael -d $DatabaseName -v ON_ERROR_STOP=1 -c `"$Sql`""
    $rows = @(wsl -e sh -lc $command)
    if ($LASTEXITCODE -ne 0) {
        throw "smoke query failed: $Sql"
    }
    $rows | ForEach-Object { "$_".Trim() } | Where-Object { $_ }
}

# One authority row as 'hash|rotated_at', or $null when the row is absent.
function Get-Authority([string]$Principal, [string]$OperationClass) {
    $rows = @(Invoke-SmokeSql "SELECT capability_hash || '|' || coalesce(rotated_at::text, 'never') FROM restart.principal_capabilities WHERE principal = '$Principal' AND operation_class = '$OperationClass';" | Where-Object { $_ -cmatch '^[0-9a-f]{64}\|' })
    if ($rows.Count -eq 0) { return $null }
    $rows[0]
}

# Holders trim the file and present its UTF-8 bytes, as the authority hashes them.
function Get-HolderHash([string]$Path) {
    $secret = ([string](Get-Content -LiteralPath $Path -Raw)).Trim()
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        -join ($sha.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($secret)) | ForEach-Object { $_.ToString("x2") })
    } finally {
        $sha.Dispose()
    }
}

function Assert-Holds([string]$Path, $Authority, [string]$Message) {
    Assert-True (Test-Path -LiteralPath $Path -PathType Leaf) "${Message}: the holder file is missing"
    Assert-True ($null -ne $Authority -and ([string]$Authority).StartsWith((Get-HolderHash $Path) + "|")) "${Message}: the holder secret does not match its authority"
}

function Assert-Refused([scriptblock]$Action, [string]$Pattern, [string]$Message) {
    $refusal = $null
    try {
        & $Action
    } catch {
        $refusal = $_.Exception.Message
    }
    Assert-True ($null -ne $refusal) "${Message}: the run must refuse"
    Assert-True ($refusal -like $Pattern) "${Message}: unexpected refusal: $refusal"
}

function New-Holder([string]$Name) {
    $path = Join-Path $root $Name
    New-Item $path -ItemType Directory -Force | Out-Null
    $path
}

try {
    New-Item $root -ItemType Directory -Force | Out-Null
    $table = @(Invoke-SmokeSql "SELECT to_regclass('restart.principal_capabilities') IS NOT NULL;")
    Assert-True ($table -contains "t") "database $DatabaseName has no restart.principal_capabilities; apply migration 0026 first"

    # 1. Two keeper holders share one restart_claim authority.
    $keeper = "$run-keeper"
    $kodo = New-Holder "kodo"
    $kintsu = New-Holder "kintsu"
    $kodoClaim = Join-Path $kodo "restart-capability"
    $kintsuClaim = Join-Path $kintsu "restart-capability"

    & $provision -Principal $keeper -HolderDir $kodo @database
    $authority = Get-Authority $keeper "restart_claim"
    Assert-Holds $kodoClaim $authority "the first holder must mint the shared secret"
    $kodoSecret = Get-Content -LiteralPath $kodoClaim -Raw

    & $provision -Principal $keeper -HolderDir $kodo @database
    Assert-True ((Get-Authority $keeper "restart_claim") -ceq $authority) "a repeat run must leave the authority alone"
    Assert-True ((Get-Content -LiteralPath $kodoClaim -Raw) -ceq $kodoSecret) "a repeat run must leave the holder secret alone"

    Assert-Refused { & $provision -Principal $keeper -HolderDir $kintsu @database } "*already has a restart_claim authority*" "a second holder without a source"
    Assert-True (-not (Test-Path -LiteralPath $kintsuClaim)) "a refused second holder must get no file"
    Assert-True ((Get-Authority $keeper "restart_claim") -ceq $authority) "a refused second holder must leave the authority alone"

    & $provision -Principal $keeper -HolderDir $kintsu -SourceSecretPath $kodoClaim @database
    Assert-True ((Get-Content -LiteralPath $kintsuClaim -Raw) -ceq $kodoSecret) "the second holder must get the shared secret"
    Assert-True ((Get-Authority $keeper "restart_claim") -ceq $authority) "sharing must leave the authority alone"
    Assert-Holds $kodoClaim $authority "the first holder must still work after the second joins"

    & $provision -Principal $keeper -HolderDir $kintsu @database
    Assert-True ((Get-Authority $keeper "restart_claim") -ceq $authority) "a repeat run on the second holder must leave the authority alone"

    $stale = Join-Path $root "stale-capability"
    Set-Content -LiteralPath $stale -Value ([Guid]::NewGuid().ToString("N") + [Guid]::NewGuid().ToString("N")) -NoNewline -Encoding ascii
    $staleSecret = Get-Content -LiteralPath $stale -Raw
    $hugo = New-Holder "hugo"
    Assert-Refused { & $provision -Principal $keeper -HolderDir $hugo -SourceSecretPath $stale @database } "*does not match*" "a stale source"
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $hugo "restart-capability"))) "a stale source must write no holder file"
    Assert-True ((Get-Authority $keeper "restart_claim") -ceq $authority) "a stale source must leave the authority alone"

    # The incident shape: a holder whose own file went stale.
    Copy-Item -LiteralPath $stale -Destination $kintsuClaim -Force
    Assert-Refused { & $provision -Principal $keeper -HolderDir $kintsu @database } "*does not match*" "a stale holder file"
    Assert-True ((Get-Content -LiteralPath $kintsuClaim -Raw) -ceq $staleSecret) "a refused stale holder must keep its file"
    Assert-True ((Get-Authority $keeper "restart_claim") -ceq $authority) "a stale holder file must leave the authority alone"
    & $provision -Principal $keeper -HolderDir $kintsu -SourceSecretPath $kodoClaim @database
    Assert-True ((Get-Content -LiteralPath $kintsuClaim -Raw) -ceq $kodoSecret) "a stale holder must be repaired from a working holder"
    Assert-True ((Get-Authority $keeper "restart_claim") -ceq $authority) "the repair must leave the authority alone"

    Assert-Refused { & $provision -Principal $keeper -HolderDir $kodo -Rotate @database } "*shared by every keeper holder*" "rotating the shared secret"
    Assert-True ((Get-Authority $keeper "restart_claim") -ceq $authority) "a refused rotation must leave the authority alone"
    Assert-True ((Get-Content -LiteralPath $kodoClaim -Raw) -ceq $kodoSecret) "a refused rotation must leave the holder alone"

    $absent = "$run-absent"
    $orphan = New-Holder "orphan"
    Assert-Refused { & $provision -Principal $absent -HolderDir $orphan -SourceSecretPath $kodoClaim @database } "*has no restart_claim authority*" "a source without an authority"
    Assert-True ($null -eq (Get-Authority $absent "restart_claim")) "a source without an authority must create no row"
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $orphan "restart-capability"))) "a source without an authority must write no file"
    Write-Host "shared restart_claim smoke passed"

    # 2. A room key is checked by default and replaced only with -Rotate.
    $roomKey = "$run-room"
    $roomHolder = New-Holder "room"
    $verify = Join-Path $roomHolder "restart-verify-capability"
    & $provision -Principal $roomKey -OperationClass restart_verify -HolderDir $roomHolder @database
    $roomAuthority = Get-Authority $roomKey "restart_verify"
    Assert-Holds $verify $roomAuthority "a room key must be minted"
    & $provision -Principal $roomKey -OperationClass restart_verify -HolderDir $roomHolder @database
    Assert-True ((Get-Authority $roomKey "restart_verify") -ceq $roomAuthority) "a repeat room run must leave the authority alone"
    Remove-Item -LiteralPath $verify
    Assert-Refused { & $provision -Principal $roomKey -OperationClass restart_verify -HolderDir $roomHolder @database } "*already has a restart_verify authority*" "a lost room key without -Rotate"
    Assert-True ((Get-Authority $roomKey "restart_verify") -ceq $roomAuthority) "a refused room run must leave the authority alone"
    & $provision -Principal $roomKey -OperationClass restart_verify -HolderDir $roomHolder -Rotate @database
    $rotated = Get-Authority $roomKey "restart_verify"
    Assert-True ($rotated -cne $roomAuthority) "-Rotate must replace the room authority"
    Assert-Holds $verify $rotated "-Rotate must deliver the new room key"
    Write-Host "room key smoke passed"

    # 3. The keeper room provisioner gives a later room the first room's keeper
    #    secret. It always names omp-keeper, so a pass-through wrapper points it
    #    at a disposable keeper principal and this database, then runs the real
    #    capability script.
    $sharedKeeper = "$run-shared"
    $env:RESTART_SMOKE_PROVISIONER = $provision
    $env:RESTART_SMOKE_KEEPER = $sharedKeeper
    $env:RESTART_SMOKE_DATABASE = $DatabaseName
    $wrapper = Join-Path $root "capability-on-smoke-database.ps1"
    @'
param(
    [string]$Principal,
    [string]$OperationClass,
    [string]$HolderDir,
    [string]$SubstrateEnv,
    [string]$SourceSecretPath,
    [switch]$Rotate,
    [switch]$Remove
)
$forward = @{}
foreach ($name in $PSBoundParameters.Keys) { $forward[$name] = $PSBoundParameters[$name] }
if ($Principal -eq "omp-keeper") { $forward.Principal = $env:RESTART_SMOKE_KEEPER }
$forward.DatabaseName = $env:RESTART_SMOKE_DATABASE
& $env:RESTART_SMOKE_PROVISIONER @forward
'@ | Set-Content -LiteralPath $wrapper
    $programRoot = New-Holder "program"
    New-Item (Join-Path $programRoot "bin") -ItemType Directory -Force | Out-Null
    Set-Content (Join-Path $programRoot "bin/athanor.exe") "keeper"
    $omp = Join-Path $root "omp.exe"
    Set-Content $omp "omp"
    $roomArguments = @{
        OmpProgram = $omp
        ProgramRoot = $programRoot
        StateRoot = (New-Holder "state")
        SubstrateEnv = $SubstrateEnv
        CapabilityScript = $wrapper
    }

    $first = New-Holder "$run-kodo"
    $firstClaim = Join-Path $first ".omp/runtime/restart-capability"
    & $keeperProvision -RoomDir $first @roomArguments
    $shared = Get-Authority $sharedKeeper "restart_claim"
    Assert-Holds $firstClaim $shared "the first room must mint the keeper secret"
    foreach ($class in "restart_request", "restart_exit", "restart_verify") {
        Assert-Holds (Join-Path $first ".omp/runtime/$($class -replace '_', '-')-capability") (Get-Authority "$run-kodo" $class) "the first room's $class key"
    }

    $second = New-Holder "$run-kintsu"
    $secondRuntime = Join-Path $second ".omp/runtime"
    Assert-Refused { & $keeperProvision -RoomDir $second @roomArguments } "*already has a restart_claim authority*" "a second room without a source"
    Assert-True ((Get-Authority $sharedKeeper "restart_claim") -ceq $shared) "a refused second room must leave the keeper authority alone"
    Assert-Holds $firstClaim $shared "the first room must still claim after a refused second room"
    Assert-True (@(Get-ChildItem -LiteralPath $secondRuntime -File).Count -eq 0) "a refused second room must keep no capability file"

    & $keeperProvision -RoomDir $second -SourceSecretPath $firstClaim @roomArguments
    Assert-True ((Get-Authority $sharedKeeper "restart_claim") -ceq $shared) "the second room must leave the keeper authority alone"
    Assert-Holds $firstClaim $shared "the first room must still claim after the second room joins"
    Assert-Holds (Join-Path $secondRuntime "restart-capability") $shared "the second room must claim with the shared secret"
    Assert-True (Test-Path -LiteralPath (Join-Path $secondRuntime "omp-keeper.json") -PathType Leaf) "the second room must get its keeper config"

    $third = New-Holder "$run-hugo"
    Assert-Refused { & $keeperProvision -RoomDir $third -SourceSecretPath $stale @roomArguments } "*does not match*" "a third room with a stale source"
    Assert-True ((Get-Authority $sharedKeeper "restart_claim") -ceq $shared) "a stale source must leave the keeper authority alone"
    Assert-True (@(Get-ChildItem -LiteralPath (Join-Path $third ".omp/runtime") -File).Count -eq 0) "a stale source must leave the third room without files"

    Assert-Refused { & $keeperProvision -RoomDir $first @roomArguments } "*already provisioned*" "a repeat room provisioning"
    Assert-True ((Get-Authority $sharedKeeper "restart_claim") -ceq $shared) "a repeat room provisioning must leave the keeper authority alone"
    Write-Host "keeper room provisioner smoke passed"
} finally {
    Remove-Item Env:RESTART_SMOKE_PROVISIONER, Env:RESTART_SMOKE_KEEPER, Env:RESTART_SMOKE_DATABASE -ErrorAction SilentlyContinue
    try {
        Invoke-SmokeSql "DELETE FROM restart.principal_capabilities WHERE principal LIKE '$run-%';" | Out-Null
    } finally {
        Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
    }
}
