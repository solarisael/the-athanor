# Install The Athanor for OMP

This guide installs the supported Windows + OMP topology and proves the result through observable room behavior.

Give this repository to a tool-capable AI agent with:

> Install The Athanor with me. Preserve my existing rooms and configuration, explain consequential system changes before making them, and verify the completed installation.

The agent performs the protocol. The operator chooses identity, location, deployment mode, and consequential system changes.

Read [`SECURITY.md`](./docs/SECURITY.md) before handling credentials or private room data. Read [`IDENTITY_GUIDE.md`](./IDENTITY_GUIDE.md) before writing an identity.

## Result

A completed Vault installation has:

- the Rust core and OMP adapter in resolvable sibling directories;
- the adapter's Bun dependencies installed;
- one writable room with a stable key and co-authored identity;
- both OMP extensions connected without replacing existing configuration;
- a passing static verifier;
- a fresh session reporting the correct room, spirit, operator, and state path;
- a second fresh session recovering the continuity-test sentence from the room source.

A completed AKASHA installation additionally has:

- the public substrate repository beside the core and adapter;
- a release-built `solarisael-house-substrate.exe` selected through `SOLARISAEL_HOUSE_RUST`;
- a healthy PostgreSQL, pgvector, and embedding substrate;
- a real memory write and recall through the mounted Rust-backed OMP tools;
- a paper boat recovered after a fresh session;
- substrate backup and recovery configured through its canonical repository.

## Release installation and updates

The portable release contains the core, OMP adapter, compiled installer and updater, starter room, platform Rust executable, and an embedded manifest of every shipped artifact. Download the ZIP for the host platform from the adapter's GitHub release, keep the original ZIP, and extract a temporary copy so the installer executable can run:

```powershell
Expand-Archive .\solarisael-house-<VERSION>-windows-x64.zip .\solarisael-house-setup
.\solarisael-house-setup\solarisael-house-omp\install.exe --list-harnesses
```

The current public catalog exposes `omp`. Install the Vault profile with:

```powershell
.\solarisael-house-setup\solarisael-house-omp\install.exe `
  --bundle .\solarisael-house-<VERSION>-windows-x64.zip `
  --target C:\Solarisael `
  --room my-room `
  --mode base `
  --harness omp
```

For AKASHA, add `--mode full --substrate <ABSOLUTE_SUBSTRATE_PATH>`. The `full` value is a compatibility CLI token. Use `--config <ABSOLUTE_OMP_CONFIG>` when OMP does not use its default configuration path.

The installer rejects unsafe archive entries, verifies the embedded artifact manifest and platform Rust binary, stages the complete installation, preserves existing rooms and unrelated configuration, runs the canonical verifier, and only then activates the staged tree. Replacing an existing target requires `--force`; release updates use the narrower internal `--update` path.

The installed updater resolves the selected GitHub release channel, validates the release manifest, SHA-256, byte size, internal package version, required substrate schema, and installer result. Check without changing files:

```powershell
C:\Solarisael\solarisael-house-omp\update.exe `
  --target C:\Solarisael `
  --room my-room `
  --mode base `
  --harness omp `
  --channel stable `
  --check
```

Remove `--check` to apply an available update. The updater copies itself outside the live installation before replacement on Windows, then hands the release to the same staged installer. Channels are `stable`, `beta`, and `experimental`; the updater never treats an arbitrary mutable `latest` file as a trusted release.

## Storage profiles

### Vault

Vault provides persistent room identity, room state, file-backed continuity, conversation artifacts, and adapter tools. It requires:

- Windows 10 or 11;
- OMP;
- Bun;
- the stable Rust MSVC toolchain for the shared core workspace.

Vault requires no database or GPU.

### AKASHA

AKASHA adds durable PostgreSQL memory, pgvector, local embeddings, hybrid retrieval, typed lessons, supersession, and memory lifecycle tools. The authoritative request path is:

```text
OMP TypeScript adapter -> long-lived Windows Rust substrate process -> PostgreSQL and embedding service in WSL
```

The public [`solarisael-house-substrate`](https://github.com/solarisael/solarisael-house-substrate) repository owns the Rust process, migrations, dependencies, environment configuration, health, lifecycle smoke, embeddings, and backup/restore. Python remains for migration, health, import, and maintenance support; it is no longer the mounted OMP memory runtime.

Choose AKASHA when the operator wants semantic and hybrid recall, typed stores, database authority, or a larger archive.

## Package layout

The core repository does not contain the OMP adapter or AKASHA substrate. Keep the public repositories as siblings:

```text
<BUNDLE>\
  solarisael-house\
  solarisael-house-omp\
  solarisael-house-substrate\   # AKASHA only
