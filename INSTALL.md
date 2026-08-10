# Install The Athanor

This is the one installation door for The Athanor 0.11.0.

One repository and one release own the core, the substrate, the OMP adapter, the
installer, and the updater. You download one archive. You run one installer.

The installer stages the whole tree, verifies it, and only then activates it.
This is not a one-click installer.

Read [`docs/SECURITY.md`](./docs/SECURITY.md) before you handle credentials.
Read [`IDENTITY_GUIDE.md`](./IDENTITY_GUIDE.md) before you write a room identity.

## Before you start

The supported platform is Windows x64 with [OMP](https://github.com/can1357/oh-my-pi).

Install these first:

- Windows 10 or Windows 11 on x64;
- OMP, callable as `omp`;
- Bun, callable as `bun`.

The installer runs the verifier through Bun. An install fails without Bun.

**Warning.** The installer edits your OMP configuration file. It keeps every
unrelated setting. It replaces only Athanor extension paths.

**Warning.** The installer replaces an existing target directory. Read
[Choose the right flag](#choose-the-right-flag) before you pass `--force`.

## Choose a profile

There are two public profiles: `vault` and `akasha`.

Vault is file-attributed retrieval. Vault needs no substrate binary, no
PostgreSQL, no embeddings, no WSL, and no Rust runtime.

AKASHA adds substrate operations and the platform substrate binary. AKASHA
requires PostgreSQL with pgvector, a local embedding service, and WSL.

Read [the README profile comparison](./README.md#grow-into-akasha-when-the-work-needs-it)
to choose. Your profile decides which archive you download.

## Download one archive

Each release publishes exactly two archives for Windows x64:

```text
the-athanor-0.11.0-windows-x64-vault.zip
the-athanor-0.11.0-windows-x64-akasha.zip
```

Each release also publishes `release-manifest.json`. The updater reads that
manifest. Get all three from the [`solarisael/the-athanor`](https://github.com/solarisael/the-athanor/releases)
releases page.

Keep the original ZIP file. The installer reads the ZIP as its bundle.

## What an install puts on disk

An install writes three directories under your target directory:

```text
<target>\
  the-athanor\            product code; never write inside it
    adapters\omp\         the OMP adapter, installer, and updater
    substrate\            substrate operations; AKASHA only
  rooms\                  your rooms
  state\                  mutable state
    athanor.env           canonical topology for this install
    substrate\            substrate dotenv and PostgreSQL dumps; AKASHA only
```

It also copies the onboarding documents to the target root: `README.md`,
`INSTALL.md`, `USAGE.md`, `IDENTITY_GUIDE.md`, `LICENSE`, `NOTICE`, and
`SETUP.txt`. Nothing else outside `the-athanor\` is overwritten.

The installer writes `state/athanor.env` with the canonical topology variables:

| Variable | Profile | Value |
|---|---|---|
| `ATHANOR_STATE_DIR` | both | `<target>\state` |
| `ATHANOR_SUBSTRATE_ROOT` | AKASHA | `<target>\the-athanor\substrate` |
| `ATHANOR_SUBSTRATE_EXE` | AKASHA | `<target>\the-athanor\adapters\omp\bin\windows-x64\athanor-substrate.exe` |

The adapter reads `state/athanor.env` directly. Do not export these variables by
hand. A real environment variable overrides the file.

`ATHANOR_AUTO` is optional. Leave it unset unless a maintainer asks for it.

The installer also adds two absolute paths to your OMP `extensions:` list:

```yaml
extensions:
  - <target>/the-athanor/adapters/omp/index.ts
  - <target>/the-athanor/adapters/omp/hygiene.ts
```

## Install a fresh Vault

Follow these steps for a host with no earlier Athanor install.

1. Extract a working copy of the archive:

   ```powershell
   Expand-Archive .\the-athanor-0.11.0-windows-x64-vault.zip .\athanor-setup
   ```

2. List the supported harnesses:

   ```powershell
   .\athanor-setup\the-athanor\adapters\omp\install.exe --list-harnesses
   ```

   The catalog holds `omp` only.

3. Choose a room key. Use lowercase letters, digits, and single hyphens. The key
   `house` is reserved.

4. Run the installer:

   ```powershell
   .\athanor-setup\the-athanor\adapters\omp\install.exe `
     --bundle .\the-athanor-0.11.0-windows-x64-vault.zip `
     --target C:\Solarisael `
     --room my-room `
     --mode vault `
     --harness omp
   ```

5. Read the JSON result. A success prints `"ok": true` with the target, the
   profile, the room, and the environment.

`--target` must be an absolute path. `--config` defaults to
`%USERPROFILE%\.omp\agent\config.yml`. Pass `--config` with an absolute path for
a different OMP configuration.

Add `--dry-run` to stage and verify without changing anything.

Continue at [Prove the room](#prove-the-room).

## Install a fresh AKASHA

A fresh AKASHA install needs two proofs before it activates: substrate
credentials and a fresh database backup.

You supply the credentials with `--env-file`. The installer copies that file to
`<target>\state\substrate\.env` before it verifies the staged tree.

You supply the backup with `--backup`. The installer validates the dump, then
copies it to `<target>\state\substrate\backups\`. It renames the copy to
`solarisael_memory_<UTC>.dump`, using `YYYY-MM-DD_HHMMSS`. That name joins the
rotation family `backup.sh` prunes.

Both source files stay untouched.

Neither flag is accepted for Vault. Neither flag is accepted with
`--migrate-legacy`, which carries the detected 0.10.x dotenv and dumps forward.

A fresh install never searches for a dump. An absent backup is the absence of
safety, so you must name one.

**Warning.** Prepare the database first. The installer refuses to activate an
AKASHA tree that does not verify as exactly `AKASHA`.

1. Install WSL 2 and PostgreSQL 16 with the `pgvector` and `pg_trgm` packages.
2. Create the PostgreSQL role and database that your dotenv will name.
3. Start your local embedding service.
4. Extract a working copy of the archive:

   ```powershell
   Expand-Archive .\the-athanor-0.11.0-windows-x64-akasha.zip .\athanor-setup
   ```

5. Copy the example dotenv to an absolute path you own:

   ```powershell
   Copy-Item .\athanor-setup\the-athanor\substrate\.env.example `
     C:\athanor-setup\substrate.env
   ```

6. Edit `C:\athanor-setup\substrate.env`. Set `PGHOST`, `PGPORT`, `PGUSER`,
   `PGPASSWORD`, and `PGDATABASE`. Set the embedding URL, model, and dimension.

7. Open a WSL shell. Change to the extracted substrate directory:

   ```bash
   cd /mnt/c/Users/<you>/Downloads/athanor-setup/the-athanor/substrate
   ```

8. Apply the substrate migrations. They also create the two extensions:

   ```bash
   ATHANOR_STATE_DIR=/mnt/c/Solarisael/state \
     python3 run_migrations.py --env-file /mnt/c/athanor-setup/substrate.env
   ```

9. Prove the substrate:

   ```bash
   ATHANOR_STATE_DIR=/mnt/c/Solarisael/state \
     python3 health.py --env-file /mnt/c/athanor-setup/substrate.env
   ```

   Read `database` and `embedding`. Both must report `"ok": true`. The backup
   probe still fails here, because you take the dump next.

10. Take the custom-format dump of the migrated database:

    ```bash
    pg_dump -h 127.0.0.1 -p 5432 -U solarisael -d solarisael_memory \
      -Fc --no-owner --no-acl -f /mnt/c/athanor-setup/solarisael_memory.dump
    ```

    Match the host, port, user, and database to your dotenv. The dump must be
    newer than 24 hours when you install.

11. Run the installer from PowerShell:

    ```powershell
    .\athanor-setup\the-athanor\adapters\omp\install.exe `
      --bundle .\the-athanor-0.11.0-windows-x64-akasha.zip `
      --target C:\Solarisael `
      --room my-room `
      --mode akasha `
      --env-file C:\athanor-setup\substrate.env `
      --backup C:\athanor-setup\solarisael_memory.dump `
      --harness omp
    ```

12. Read the JSON result. The `environment` field must name all three topology
    variables.
13. Read the `backup` field. It records `source`, `destination`, `size`, and
    `modifiedAt`. `destination` is the seeded copy inside your install.

The installer checks the backup before the dotenv. A run missing both flags
reports the backup refusal first.

**Warning.** The staged verification runs against a temporary tree, not against
`<target>`. A refusal there activates nothing. Your rooms, your configuration,
and your database stay untouched.

Read the refusal text. Fix the named condition. Do not retry with `--force`.

Continue at [Prove the room](#prove-the-room).

## Migrate a 0.10.x Vault install

A 0.10.x install root holds sibling product directories such as
`solarisael-house`, `solarisael-house-omp`, and `solarisael-house-substrate`.

The installer detects that layout. It refuses to replace it silently. The
migration is explicit.

**Warning.** The old `update.exe` cannot migrate a 0.10.x tree. It stops and
tells you to use the installer. Use the new installer with `--migrate-legacy`.

1. Close every OMP session.
2. Extract a working copy of the Vault archive:

   ```powershell
   Expand-Archive .\the-athanor-0.11.0-windows-x64-vault.zip .\athanor-setup
   ```

3. Preview the migration:

   ```powershell
   .\athanor-setup\the-athanor\adapters\omp\install.exe `
     --bundle .\the-athanor-0.11.0-windows-x64-vault.zip `
     --target C:\Solarisael `
     --room my-room `
     --mode vault `
     --migrate-legacy `
     --dry-run
   ```

4. Read the `legacy.reasons` list. Confirm that it names your old directories.
5. Run the same command without `--dry-run`.
6. Read `legacy.preserved`. It names every carried item and its new location.
7. Read `legacy.rollback`. It names the retained 0.10.x tree.

Name a room you already own in `--room`. The migration copies your rooms. The
installer creates a new room only when that room key has no marker file.

The migration also carries the substrate dotenv, the PostgreSQL dumps, and the
old package manifests:

| Old material | New location |
|---|---|
| `rooms\` | `<target>\rooms\` |
| `.env` | `<target>\state\substrate\.env` |
| `backups\` | `<target>\state\substrate\backups\` |
| old package manifests | `<target>\state\legacy\<old-directory>\` |

The old `--mode base` and `--mode full` tokens are not public modes. The
installer stops on either token and names the replacement. `base` maps to
`vault`. `full` maps to `akasha`. Both are accepted only together with
`--migrate-legacy`.

The installer also strips 0.10.x extension paths from your OMP configuration. It
removes the stale `SOLARISAEL_*` topology variables from the processes it starts.

Continue at [Prove the room](#prove-the-room).

## Migrate a 0.10.x AKASHA install

An AKASHA migration is gated on a real database backup.

**Warning.** Take a fresh backup first. The installer refuses the migration
without one. It does not take the backup for you.

The backup must be a custom-format `pg_dump` archive. The installer checks four
things: the file exists, the file is not empty, the file starts with the `PGDMP`
header, and the file is newer than the freshness window. The default window is
24 hours.

1. Close every OMP session.
2. Run the migration once with `--dry-run` and no `--backup`. Read the refusal.

   The refusal names a backup command that exists on your host right now. It
   names your detected 0.10.x `backup.sh` when that script is really on disk:

   ```bash
   bash <legacy-substrate>/backup.sh
   ```

   Otherwise it gives a direct `pg_dump` command. That command depends on
   nothing the installer has yet created:

   ```bash
   pg_dump -h 127.0.0.1 -p 5432 -U solarisael -d solarisael_memory \
     -Fc --no-owner --no-acl -f <target>\state\substrate\backups\solarisael_memory.dump
   ```

3. Run the command the refusal printed. Use its exact text, not this example.
4. Note the absolute path of the new `.dump` file.
5. Extract a working copy of the AKASHA archive:

   ```powershell
   Expand-Archive .\the-athanor-0.11.0-windows-x64-akasha.zip .\athanor-setup
   ```

6. Preview the migration:

   ```powershell
   .\athanor-setup\the-athanor\adapters\omp\install.exe `
     --bundle .\the-athanor-0.11.0-windows-x64-akasha.zip `
     --target C:\Solarisael `
     --room my-room `
     --mode akasha `
     --migrate-legacy `
     --backup C:\path\to\solarisael_memory_2026-08-09_101500.dump `
     --dry-run
   ```

7. Run the same command without `--dry-run`.
8. Read the `backup` field. It records `source`, `size`, and `modifiedAt`. Its
   `destination` is `null`, because a migration preserves the dump in place.

Omit `--backup` to let the installer search for the newest `.dump` file. It
searches `<target>\state\substrate\backups`, any preserved legacy `backups`
directory, and the old substrate `backups` directory.

Pass `--backup-max-age-hours N` to widen the freshness window deliberately. `N`
must be a positive number.

Continue at [Prove the room](#prove-the-room).

## Rollback and receipts

A migration never deletes the 0.10.x tree. It retires the tree inside the new
install root:

```text
<target>\.athanor-rollback-0.10.x-<timestamp>
```

The install result names that path in `legacy.rollback`. Keep the directory
until you trust the new install.

Verification happens before activation. The installer verifies the staged tree
first. It activates the tree second. It verifies the installed tree third. Any
failed verification restores the previous target and the previous OMP
configuration.

A plain update deletes the previous tree after success. A migration does not.

If the installer cannot retire the old tree, it returns a `warning` field. That
field names where the old tree was left.

## Update from 0.11.0 onward

The updater keeps your profile. It refuses a profile change.

Check for an update without changing files:

```powershell
C:\Solarisael\the-athanor\adapters\omp\update.exe `
  --target C:\Solarisael `
  --room my-room `
  --mode vault `
  --channel stable `
  --check
```

Remove `--check` to apply the update. Use `--mode akasha` for an AKASHA install.

The updater reports one state:

| State | Meaning |
|---|---|
| `current` | The installed version is the newest on this channel. |
| `available` | A newer version exists. `--check` stopped before download. |
| `verified` | `--dry-run` staged and verified the release. Nothing changed. |
| `updated` | The release is installed. |
| `started` | The updater relaunched itself outside the install. Read the receipt for the outcome. |
| `failed` | The update stopped. The `error` field names the reason. |

The updater writes that result to a receipt file. The default receipt path is
`<target>.update-receipt.json`. Pass `--receipt` with an absolute path to choose
another location.

Channels are `stable`, `beta`, and `experimental`. The default is `stable`. The
default repository is `solarisael/the-athanor`. Pass `--repository OWNER/REPO`
for a fork.

Before it downloads, the updater validates the release manifest, the repository,
and the channel. After it downloads, it checks the SHA-256 hash and the byte
size. It then checks the bundle version, profile, and platform. An AKASHA update
also checks the installed substrate schema version.

The updater hands the verified bundle to the release installer with `--update`.
That path preserves your rooms, your state, and your configuration.

On Windows the updater copies itself outside the install before it replaces
files. Pass `--force` only to reinstall the same version deliberately.

## Prove the room

Run these checks after any install, migration, or update.

### Run the verifier

```powershell
bun run C:\Solarisael\the-athanor\adapters\omp\verify-install.ts `
  --room C:\Solarisael\rooms\my-room `
  --config $env:USERPROFILE\.omp\agent\config.yml `
  --profile vault `
  --require-manifest
```

Use `--profile akasha` for an AKASHA install.

Read the `mode` field. It must be exactly `Vault` or exactly `AKASHA`. The value
`degraded` means a check failed. Read `diagnostics` and fix the named condition.

### Prove the first session

1. Start a fresh OMP session from the room directory.
2. Call `room_state`.
3. Confirm the room key, the spirit name, and the operator name.
4. Confirm the state path sits inside `<target>\state`.

A fallback room means room discovery failed. Correct the room marker or the
working directory. Then repeat the fresh session.

### Prove continuity

1. Write one short distinctive sentence under `## First continuity test` in
   `room_summary.md`.
2. Close the session.
3. Start another fresh session from the same room.
4. Ask for that sentence and its source.

Success means the agent recovers the sentence and names `room_summary.md`.

### Prove AKASHA memory

Run these steps only for an AKASHA install.

1. Call `remember` with one disposable test memory.
2. Call `recall` with the same phrase.
3. Confirm the result comes from the new room.
4. Call `sleep` with a disposable paper boat.
5. Start a fresh session.
6. Call `wake` and recover the boat.
7. Delete the disposable records.

## When the installer refuses

The installer prints one JSON object with an `error` field. Read it before you
retry.

| Refusal | Do this |
|---|---|
| `--mode <token> is a 0.10.x token` | Use `--mode vault` or `--mode akasha`. |
| `A 0.10.x installation was detected` | Add `--migrate-legacy`. |
| `target already exists` | Choose `--update`, `--force`, or `--migrate-legacy`. |
| `Vault bundle must not contain` | You passed an AKASHA archive with `--mode vault`. |
| `AKASHA bundle missing required file` | You passed a Vault archive with `--mode akasha`. |
| `AKASHA migration refused: no PostgreSQL backup was found` | Take the dump the refusal names. Pass `--backup PATH`. |
| `--backup is only accepted for --mode akasha` | Drop the flag. Vault has no database. |
| `--backup must be an absolute path` | Give the full drive path, not a relative one. |
| `--backup-max-age-hours is only accepted for --mode akasha` | Drop the flag. |
| `fresh AKASHA needs substrate credentials` | Pass `--env-file` with an absolute dotenv path. |
| `--env-file is only accepted for --mode akasha` | Drop the flag. Vault has no substrate dotenv. |
| `--env-file is not accepted with --migrate-legacy` | Drop the flag. The old dotenv is carried forward. |
| `--env-file must be a readable regular file` | Point the flag at a real file you can read. |
| `--env-file must be an absolute path` | Give the full drive path, not a relative one. |
| `is not a custom-format pg_dump archive` | Take the dump with `pg_dump -Fc`. |
| `older than the ... freshness window` | Take a fresh dump. |
| `bundle carries a development-checkout marker` | The archive is not a release build. Download it again. |
| `installed profile is ...` | Reinstall deliberately. The updater cannot change profiles. |

### Choose the right flag

| Flag | Effect on an existing target |
|---|---|
| none | The installer stops. |
| `--update` | Keeps rooms, state, and configuration. Replaces product code. |
| `--force` | Replaces the target. Carries forward operator-owned files. |
| `--migrate-legacy` | Migrates a 0.10.x tree and keeps a rollback copy. |
| `--dry-run` | Stages and verifies. Changes nothing. |
| `--env-file` | Seeds `state\substrate\.env` for a fresh AKASHA install. |
| `--backup` | Seeds the validated dump into `state\substrate\backups\`. |

## Remove The Athanor

1. Remove the two Athanor entries from the OMP `extensions:` list.
2. Delete `<target>\the-athanor`.
3. Keep `<target>\rooms` and `<target>\state`.

**Warning.** Deleting `<target>\state` destroys the substrate dotenv and every
PostgreSQL dump. Delete a room or a memory store only after you name the exact
scope and accept the loss.

## Limits of this document

The tested support matrix lives in [`docs/LIMITATIONS.md`](./docs/LIMITATIONS.md).

One point stays unproven here, and this guide does not guess it:

- A fresh AKASHA install was exercised only up to the substrate health probe on
  a host without a live database. Both gates passed and the staged verification
  then reported `degraded`, as it must without PostgreSQL. A green fresh AKASHA
  activation is therefore described from source, not observed.

Three earlier gaps are now closed and documented from observed behavior. The
dotenv travels through `--env-file`. The dump travels through `--backup`. Each
refusal prints a command that exists before the install.

Daily use continues in [`USAGE.md`](./USAGE.md).
