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

Product code is immutable per version. The OMP adapter is an independently
installed immutable component. Operator data and secrets are separate:

```text
%ProgramFiles%\Solarisael\Athanor\
  bin\
    athanor-manage.exe
    athanor-omp-loader.ts          stable product-owned loader
  current.json                     native product activation pointer
  versions\<version>\
    release-manifest.json
    bin\
      athanor-substrate.exe
      athanor-house-delivery.exe
      house-host.exe
      athanor-gui.exe              pinned Godot 4.7.1 runtime
    runtime\
      godot\                        imported client project + GDExtension
      postgresql\                   EnterpriseDB PostgreSQL 18.4-2 + pgvector 0.8.6
      nats\nats-server.exe          NATS Server 2.14.4
  components\omp-adapter\
    current.json                    component activation pointer
    versions\<releaseId>\
      component-manifest.json
      index.ts and other adapter artifacts
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

The stable loader reads native `current.json` only to obtain the native runtime
environment. It reads `components\omp-adapter\current.json` to select imports.
The component pointer has `format`, `releaseId`, and `previousReleaseId` fields.
It does not select a native product version.

The manager validates the component manifest, its release identity, every
artifact size and SHA-256 value, and all four native compatibility fields before
it activates an adapter. The loader repeats those checks before it imports the
adapter. A failed check refuses the adapter; it does not load a version-pinned
copy from the native payload.

Native `current.json` is the atomic product activation pointer. The component
pointer is a separate atomic adapter activation pointer. At least the active and
previous product and adapter releases are retained for their own rollback paths.
`%ProgramData%\Solarisael\Athanor` and the secret file have inherited access
removed and grant full control only to SYSTEM and the local Administrators group.
Tokens and database passwords are generated from the operating-system random
source, are never printed, and live only in the restricted secret file.

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

Doctor exits nonzero if a native or component activation pointer, a native
release manifest, a component manifest identity, a component artifact, native
compatibility, the service, or the persistent data root is absent or invalid.

## Adapter component operations

The manager is the only installed writer. To install a prebuilt adapter
component, run:

```powershell
& "$env:ProgramFiles\Solarisael\Athanor\bin\athanor-manage.exe" `
  install-omp-adapter --source <component-root>
```

`<component-root>` must directly contain `component-manifest.json` and every
declared artifact. The manager stages and verifies the component before it
renames the release directory and atomically updates the component pointer.

To roll back the adapter only, run:

```powershell
& "$env:ProgramFiles\Solarisael\Athanor\bin\athanor-manage.exe" `
  rollback-omp-adapter [--release-id <releaseId>]
```

The manager refuses an adapter release that is incompatible with the active
native manifest. These commands never change a native product version.

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
`solarisael-house`, `athanor-omp`, and
`athanor-substrate`. Before first activation it copies a bounded,
cache-excluding backup under
`%ProgramData%\Solarisael\Athanor\backups\legacy-preinstall` and records a
one-time marker. Legacy code is never used as a runtime fallback. Database
migration still requires a valid supported backup and schema lineage; the door
does not silently execute legacy Python, WSL, Bun, or shell tooling.
