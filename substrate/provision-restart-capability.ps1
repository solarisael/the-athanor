# Provision, share, or rotate one restart capability.
#
# WARNING: this script prints no secret and takes none as an argument. A secret
# moves only from a holder file into a holder file.
#
# restart.principal_capabilities keeps one sha256 per (principal, operation
# class). Every holder of that pair holds the same secret. restart_claim is
# shared, because each room's keeper presents the one omp-keeper secret. A new
# hash for it invalidates every holder at once, so provisioning one holder never
# mints over an authority that already exists.
#
# What a run does:
# - No authority row: make a 32-byte random secret in lowercase hex, insert its
#   sha256 only while the row is still absent, then write the holder file.
# - An authority row exists: take the secret from -SourceSecretPath, or else
#   from the holder file itself. If its sha256 matches the row, write it to the
#   holder file. The row never changes, so a repeat run changes nothing. A
#   missing or stale secret refuses and changes nothing.
# - -Rotate mints a fresh secret and replaces the authority of one room class.
#   The room is the only holder. Its old secret stops working the moment the new
#   hash lands. -Rotate refuses restart_claim: nothing re-keys every keeper
#   holder at once, so this script never rotates the shared secret.
# - -Remove deletes the holder file, then the authority row. The restart_claim
#   row is shared, so removing it revokes every keeper holder.
#
# The principal is a lowercase slug, like a room key. The keeper is a
# first-class principal and never impersonates a room.
#
# The plane has four classes. The keeper holds restart_claim. The room holds
# restart_request to ask, restart_exit to arm the exit, and restart_verify to
# sign the successor. The restart intent id is public, so each door needs its
# own secret.
#
# Requires: migration 0026 applied. Requires WSL psql. Requires the substrate
# credentials file at house/state/substrate/.env. -DatabaseName selects another
# database on the same server, for a disposable smoke.
#
# Usage:
#   The first keeper holder mints the shared secret:
#   pwsh substrate/provision-restart-capability.ps1 `
#     -HolderDir C:/Solarisael/Obsidian/obsidian/kodo/.omp/runtime
#   Every later keeper holder copies it from a holder whose secret works:
#   pwsh substrate/provision-restart-capability.ps1 `
#     -SourceSecretPath C:/Solarisael/Obsidian/obsidian/kodo/.omp/runtime/restart-capability `
#     -HolderDir C:/Solarisael/Obsidian/obsidian/kintsu/.omp/runtime
#   One room key is minted, or checked if it exists. Add -Rotate to replace it:
#   pwsh substrate/provision-restart-capability.ps1 `
#     -Principal kodo -OperationClass restart_exit `
#     -HolderDir C:/Solarisael/Obsidian/obsidian/kodo/.omp/runtime

param(
    [string]$Principal = "omp-keeper",
    [ValidateSet("restart_claim", "restart_request", "restart_exit", "restart_verify")]
    [string]$OperationClass = "restart_claim",
    [Parameter(Mandatory = $true)][string]$HolderDir,
    [string]$SourceSecretPath,
    [string]$SubstrateEnv = "C:/Solarisael/Obsidian/obsidian/house/state/substrate/.env",
    [string]$DatabaseName = "solarisael_memory",
    [switch]$Rotate,
    [switch]$Remove
)

$ErrorActionPreference = "Stop"

