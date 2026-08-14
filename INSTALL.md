# Install The Athanor on Windows

The supported ordinary package is one native Windows x64 installer:

```text
The-Athanor-<version>-windows-x64.exe
The-Athanor-<version>-windows-x64.exe.sha256
```

The installed runtime does not require WSL, Python, Bun, Cargo, a Rust toolchain,
or a separately installed PostgreSQL or NATS server. PowerShell, MSVC, Cargo,
and Inno Setup are build-time tools only.

The command examples below name `1.0.0-rc.3` because it is the last immutable
public installer artifact proven on the reference workstation. The active
source version is `0.9.6`; locally installed native versions may be newer than
the last publicly published checksum-paired installer.

## Verify and install

Verify the downloaded installer before elevation:

```powershell
$expected = (Get-Content .\The-Athanor-1.0.0-rc.3-windows-x64.exe.sha256).Split(' ')[0]
$actual = (Get-FileHash .\The-Athanor-1.0.0-rc.3-windows-x64.exe -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw "installer checksum mismatch" }
.\The-Athanor-1.0.0-rc.3-windows-x64.exe
```

The installer requires Administrator elevation. It verifies the SHA-256 and byte
size of every staged component before it changes the active release. It then
initializes the database, applies ordered schema migrations, installs the
`SolarisaelAthanor` Windows service, and waits for managed readiness.

## Installed topology

Product code is immutable per version. Operator data and secrets are separate:

```text
%ProgramFiles%\Solarisael\Athanor\
  bin\athanor-manage.exe
  current.json
  versions\<version>\
    release-manifest.json
    bin\
      athanor-substrate.exe
      athanor-house-delivery.exe
      house-host.exe
      athanor-gui.exe             pinned Godot 4.7.1 runtime
    adapters\omp\
    runtime\
      godot\                      imported client project + GDExtension
      postgresql\                 EnterpriseDB PostgreSQL 18.4-2 + pgvector 0.8.6
      nats\nats-server.exe        NATS Server 2.14.4
%ProgramData%\Solarisael\Athanor\
  config\runtime.json
  secrets\runtime-secrets.json
  data\postgresql\
  data\nats\
  state\host\
  rooms\
  logs\
  backups\
```

OMP receives one stable loader entry at
`%ProgramFiles%\Solarisael\Athanor\bin\athanor-omp-loader.ts`. The loader follows
`current.json`, so upgrades and rollback never leave a version-pinned extension
path. Its per-user client projection lives at
`%USERPROFILE%\.omp\agent\athanor\client.json` with an explicit user-only ACL.
`config.yml` contains no token, password, or database URL.

`current.json` is the atomic activation pointer. At least the active and previous
versions are retained for rollback. `%ProgramData%\Solarisael\Athanor` and the
secret file have inherited access removed and grant full control only to SYSTEM
and the local Administrators group. Tokens and database passwords are generated
from the operating-system random source, are never printed, and live only in the
restricted secret file.

## Managed service contract

The Rust Windows service owns only child processes it starts. Startup remains
`START_PENDING` and reports checkpoints while dependencies become ready:

1. managed PostgreSQL on `127.0.0.1:5432` (omitted in external database mode);
2. NATS JetStream on `127.0.0.1:4222`;
3. boat delivery after PostgreSQL authority and NATS are ready;
4. one Host per configured room on its declared loopback port (`8787`, `8788`,
   and upward by convention).

The service reports `RUNNING` only after all configured children pass their
readiness contract. On SCM Stop or Shutdown it drains in reverse dependency
order: Host, delivery, NATS, PostgreSQL. Each owned child has a bounded stop
window. Only the verified child handle retained by the supervisor may be
hard-killed after that window; unrelated processes are never selected by name.

Run the native doctor from an elevated terminal:

```powershell
& "$env:ProgramFiles\Solarisael\Athanor\bin\athanor-manage.exe" doctor
```

Doctor exits nonzero if the activation pointer, release manifest, service, or
persistent data root is absent or invalid.

