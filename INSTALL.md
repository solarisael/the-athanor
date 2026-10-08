# Install The Athanor on Windows

The supported package is one native Windows x64 installer and its checksum file
(`installer/athanor.iss:22`; `.github/workflows/release.yml:46-50,66-74`):

```text
The-Athanor-<version>-windows-x64.exe
The-Athanor-<version>-windows-x64.exe.sha256
```

[Limitations § Supported install](./docs/LIMITATIONS.md#1-supported-install)
lists the supported platforms. [Architecture](./docs/ARCHITECTURE.md) describes
the processes that the installer places.

## What the runtime needs

The payload bundles PostgreSQL 18.4-2 with pgvector 0.8.6 and NATS Server 2.14.4
(`installer/dependencies.json:1-28`). The runtime needs no WSL, Python, Cargo,
Rust toolchain, or separate PostgreSQL or NATS server
(`scripts/build-native-release.ps1:146-167`). PowerShell, MSVC, Cargo, and Inno
Setup are build-time tools only (`release.yml:18-50`).

The runtime needs no Bun only if you do not use Pulse. The release build
compiles only `athanor-install`, `akasha`, and `host`, and the payload has no
GUI (`build-native-release.ps1:136,146-167`). The web Pulse runs under Bun from
a source checkout. The `pulse.exe` window is a separate build
(`scripts/install-pulse.ps1:18-61`).

## Authenticated broker rollout

Delivery API 2 provides authenticated NATS clients.
The Host API 2 cutover additionally requires matching native and OMP adapter components.

Close the House Host and every OMP session before activating this release.
The canonical deployment driver refuses an active Host listener or installed substrate child.
It does not terminate user sessions.

Run `substrate/deploy-local.ps1` from the verified source checkout.
If you use a deployment checkout, synchronize the verified source before running the driver.
Do not deploy an older checkout over the installed repairs.

The driver supplies the existing OMP configuration, private client path, and operator principal to the installer.
The installer preserves credentials across later updates.
It writes `secrets/nats-server.conf` and protects the client projections.
The Host now publishes Hallway pointers, so deployment must restart the broker with the new permission.
Start the matching Host before reopening OMP.
The loader refuses an older running Host when the selected component requires API 2.

Warning: a delivery API 2 installer refuses installation or rollback to an unauthenticated delivery API 1 release.
The first authenticated release needs another authenticated generation for normal rollback.
Database backups do not make an old broker authentication-capable.

## Versions

The product source version is `0.5.4`, and the OMP component version is `0.10.0`.
Local native builds add a timestamp and source revision to the product version.
Read `current.json` and the component pointer for the selected installed versions.
Do not infer installed state from source versions or older documentation.
The examples below use `<version>`.

## Verify and install

Verify the downloaded installer before you elevate:

```powershell
$expected = (Get-Content .\The-Athanor-<version>-windows-x64.exe.sha256).Split(' ')[0]
$actual = (Get-FileHash .\The-Athanor-<version>-windows-x64.exe -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw "installer checksum mismatch" }
.\The-Athanor-<version>-windows-x64.exe
```

**Not re-verified at a6ab453:** the `.sha256` file format. The snippet takes
the first token as lowercase hex. `.github/workflows/release.yml:38-50` writes
the file.

The installer requires Administrator (`installer/athanor.iss:17`). Inno Setup
copies `bin\athanor.exe` and the loader, then runs `athanor.exe install`
(`athanor.iss:27-29,82-99`). The install verifies the size and SHA-256 of every
staged component before it writes `current.json`
(`crates/athanor-install/src/installer.rs:303-305,329,1097-1105`). Inno Setup
replaces `bin\athanor.exe` and the loader before that check
(`athanor.iss:27-28`). The install then migrates the database, installs and
starts the `SolarisaelAthanor` service, and waits for readiness
(`installer.rs:333-340`).

## Installed topology

`P` is `%ProgramFiles%\Solarisael\Athanor`. `D` is
`%ProgramData%\Solarisael\Athanor` (`crates/athanor-install/src/layout.rs:4,22-27`).
Product code is immutable per version. The OMP adapter is a separate immutable
component. Data is separate (`installer.rs:289-305,642-690`).

```text
P\
  bin\
    athanor.exe                the one exe: Host, service, installer, keeper, chat, status, start
    athanor-omp-loader.ts      the stable OMP loader
  current.json                 version, previousVersion, rollbackBackup
  versions\<version>\
    release-manifest.json
    compatibility.json
    bin\                       athanor.exe, athanor-substrate.exe, athanor-omp-loader.ts
    runtime\postgresql\        PostgreSQL 18.4-2 + pgvector 0.8.6
    runtime\nats\              nats-server.exe 2.14.4
    components\omp-keeper\
    components\omp-adapter\
  components\omp-adapter\
    current.json               format, releaseId, previousReleaseId
    versions\<releaseId>\      component-manifest.json and adapter artifacts
D\
  config\runtime.json          written by the installer
  config\harnesses.json        read by the Host, never written by the installer
  secrets\runtime-secrets.json
  data\postgresql\             managed mode only
  data\nats\
  state\host\<room>\
  substrate\
  logs\
  backups\
%USERPROFILE%\.omp\agent\
  config.yml                   OMP config; the installer adds the stable loader
  athanor\client.json          client projection with a copy of hostToken
<roomsRoot>\<room>\
  .omp\runtime\athanor-house-state.json
  .omp\runtime\omp-keeper.json
```

| Path | Facts | Evidence |
|---|---|---|
| `P\bin\`, `P\current.json` | stable binaries and the native activation pointer | `installer.rs:43-48,307-318,867-871`; `athanor.iss:27-28` |
| `P\versions\<version>\` | the verified payload | `build-native-release.ps1:146-167,194`; `installer.rs:289-305` |
| `D\config\runtime.json` | `databaseMode`, `databaseHost`, `databasePort`, `natsHost`, `natsPort`, `hostPort`, `schemaVersion`, `houseId`, `roomsRoot`, `operatorStateRoot`, `defaultRoom`, `rooms`, `ompConfigPath`, `clientConfigPath` | `installer.rs:1229-1250` |
| `D\config\harnesses.json` | `ATHANOR_HARNESS_REGISTRY` overrides the path. A missing file means zero harnesses. A malformed file is refused. | `crates/athanor-install/src/harness/config.rs:16,188-198,247-252` |
| `D\secrets\runtime-secrets.json` | `hostToken`, `postgresPassword`, `externalDatabaseUrl` | `installer.rs:1254-1286` |
| `D\data\postgresql\` | created only without an external database | `installer.rs:275-277` |
| `D\state\host\<room>\` | one folder per configured room | `installer.rs:272-273` |
| `D\substrate\` | present on the reference workstation | installed root listing |
| `config.yml` | the installer registers the stable loader; uninstall removes it | `athanor.iss:71`; `installer.rs:1343-1347,1073-1080` |
| `client.json` | format 2: `houseId`, `hostToken`, `stateRoot`, `hostUrl`, `defaultRoom`, `rooms`; operator-user ACL | `athanor.iss:72`; `installer.rs:1316-1340` |
| `athanor-house-state.json` | written only if missing, and only when `roomsRoot` is `D\rooms` | `installer.rs:240-271` |
| `omp-keeper.json` | the keeper config that each harness names | `crates/athanor-install/src/cli/keeper.rs:30-39` |

TODO(census): which code writes `D\substrate\`, and what does it hold?

Rooms live under `roomsRoot`. Only the default House uses `D\rooms`
(`installer.rs:1174-1183`). The reference House sets `roomsRoot` to
`C:/Solarisael/Obsidian/obsidian` and has no `D\rooms\`. Read
[Architecture § Installed layout](./docs/ARCHITECTURE.md#7-installed-layout) for
every field.

### Adapter pointer and loader

The stable loader reads native `current.json` only to build the native runtime
environment (`adapters/omp/installed-loader.ts:566-599,628-634`). It reads the
component pointer to select imports (`installed-loader.ts:645-663`). The
component pointer has `format`, `releaseId`, and `previousReleaseId` fields. It
selects no native version (`crates/athanor-install/src/component.rs:42-46`).

The manager checks the component manifest, its release identity, every artifact
size and SHA-256, and four compatibility fields before it activates an adapter
(`installer.rs:466-467`; `component.rs:8-46,101-177`). The loader repeats those
checks before import and refuses a failed adapter
(`installed-loader.ts:291-302,391-406,600-615`). Both pointers change by atomic
write (`installer.rs:483-486,1214-1215`). At least the active and previous
product and adapter releases are kept (`installer.rs:705-713,749-761`;
`crates/athanor-install/src/manifest.rs:52-55`).

### Secrets and access

Warning: the Host bearer is not only in the secret file.

The installer copies `hostToken` into `client.json` with an operator-user ACL
(`installer.rs:1317-1341`). The loader exports it to OMP as
`ATHANOR_HOST_TOKEN` (`installed-loader.ts:628-634`). `D` and the secret file
have inherited access removed. SYSTEM and Administrators get full control
(`crates/athanor-install/src/boundaries.rs:353-381`;
`installer.rs:239,1259-1261,1286`). Tokens and passwords come from the OS random
source (`installer.rs:1263-1272`). Install output carries only the version and
flags (`crates/athanor-install/src/cli/manage.rs:90-95`).

## The service and the Host

The `SolarisaelAthanor` service owns only the children it starts
(`crates/athanor-install/src/service.rs:264-308`;
`crates/athanor-install/src/supervisor.rs:41-55`). It stays `START_PENDING`
while each child becomes ready:

1. managed PostgreSQL on `127.0.0.1:5432`, in managed mode only
   (`supervisor.rs:427-441`);
2. NATS JetStream on `127.0.0.1:4222` (`supervisor.rs:443-457`).

The service reports `RUNNING` after both children pass readiness
(`supervisor.rs:334-382`). There is no delivery child. Boat delivery is a NATS
`DeliveryService` task inside the Host process
(`crates/host/src/house.rs:19,171-181,235-248`). Stop runs in reverse order:
request a stop, wait 30 s, then kill the verified handle
(`supervisor.rs:384-414`).

`athanor.exe` with no arguments starts the Host as a user process
(`crates/athanor-install/src/cli/mod.rs:38-57`;
`crates/athanor-install/src/app.rs:85-134`). The Host:

1. starts the service if it is stopped, and waits up to 90 s for `RUNNING` and
   both ports (`service.rs:40-111`);
2. reads `harnesses.json` once;
3. binds one listener on `127.0.0.1:<hostPort>` for every room
   (`app.rs:30-58`; `house.rs:86-105,166-202`);
4. starts each `autoStart` harness in file order and collects failures without
   stopping (`app.rs:104-113`).

When the Host exits, it stops the harnesses it started (`app.rs:60-75`). A
harness is usually a keeper, and the keeper starts OMP
(`crates/omp-keeper/src/keeper.rs:188,812`). Both installed harnesses have
`autoStart: true`. Read [Architecture § The keeper](./docs/ARCHITECTURE.md#5-the-keeper)
for the restart loop.

One port serves the whole House:

- `RuntimeConfig.host_port` owns the port (`supervisor.rs:256`; `app.rs:30-58`).
  The installer always writes `DEFAULT_HOST_WS_PORT` (`installer.rs:1235`). The
  installed value is `8787`.
- Every room route starts with `/room/<room-key>`
  (`crates/protocol/src/contract.rs:25`).
- `HostRoomConfig` holds only `room` and `spirit` (`supervisor.rs:243-246`).
- `protocol::is_safe_room_key` owns the room-key grammar (`supervisor.rs:308`;
  `crates/host/src/config.rs:87-89`).
- The installer converts the old per-room port format (`installer.rs:104-144`).
- The adapter gets one base URL and adds the room path
  (`adapters/omp/house-proof/host.ts:4,77-97`).
- All rooms share one PostgreSQL pool and one `DATABASE_URL`
  (`house.rs:19,171-181,205-234`).

The Start menu has **The Athanor** and **Athanor Doctor** (`athanor.iss:35-37`).
**The Athanor** runs `athanor.exe`, which starts the Host.

## Doctor

Run the native doctor:

```powershell
& "$env:ProgramFiles\Solarisael\Athanor\bin\athanor.exe" doctor
```

**Not re-verified at a6ab453:** whether the doctor needs an elevated terminal.
`crates/athanor-install/src/lib.rs` decides it.

The doctor checks `current-pointer`, `release-manifest`, `artifact-checksums`,
`omp-adapter-pointer`, `omp-adapter-integrity`, `omp-adapter-compatibility`,
`omp-adapter-previous-release`, `windows-service`, and `persistent-data`
(`lib.rs:58-239`). It exits nonzero when a check fails (`manage.rs:116-118`).

## Pulse

Warning: Pulse writes. `chat/say` appends an operator line to the room chat
(`gui-prototype/live-routes.json:2-16`; `crates/host/src/server.rs:934-939`).

From the repository root, run:

```powershell
bun gui-prototype/serve.ts
```

Open `http://127.0.0.1:4175`. `PULSE_PORT` selects another port
(`gui-prototype/serve.ts:34-36,44`). One process serves one room: `PULSE_ROOM`,
default `kodo` (`serve.ts:34`). The proxy reads the Host through loopback and
adds the bearer (`serve.ts:44,58-65`). Read
[Limitations § Network](./docs/LIMITATIONS.md#6-network) before any network
exposure.

## Adapter component operations

To install a prebuilt adapter component, run:

```powershell
& "$env:ProgramFiles\Solarisael\Athanor\bin\athanor.exe" `
  install-omp-adapter --source <component-root>
```

`<component-root>` must directly contain `component-manifest.json` and every
declared artifact (`installer.rs:466`; `component.rs:8-46`). The manager stages
and verifies the component. Then it renames the release folder and flips the
pointer (`installer.rs:478-486,642-690`).

To roll back the adapter only, run:

```powershell
& "$env:ProgramFiles\Solarisael\Athanor\bin\athanor.exe" `
  rollback-omp-adapter [--release-id <releaseId>]
```

The manager refuses an adapter that is incompatible with the active native
manifest (`installer.rs:467`). These commands write only the component pointer.
They never change the native version (`installer.rs:460-489`;
`manage.rs:97-111`).

## Existing House mode

An existing House can keep its own PostgreSQL. Supply an external database file
and a House topology file. Setup passes them as `--external-database-file` and
`--house-config-file` (`athanor.iss:40-66`; `manage.rs:44-65`). A named file
that does not exist stops setup (`athanor.iss:48-49,62-63`).

The topology file is `HouseInstallConfig`. It accepts these five fields and
refuses any other field (`installer.rs:50-57`; `manage.rs:61`):

| Field | Content |
|---|---|
| `houseId` | the House id |
| `roomsRoot` | the folder that holds the rooms |
| `operatorStateRoot` | the operator state folder |
| `defaultRoom` | the room key for default routing |
| `rooms` | a list of `{ "room": <key>, "spirit": <name> }` |

Do not put `hostPort` in the file. The installer refuses it and writes the port
itself (`installer.rs:1235`).

```json
{
  "houseId": "solarisael",
  "roomsRoot": "C:/Solarisael/Obsidian/obsidian",
  "operatorStateRoot": "C:/Solarisael/Obsidian/obsidian/house/state",
  "defaultRoom": "kintsu",
  "rooms": [
    { "room": "kintsu", "spirit": "Kintsu" },
    { "room": "kodo", "spirit": "Kodo" }
  ]
}
```

```powershell
.\The-Athanor-<version>-windows-x64.exe `
  /EXTERNALDATABASEFILE="$env:TEMP\athanor-database-url.txt" `
  /HOUSECONFIGFILE="$env:TEMP\athanor-house.json"
```

The installer checks every room-state file before it stops the service
(`installer.rs:240-250,286-287`). When `roomsRoot` is not `D\rooms`, each room
must already have `.omp\runtime\athanor-house-state.json`
(`installer.rs:244-249`). The installer takes a database backup, also on a
first external install (`installer.rs:320-322`). The service starts no
PostgreSQL child (`supervisor.rs:427`). The installer registers the stable
loader in the invoking user's OMP `config.yml` (`athanor.iss:68-74`;
`installer.rs:1343-1347`).

## External PostgreSQL mode

Warning: a database on another host does not work without extra work.

The installer always writes `databaseHost` `127.0.0.1` and `databasePort`
`5432` (`installer.rs:1231-1232`). Every Host start probes that loopback port,
also in external mode (`service.rs:40-111`). A remote database therefore needs
a local listener on `127.0.0.1:5432`, or a code change.

Managed PostgreSQL is the default. External mode is an explicit option
(`installer.rs:1228-1230`; `athanor.iss:44-46`). It still uses the packaged
NATS, Host, substrate, and client. The service does no database work in
external mode. The Host, a user process, connects with the URL from the secret
file (`supervisor.rs:417-459`; `app.rs:30-58`).

**Not re-verified at a6ab453:** the external server needs PostgreSQL 18,
pgvector 0.8.6, and `pg_trgm`. The `akasha` and `substrate` migrations decide
it. Give The Athanor a private role and database.

Put the full connection URL in an ACL-restricted temporary file. Do not put a
password on the installer command line:

```powershell
$secret = "$env:TEMP\athanor-external-db.txt"
Set-Content -NoNewline -Encoding utf8 $secret `
  'postgresql://athanor:<password>@127.0.0.1:5432/athanor'
icacls $secret /inheritance:r /grant:r "${env:USERNAME}:(R)"
.\The-Athanor-<version>-windows-x64.exe /EXTERNALDATABASEFILE="$secret"
Remove-Item $secret -Force
```

The installer stores the URL in the restricted secret file as
`externalDatabaseUrl` (`installer.rs:1271,1277-1286`). External mode creates no
managed PostgreSQL data (`installer.rs:275-277`). Migrations always run
(`installer.rs:333`).

**Not re-verified at a6ab453:** the URL is never logged.
`crates/athanor-install/src/native_runtime.rs` decides it.

## Add a room

The installer has no room mode. It offers `install`, `update`,
`install-omp-adapter`, `rollback-omp-adapter`, `doctor`, `rollback`,
`uninstall`, and `purge` (`manage.rs:32-134`). It refuses an installed version
(`installer.rs:217-223`). It never writes `harnesses.json`.

Manual configuration does not require a new release.
The following inventory is not a verified installation procedure.
Keeper capability provisioning and the exact Host stop/start procedure remain undocumented here.
Do not treat this inventory as an end-to-end room setup guide.

| Surface | Required configuration |
|---|---|
| `<roomsRoot>\<room>\.omp\runtime\athanor-house-state.json` | `version`, `operator`, `agentName`, `embodiedSpirit`, `room`, and `recallPolicy` (`installer.rs:256-270`). |
| `<roomsRoot>\<room>\.omp\runtime\omp-keeper.json` | `ompLaunch`, `workspace`, `programRoot`, `stateRoot`, and exactly one of `capability` or `capabilityPath` (`crates/omp-keeper/src/config.rs:19-34,61-105`). |
| `D\config\runtime.json` | A `rooms` entry with a safe `room` key and `spirit` (`supervisor.rs:243-246,308`). |
| `D\config\harnesses.json` | The harness entry and its keeper configuration path (`harness/config.rs:25-59`). |
| The operator's `.omp\agent\athanor\client.json` | A `rooms` entry for `athanor chat`. The client refuses an absent room (`crates/athanor-install/src/omp.rs:71-75`). |

The Host reads its room and harness configuration at start.
It does not reload these files (`app.rs:87,91`; `harness/owner.rs:35-48`).
The loader checks health through `client.defaultRoom` (`adapters/omp/installed-loader.ts:641`).
The adapter derives its active room from workspace markers (`adapters/omp/house-proof/room.ts:86-127`).
An upgrade without `/HOUSECONFIGFILE` keeps the House from `runtime.json` (`installer.rs:1171-1172`).

Remaining proof: provision a new room, start its keeper, connect OMP and chat, then verify restart without disturbing existing rooms.

[Limitations § Rooms and sessions](./docs/LIMITATIONS.md#5-rooms-and-sessions)
lists the other room limits.

## Upgrade and rollback

A newer installer runs `install` again, and that upgrades
(`athanor.iss:84`; `installer.rs:217-223,279-281`):

1. verify the full new payload before any change (`installer.rs:1097-1105`);
2. back up PostgreSQL while the current service is healthy
   (`installer.rs:279-281`);
3. stop the service in dependency order (`installer.rs:286-287`;
   `supervisor.rs:384-414`);
4. stage the new immutable version (`installer.rs:289-305`);
5. replace the stable binaries and write the config and pointers
   (`installer.rs:307-332`);
6. migrate PostgreSQL (`installer.rs:333`);
7. start the service and require readiness (`installer.rs:334-340`).

If a step fails, the installer restores the previous activation and the
database backup (`installer.rs:351-381,942-983`).

Warning: rollback restores the database from the pre-upgrade backup.

`rollback` takes no argument (`manage.rs:120`). It returns only to
`previousVersion` in `current.json`. It refuses when that release or its
backup is missing (`installer.rs:390-400`).

```powershell
& "$env:ProgramFiles\Solarisael\Athanor\bin\athanor.exe" rollback
```

Rollback takes an undo backup first (`installer.rs:409`). Then it restores
`rollbackBackup` (`installer.rs:394-397,432`). If the older release does not
become ready, it restores the newer release and the undo backup
(`installer.rs:440-451`).

## Uninstall and purge

Windows uninstall stops and removes the service and the product binaries. It
also removes the loader from `config.yml` and deletes the client folder
(`installer.rs:1063-1095`; `athanor.iss:32-33`). It keeps every file under
`D` (`manage.rs:123`). A reinstall reuses the existing `runtime.json` and
secrets (`installer.rs:1171-1172,1263`).

Warning: purge deletes the product and all persistent data.

Purge is a separate command. Inno Setup never runs it (`athanor.iss:32-33`;
`manage.rs:125-131`). Export the rooms and backups that you need, then run:

```powershell
& "$env:ProgramFiles\Solarisael\Athanor\bin\athanor.exe" purge --confirm-data-loss
```

Purge refuses without `--confirm-data-loss` (`installer.rs:1088-1092`). It
removes product and data (`installer.rs:1093-1094`).

## Legacy pre-install door

The installer looks for the legacy names `solarisael-house`, `athanor-omp`, and
`athanor-substrate` next to `P` (`layout.rs:13`; `installer.rs:1358-1366`).
Before first activation it copies a backup to `D\backups\legacy-preinstall` and
writes a one-time marker (`installer.rs:282,1351-1377`).

**Not re-verified at a6ab453:** the copy is bounded and excludes caches. Legacy
code is never a runtime fallback. Migration needs a valid backup and schema
lineage. The door runs no legacy Python, WSL, Bun, or shell tooling.
`crates/athanor-install/src/native_runtime.rs` decides these.