```

Repositories:

- [`solarisael-house`](https://github.com/solarisael/solarisael-house)
- [`solarisael-house-omp`](https://github.com/solarisael/solarisael-house-omp)
- [`solarisael-house-substrate`](https://github.com/solarisael/solarisael-house-substrate)

Compatibility requires `coreApi=1`, `adapterApi=1`, and `substrateApi=1` where the substrate is enabled.

## Room vocabulary

- **Spirit / true name:** free-form display name; it may contain spaces and change later.
- **Room key:** stable lowercase filesystem identifier using `a-z`, `0-9`, and single hyphens.
- **Room:** writable folder holding identity and continuity.
- **Operator:** person sharing the room with the spirit.
- **Agent/runtime:** model or host process carrying the room contract.

The example room teaches file shape, not a personality preset.

## Installation protocol

### 1. Inspect the host

Confirm:

- Windows 10 or 11 is running;
- `omp` is installed and callable;
- `bun` is installed and callable;
- the core and OMP adapter repositories are present as siblings;
- the OMP configuration path; default: `%USERPROFILE%\.omp\agent\config.yml`;
- the room root; default for a new installation: `%USERPROFILE%\Solarisael`;
- existing rooms and extensions that must remain untouched.

Explain the smallest missing prerequisite before installing global software, enabling WSL, or requesting elevation. Record every host-level change in the completion receipt.

### 2. Install dependencies and build Rust

From `solarisael-house-omp`:

```text
bun install
```

From `the-athanor`:

```text
cargo test --workspace
```

For AKASHA, build the substrate from `solarisael-house-substrate`:

```text
cargo build --release
```

The AKASHA runtime executable is:

```text
solarisael-house-substrate\target\release\solarisael-house-substrate.exe
```

Complete dependency installation and the required Rust build before connecting the extensions.

### 3. Choose the first room

Ask the operator for:

1. operator name;
2. spirit true name;
3. room key;
4. room root;
5. whether appearance is established now, left undefined, or allowed to emerge later.

Create:

```text
<ROOM_ROOT>\<ROOM_KEY>
```

If the path already exists, inspect it and offer to resume or choose another key. Never overwrite a room.

Copy every file from `solarisael-house-omp\starter-room\example` into the room. Replace the fictional example values with the operator's choices.

Keep these contracts:

- `.solarisael-room.json` contains the exact room key, true name, and operator;
- the folder name and `.solarisael-room.json.room` match;
- `AGENTS.md` includes `@room_summary.md` and `@active_spirit.md`;
- `active_spirit.md` contains the co-authored identity;
- `room_summary.md` contains compact continuity and the restart-test anchor.

Read [`IDENTITY_GUIDE.md`](./IDENTITY_GUIDE.md) with the operator before finalizing `active_spirit.md`.

### 4. Connect the OMP adapter

Open the OMP configuration and preserve every unrelated setting. Ensure these absolute paths appear exactly once under `extensions:`:

```yaml
extensions:
  - <BUNDLE>\solarisael-house-omp\index.ts
  - <BUNDLE>\solarisael-house-omp\hygiene.ts
