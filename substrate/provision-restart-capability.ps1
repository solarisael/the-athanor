# Provision or rotate one restart capability.
#
# WARNING: this script prints no secret. The secret goes only to the holder's
# local runtime file. Run it again to rotate. The old secret stops to work at
# the moment the new hash lands.
#
# What it does:
# 1. Make a 32-byte random secret in lowercase hex.
# 2. Write sha256(secret) to restart.principal_capabilities for
#    (principal, operation class).
# 3. Write the secret to <HolderDir>/<file>.
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
# credentials file at house/state/substrate/.env.
#
# Usage:
#   pwsh substrate/provision-restart-capability.ps1 `
#     -HolderDir C:/Solarisael/keeper
#   pwsh substrate/provision-restart-capability.ps1 `
#     -Principal kodo -OperationClass restart_exit `
#     -HolderDir C:/Solarisael/Obsidian/obsidian/kodo/.omp/runtime

param(
    [string]$Principal = "omp-keeper",
    [ValidateSet("restart_claim", "restart_request", "restart_exit", "restart_verify")]
    [string]$OperationClass = "restart_claim",
    [Parameter(Mandatory = $true)][string]$HolderDir,
    [string]$SubstrateEnv = "C:/Solarisael/Obsidian/obsidian/house/state/substrate/.env",
    [switch]$Remove
)

$ErrorActionPreference = "Stop"

if ($Principal -notmatch '^[a-z0-9]+(-[a-z0-9]+)*$') {
    throw "Principal must be a lowercase slug: $Principal"
}
if (-not (Test-Path $HolderDir)) {
    throw "HolderDir does not exist: $HolderDir"
}
if (-not (Test-Path $SubstrateEnv)) {
    throw "Substrate env file does not exist: $SubstrateEnv"
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
if ($Remove) {
    if (Test-Path $secretPath -PathType Leaf) {
        Remove-Item $secretPath -Force -ErrorAction Stop
    }
    if (Test-Path $secretPath) {
        throw "capability secret could not be removed: $secretPath"
    }
    $sql = "DELETE FROM restart.principal_capabilities WHERE principal = '$Principal' AND operation_class = '$OperationClass';"
    $command = "PGPASSWORD=`$(grep '^PGPASSWORD=' '$envPathWsl' | cut -d= -f2 | tr -d '\r') psql -h 127.0.0.1 -p 5432 -U solarisael -d solarisael_memory -v ON_ERROR_STOP=1 -c `"$sql`""
    wsl -e sh -lc $command | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "capability database removal failed for '$Principal' and '$OperationClass'; the holder secret is already absent"
    }
    Write-Host "principal '$Principal': $OperationClass capability removed"
    return
}

# 1. Mint the secret.
$bytes = [byte[]]::new(32)
[System.Security.Cryptography.RandomNumberGenerator]::Fill($bytes)
$secret = -join ($bytes | ForEach-Object { $_.ToString("x2") })

$sha = [System.Security.Cryptography.SHA256]::Create()
$hashBytes = $sha.ComputeHash([System.Text.Encoding]::ASCII.GetBytes($secret))
$hash = -join ($hashBytes | ForEach-Object { $_.ToString("x2") })

# 2. Upsert the hash. Credentials come from the substrate env file inside WSL;
#    the CRLF strip is load-bearing (Windows-edited env file).
$sql = @"
INSERT INTO restart.principal_capabilities (principal, operation_class, capability_hash)
VALUES ('$Principal', '$OperationClass', '$hash')
ON CONFLICT (principal, operation_class)
DO UPDATE SET capability_hash = EXCLUDED.capability_hash, rotated_at = NOW();
"@
$command = "PGPASSWORD=`$(grep '^PGPASSWORD=' '$envPathWsl' | cut -d= -f2 | tr -d '\r') psql -h 127.0.0.1 -p 5432 -U solarisael -d solarisael_memory -v ON_ERROR_STOP=1 -c `"$sql`""
wsl -e sh -lc $command | Out-Null
if ($LASTEXITCODE -ne 0) {
    throw "capability hash write failed for '$Principal' and '$OperationClass'; the holder file was NOT updated"
}

# 3. Deliver the secret to the holder, only after the hash landed.
Set-Content -Path $secretPath -Value $secret -NoNewline -Encoding ascii

Write-Host "principal '$Principal': $OperationClass capability provisioned; secret at $secretPath"