if ($Principal -notmatch '^[a-z0-9]+(-[a-z0-9]+)*$') {
    throw "Principal must be a lowercase slug: $Principal"
}
if ($DatabaseName -cnotmatch '^[a-z_][a-z0-9_]*$') {
    throw "DatabaseName must be a plain lowercase PostgreSQL name: $DatabaseName"
}
if (-not (Test-Path $HolderDir)) {
    throw "HolderDir does not exist: $HolderDir"
}
if (-not (Test-Path $SubstrateEnv)) {
    throw "Substrate env file does not exist: $SubstrateEnv"
}
$hasSource = -not [string]::IsNullOrWhiteSpace($SourceSecretPath)
if ($Remove -and ($Rotate -or $hasSource)) {
    throw "-Remove takes neither -Rotate nor -SourceSecretPath"
}
if ($Rotate -and $hasSource) {
    throw "-Rotate mints a fresh secret and takes no -SourceSecretPath"
}
if ($Rotate -and $OperationClass -eq "restart_claim") {
    throw "restart_claim is shared by every keeper holder. A new secret would invalidate all of them, and nothing re-keys every holder at once. Nothing was changed. To give this holder the existing secret, use -SourceSecretPath."
}
if ($hasSource -and -not (Test-Path -LiteralPath $SourceSecretPath -PathType Leaf)) {
    throw "SourceSecretPath is not a file: $SourceSecretPath"
}

# The keeper crate reads the file name 'restart-capability'; only the newer
# room classes get a class-named file.
$fileName = if ($OperationClass -eq "restart_claim") {
    "restart-capability"
} else {
    ($OperationClass -replace "_", "-") + "-capability"
}

$secretPath = Join-Path $HolderDir $fileName
$envPathWsl = (wsl -e wslpath -a ($SubstrateEnv -replace "\\", "/")).Trim()

