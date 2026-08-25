# Provision or rotate the keeper's restart claim capability.
#
# WARNING: this script prints no secret. The secret goes only to the keeper's
# local runtime file. Run it again to rotate; the old secret stops working at
# the moment the new hash lands.
#
# What it does:
# 1. Make a 32-byte random secret (lowercase hex).
# 2. Write sha256(secret) to restart.principal_capabilities for
#    (principal, restart_claim).
# 3. Write the secret to <KeeperDir>/restart-capability.
#
# The principal is a lowercase slug, like a room key. The keeper is a
# first-class principal and never impersonates a room.
#
# Requires: migration 0026 applied; WSL psql; the substrate credentials file at
# house/state/substrate/.env.
#
# Usage:
#   pwsh substrate/provision-restart-capability.ps1 `
#     -KeeperDir C:/Solarisael/keeper

param(
    [string]$Principal = "omp-keeper",
    [Parameter(Mandatory = $true)][string]$KeeperDir,
    [string]$SubstrateEnv = "C:/Solarisael/Obsidian/obsidian/house/state/substrate/.env"
)

$ErrorActionPreference = "Stop"

if ($Principal -notmatch '^[a-z0-9]+(-[a-z0-9]+)*$') {
    throw "Principal must be a lowercase slug: $Principal"
}
if (-not (Test-Path $KeeperDir)) {
    throw "KeeperDir does not exist: $KeeperDir"
}
if (-not (Test-Path $SubstrateEnv)) {
    throw "Substrate env file does not exist: $SubstrateEnv"
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
$envPathWsl = (wsl -e wslpath -a ($SubstrateEnv -replace "\\", "/")).Trim()
$sql = @"
INSERT INTO restart.principal_capabilities (principal, operation_class, capability_hash)
VALUES ('$Principal', 'restart_claim', '$hash')
ON CONFLICT (principal, operation_class)
DO UPDATE SET capability_hash = EXCLUDED.capability_hash, rotated_at = NOW();
"@
$command = "PGPASSWORD=`$(grep '^PGPASSWORD=' '$envPathWsl' | cut -d= -f2 | tr -d '\r') psql -h 127.0.0.1 -p 5432 -U solarisael -d solarisael_memory -v ON_ERROR_STOP=1 -c `"$sql`""
wsl -e sh -lc $command | Out-Null
if ($LASTEXITCODE -ne 0) {
    throw "capability hash write failed for principal '$Principal'; the keeper file was NOT updated"
}

# 3. Deliver the secret to the keeper, only after the hash landed.
$secretPath = Join-Path $KeeperDir "restart-capability"
Set-Content -Path $secretPath -Value $secret -NoNewline -Encoding ascii

Write-Host "principal '$Principal': restart_claim capability provisioned; secret at $secretPath"
