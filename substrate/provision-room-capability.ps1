# Provision or rotate one room's Docket write capability.
#
# WARNING: this script prints no secret. The secret goes only to the room's
# local runtime file. Run it again to rotate; the old secret stops working
# at the moment the new hash lands.
#
# What it does:
# 1. Make a 32-byte random secret (lowercase hex).
# 2. Write sha256(secret) to docket.room_capabilities for (room, docket_write).
# 3. Write the secret to <RoomDir>/.omp/runtime/room-capability.
#
# Requires: migrations 0023 and 0024 applied; WSL psql; the substrate
# credentials file at house/state/substrate/.env.
#
# Usage:
#   pwsh substrate/provision-room-capability.ps1 -Room tuner `
#     -RoomDir C:/Solarisael/Obsidian/obsidian/tuner

param(
    [Parameter(Mandatory = $true)][string]$Room,
    [Parameter(Mandatory = $true)][string]$RoomDir,
    [string]$SubstrateEnv = "C:/Solarisael/Obsidian/obsidian/house/state/substrate/.env"
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $RoomDir)) {
    throw "RoomDir does not exist: $RoomDir"
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
INSERT INTO docket.room_capabilities (room, operation_class, capability_hash)
VALUES ('$Room', 'docket_write', '$hash')
ON CONFLICT (room, operation_class)
DO UPDATE SET capability_hash = EXCLUDED.capability_hash, rotated_at = NOW();
"@
$command = "PGPASSWORD=`$(grep '^PGPASSWORD=' '$envPathWsl' | cut -d= -f2 | tr -d '\r') psql -h 127.0.0.1 -p 5432 -U solarisael -d solarisael_memory -v ON_ERROR_STOP=1 -c `"$sql`""
wsl -e sh -lc $command | Out-Null
if ($LASTEXITCODE -ne 0) {
    throw "capability hash write failed for room '$Room'; the room file was NOT updated"
}

# 3. Deliver the secret to the room, only after the hash landed.
$runtimeDir = Join-Path $RoomDir ".omp/runtime"
New-Item -ItemType Directory -Force -Path $runtimeDir | Out-Null
$secretPath = Join-Path $runtimeDir "room-capability"
Set-Content -Path $secretPath -Value $secret -NoNewline -Encoding ascii

Write-Host "room '$Room': docket_write capability provisioned; secret at $secretPath"