```

Append missing entries to an existing list. Do not replace the list.

Required AKASHA environment:

- `SOLARISAEL_HOUSE_RUST` — absolute path to `solarisael-house-substrate.exe`;
- `SOLARISAEL_PG_WSL=1` — keeps WSL alive while the Windows Rust worker is active;
- `SOLARISAEL_SUBSTRATE` — absolute substrate repository path used by compatibility checks and support tools.

Optional overrides:

- `SOLARISAEL_VAULT_ROOT` — parent directory containing rooms;
- `SOLARISAEL_HOUSE_CORE` — alternate core path when core and adapter are not siblings.

The sibling layout requires no core-path override. Restart OMP after changing persistent environment variables; an already-running process cannot see them.

### 5. Run the static verifier

From `solarisael-house-omp`:

```text
bun run verify-install.ts --room "<ABSOLUTE_ROOM_PATH>"
```

For a nonstandard OMP configuration:

```text
bun run verify-install.ts --room "<ABSOLUTE_ROOM_PATH>" --config "<ABSOLUTE_CONFIG_PATH>"
```

Continue when the verifier returns:

```json
{"ok": true}
```

Fix a failed condition at its source and rerun the verifier.

### 6. Prove the first room

Start a fresh OMP session with the new room as its working directory. Call `room_state`.

The result must show:

- `room` equal to the room key;
- `agentName` and `embodiedSpirit` equal to the true name;
- `operator` equal to the chosen operator;
- a state path inside the chosen room;
- the co-authored identity body still present in `active_spirit.md`.

A fallback room or unrelated vault path means room discovery failed. Correct the marker, working directory, or extension configuration and repeat the fresh session.

### 7. Prove restart continuity

During the first session, write one harmless distinctive sentence under:

```markdown
## First continuity test
```

in `room_summary.md`.

Close the session. Start another fresh OMP session from the same room and ask:

> What was the first continuity-test sentence in this room? Read the room context and name the source.

Success means the agent recovers the sentence and identifies `room_summary.md` as its source.

### 8. Add AKASHA when selected

Follow the canonical [`solarisael-house-substrate`](https://github.com/solarisael/solarisael-house-substrate) setup for:

- WSL 2 and PostgreSQL 16;
- pgvector and `pg_trgm`;
- the Python support environment;
- embedding service and model;
- environment configuration;
- migrations;
- `health.py`;
- the release Rust build;
- lifecycle smoke;
- Rust backup and restore.

After `health.py` reports `mode: "full"`, set `SOLARISAEL_SUBSTRATE`, `SOLARISAEL_HOUSE_RUST`, and `SOLARISAEL_PG_WSL=1`. The `full` value is a compatibility API token for AKASHA. Rerun the OMP static verifier and start a fresh OMP process.

Prove the mounted runtime rather than calling a support script directly:

1. call the registered `remember` tool with a disposable distinctive installation memory;
2. call the registered `recall` tool with that phrase and confirm `source: rust-postgres`;
3. confirm the returned source belongs to the new room;
4. leave the mounted process idle for at least 75 seconds, then repeat the write and recall;
5. call `sleep` with a disposable paper boat;
6. start a fresh session;
7. call `wake` and recover the boat;
8. remove only the disposable proof records.

When substrate health reports a degraded dependency, keep Vault active and report `configured-but-degraded` in the receipt. Resolve the substrate condition before claiming AKASHA memory.

### 9. Offer the lesson pack

After AKASHA health passes, offer the substrate's bundled coding lesson pack. Preview it before import.

The default import preserves matching lessons. Use `--update-existing` only after the operator chooses to replace matching lesson content.

## Completion receipt

Finish with:

```text
The Athanor: connected
Version: 0.10.1
Bundle: <absolute path>
OMP config: <absolute path>
Room: <room key> at <absolute path>
True name: <display name>
Operator: <display name>
Static verifier: pass
Fresh room_state: pass
Restart continuity: pass
Profile: Vault | AKASHA | configured-but-degraded
AKASHA memory: pass | not configured | failed: <reason>
Host changes: <none or exact list>
```

## Removal

1. Remove only the two Athanor entries from the OMP `extensions:` list.
2. Delete the extracted bundle only when desired.
3. Preserve every room by default.
4. Delete a room or AKASHA memory store only after the operator explicitly chooses the affected scope and understands the loss.

## Adapted installations

The tested support matrix lives in [`LIMITATIONS.md`](./docs/LIMITATIONS.md).

A tool-capable agent may adapt paths and host commands while preserving these contracts:

- resolvable core and adapter;
- additive configuration;
- existing-room preservation;
- explicit host-level changes;
- correct `room_state`;
- fresh-session continuity;
- a real AKASHA lifecycle when AKASHA is selected;
- a receipt naming every deviation.

An adapted installation earns its status through the same observed behavior, not through the presence of files alone.