# Run one statement through WSL psql and return its result rows, unaligned.
# Credentials come from the substrate env file inside WSL. The CRLF strip is
# load-bearing, because the env file is edited on Windows.
function Invoke-CapabilitySql([string]$Sql, [string]$Failure) {
    $command = "PGPASSWORD=`$(grep '^PGPASSWORD=' '$envPathWsl' | cut -d= -f2 | tr -d '\r') psql -X -q -A -t -h 127.0.0.1 -p 5432 -U solarisael -d $DatabaseName -v ON_ERROR_STOP=1 -c `"$Sql`""
    $rows = @(wsl -e sh -lc $command)
    if ($LASTEXITCODE -ne 0) {
        throw $Failure
    }
    $rows | ForEach-Object { "$_".Trim() } | Where-Object { $_ }
}

if ($Remove) {
    if (Test-Path $secretPath -PathType Leaf) {
        Remove-Item $secretPath -Force -ErrorAction Stop
    }
    if (Test-Path $secretPath) {
        throw "capability secret could not be removed: $secretPath"
    }
    $sql = "DELETE FROM restart.principal_capabilities WHERE principal = '$Principal' AND operation_class = '$OperationClass';"
    Invoke-CapabilitySql $sql "capability database removal failed for '$Principal' and '$OperationClass'; the holder secret is already absent" | Out-Null
    Write-Host "principal '$Principal': $OperationClass capability removed"
    return
}

function New-CapabilitySecret {
    $bytes = [byte[]]::new(32)
    [System.Security.Cryptography.RandomNumberGenerator]::Fill($bytes)
    -join ($bytes | ForEach-Object { $_.ToString("x2") })
}

# The authority compares sha256 over the UTF-8 bytes it is shown.
function Get-CapabilityHash([string]$Secret) {
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        -join ($sha.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($Secret)) | ForEach-Object { $_.ToString("x2") })
    } finally {
        $sha.Dispose()
    }
}

# The Host, the keeper, and the adapter all trim the holder file. The secret is
# therefore the trimmed text, and a blank file holds no secret.
function Read-HolderSecret([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $null
    }
    $secret = ([string](Get-Content -LiteralPath $Path -Raw)).Trim()
    if ($secret) { $secret } else { $null }
}

if ($Rotate) {
    $secret = New-CapabilitySecret
    $hash = Get-CapabilityHash $secret
    $sql = @"
INSERT INTO restart.principal_capabilities (principal, operation_class, capability_hash)
VALUES ('$Principal', '$OperationClass', '$hash')
ON CONFLICT (principal, operation_class)
DO UPDATE SET capability_hash = EXCLUDED.capability_hash, rotated_at = NOW();
"@
    Invoke-CapabilitySql $sql "capability hash write failed for '$Principal' and '$OperationClass'; the holder file was NOT updated" | Out-Null
    # Deliver the secret to the holder only after the hash has landed.
    Set-Content -Path $secretPath -Value $secret -NoNewline -Encoding ascii
    Write-Host "principal '$Principal': $OperationClass capability rotated; secret at $secretPath"
    return
}

$sql = "SELECT capability_hash FROM restart.principal_capabilities WHERE principal = '$Principal' AND operation_class = '$OperationClass';"
$authority = @(Invoke-CapabilitySql $sql "capability authority read failed for '$Principal' and '$OperationClass'; nothing was changed" | Where-Object { $_ -cmatch '^[0-9a-f]{64}$' })
if ($authority.Count -gt 1) {
    throw "capability authority read for '$Principal' and '$OperationClass' returned more than one row; nothing was changed"
}

if ($authority.Count -eq 1) {
    $repair = if ($OperationClass -eq "restart_claim") {
        "Pass -SourceSecretPath with the restart-capability file of a keeper holder whose secret still works, for example another room's .omp/runtime/restart-capability. This script never replaces the shared secret."
    } else {
        "Pass -SourceSecretPath with a $fileName file whose secret still works, or run again with -Rotate to replace this room key."
    }
    $candidatePath = if ($hasSource) { $SourceSecretPath } else { $secretPath }
    $candidate = Read-HolderSecret $candidatePath
    if (-not $candidate) {
        throw "principal '$Principal' already has a $OperationClass authority, and $candidatePath holds no secret. Nothing was changed. $repair"
    }
    if ((Get-CapabilityHash $candidate) -cne $authority[0]) {
        throw "the secret at $candidatePath does not match the $OperationClass authority of principal '$Principal'. Nothing was changed. $repair"
    }
    if ((Read-HolderSecret $secretPath) -ceq $candidate) {
        Write-Host "principal '$Principal': the $OperationClass holder at $secretPath already matches its authority; nothing was changed"
        return
    }
    Set-Content -Path $secretPath -Value $candidate -NoNewline -Encoding utf8NoBOM
    Write-Host "principal '$Principal': $OperationClass secret copied to $secretPath; the authority is unchanged"
    return
}

if ($hasSource) {
    throw "principal '$Principal' has no $OperationClass authority to check $SourceSecretPath against. Nothing was changed. To mint the first secret, run without -SourceSecretPath."
}

# No authority yet: mint the first secret. The insert leaves any row that
# appeared since the read untouched.
$secret = New-CapabilitySecret
$hash = Get-CapabilityHash $secret
$sql = @"
INSERT INTO restart.principal_capabilities (principal, operation_class, capability_hash)
VALUES ('$Principal', '$OperationClass', '$hash')
ON CONFLICT (principal, operation_class) DO NOTHING
RETURNING capability_hash;
"@
$inserted = @(Invoke-CapabilitySql $sql "capability hash write failed for '$Principal' and '$OperationClass'; the holder file was NOT updated")
if ($inserted -cnotcontains $hash) {
    throw "principal '$Principal' gained a $OperationClass authority while this run was minting. Nothing was changed. Run again to share that secret."
}
# Deliver the secret to the holder only after the hash has landed. If delivery
# fails, withdraw exactly the row this run inserted, so no holder is left
# without a working secret.
try {
    Set-Content -Path $secretPath -Value $secret -NoNewline -Encoding ascii
} catch {
    $deliveryFailure = $_.Exception.Message
    $sql = "DELETE FROM restart.principal_capabilities WHERE principal = '$Principal' AND operation_class = '$OperationClass' AND capability_hash = '$hash';"
    Invoke-CapabilitySql $sql "the holder file could not be written ($deliveryFailure), and withdrawing the minted authority for '$Principal' and '$OperationClass' also failed" | Out-Null
    throw "the holder file could not be written ($deliveryFailure); the authority this run minted was withdrawn"
}

Write-Host "principal '$Principal': $OperationClass capability minted; secret at $secretPath"