The Start menu contains **The Athanor**, which asks the native manager to launch
the pinned Godot client with the default room's token/identity in child-process
environment, and **Athanor Doctor** for lifecycle diagnostics.

## Existing Solarisael House mode

An existing House must use its current PostgreSQL authority rather than starting
a second server on the same port. Supply both an external database secret file
and a non-secret House topology file:

```json
{
  "houseId": "solarisael",
  "roomsRoot": "C:/Solarisael/Obsidian/obsidian",
  "operatorStateRoot": "C:/Solarisael/Obsidian/obsidian/house/state",
  "defaultRoom": "kintsu",
  "rooms": [
    { "room": "kintsu", "spirit": "Kintsu", "port": 8787 },
    { "room": "kodo", "spirit": "Kodo", "port": 8788 }
  ]
}
```

```powershell
.\The-Athanor-1.0.0-rc.3-windows-x64.exe `
  /EXTERNALDATABASEFILE="$env:TEMP\athanor-database-url.txt" `
  /HOUSECONFIGFILE="$env:TEMP\athanor-house.json"
```

The installer validates every room-state file before service mutation, takes a
database backup even on first external installation, runs no managed PostgreSQL
child, and registers exactly one OMP loader for the invoking Windows user.

## External PostgreSQL mode

Managed PostgreSQL is the default. External mode is an explicit advanced install
option and still uses the packaged NATS, delivery, Host, substrate, and client.
The external server must be reachable from the service account, provide
PostgreSQL 18 with pgvector 0.8.6 and `pg_trgm`, and dedicate a private role and
database to The Athanor.

Put the complete connection URL in an ACL-restricted temporary file. Do not put a
password on the installer command line:

```powershell
$secret = "$env:TEMP\athanor-external-db.txt"
Set-Content -NoNewline -Encoding utf8 $secret `
  'postgresql://athanor:<password>@db.example.internal:5432/athanor?sslmode=require'
icacls $secret /inheritance:r /grant:r "$env:USERNAME:(R)"
.\The-Athanor-1.0.0-rc.3-windows-x64.exe /EXTERNALDATABASEFILE="$secret"
Remove-Item $secret -Force
```

The installer reads the file, stores the URL in the restricted runtime secret
file, and never logs it. External mode does not create or delete managed
PostgreSQL data. Schema compatibility and migrations remain mandatory.

## Upgrade and rollback

Running a newer installer performs an upgrade:

1. verify the complete new payload before mutation;
2. take a schema-labelled PostgreSQL backup while the current service is healthy;
3. stop the service in dependency order;
4. stage the new immutable version;
5. atomically update `current.json`;
6. initialize or migrate PostgreSQL;
7. start the service and require readiness.

If migration or readiness fails, activation returns to the previous version and
the pre-update database backup is restored. A retained release can also be
selected explicitly:

```powershell
& "$env:ProgramFiles\Solarisael\Athanor\bin\athanor-manage.exe" rollback
```

Rollback also takes a backup first. It restores the newer pointer and database
if the older release cannot become ready.

## Uninstall and purge

Windows uninstall stops and removes the service and product binaries. It
intentionally preserves every file under `%ProgramData%\Solarisael\Athanor`.
Reinstalling can reuse that data after compatibility checks.

Data destruction is a different, explicit command and is never run by Inno
Setup:

```powershell
& "$env:ProgramFiles\Solarisael\Athanor\bin\athanor-manage.exe" purge --confirm-data-loss
```

Export required rooms and backups before purge. Purge removes both product and
persistent data.

## Legacy pre-install door

The native installer recognizes only the bounded legacy directory names
`solarisael-house`, `solarisael-house-omp`, and
`solarisael-house-substrate`. Before first activation it copies a bounded,
cache-excluding backup under
`%ProgramData%\Solarisael\Athanor\backups\legacy-preinstall` and records a
one-time marker. Legacy code is never used as a runtime fallback. Database
migration still requires a valid supported backup and schema lineage; the door
does not silently execute legacy Python, WSL, Bun, or shell tooling.
