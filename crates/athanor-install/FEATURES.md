# athanor-install

The native installer and the managed runtime for Windows. It builds the `athanor-manage` binary.

### installer

`src/installer.rs`. Install, upgrade, rollback, and removal. Every public entry takes the operation lock first.

- **Install and upgrade.** `install` validates the manifest, resolves and validates the House configuration, runs the preflight, then refuses a version that is already active.
- **Install order.** It creates the directories, restricts the access list, writes a missing room state file under its own rooms root, backs up the database, imports a legacy tree once, stages the release, verifies it, renames it into place, verifies it again, copies the manager binary, writes the configuration and the secrets, writes the pointers, migrates the database, installs and starts the service, waits for readiness, then writes the operator integration.
- **Staged activation.** Artifacts land in a hidden staging directory, then move with one rename. Verification runs before the rename and again after it.
- **Failed install.** A failure restores the prior installation and reports both the original error and every restoration failure. Nothing is silently half applied.
- **Restore order.** The restore replaces the pointers, then the database, then the configuration and the secrets, then the remaining files. It removes the failed release, then restores or removes the service.
- **Rollback.** `rollback` needs a retained previous version and its pre-upgrade backup. It takes an undo backup, stops the service, writes both pointers, restores the database, then starts and waits. Every step restores the newer release when it fails.
- **Install the OMP adapter.** `install_omp_adapter` verifies the component, checks compatibility against the active native release, retains the release, then moves the pointer.
- **Roll back the OMP adapter.** `rollback_omp_adapter` takes a named release or the retained previous one. It refuses a release that is already active, and it refuses an identity that does not match its manifest.
- **Component pointers.** The pointer holds the format, the active release identifier, and the previous one. A failed write restores the prior pointer.
- **Component transition.** A compatible active component stays. Otherwise the fallback inside the native release is verified, retained, and made active.
- **Retention.** `retain_component_release` copies into a random pending directory, verifies the copy against the source manifest, then renames it. A failed staging directory is removed.
- **Pruning.** Pruning keeps the active release and the previous one, and deletes every other directory. It verifies both retained releases before it deletes anything, and it collects every failure.
- **Snapshot machinery.** `capture_install_rollback` snapshots the two pointers, the configuration, the secrets, the manager binary, the loader, the legacy marker, every room state file, and both operator files. An absent file is snapshotted as absent, so a restore removes it.
- **Native verification.** `verify_native_release` compares the retained manifest with the declared one, then checks the size and the SHA-256 digest of every artifact.
- **Preflight.** It verifies every staged artifact, requires a valid adapter fallback, requires a room state file outside its own rooms root, and requires absolute operator paths with a present loader.
- **Configuration.** `write_configuration` writes the runtime file and, on first install, generates a 32-byte host token and a 32-byte database password. It restricts the access list on the secret file and its directory.
- **Operator integration.** `write_operator_integration` writes the client projection with one endpoint for each room, restricts it to the operator, then registers the loader.
- **Legacy import.** `import_legacy_once` runs one time. A marker file stops every later attempt.
- **Uninstall and purge.** `uninstall` stops and removes the service, unregisters the loader, removes the client directory, then removes the program tree. `purge` adds the data tree and demands explicit confirmation.
- **Pointer lineage.** Reads and writes refuse an unsafe version and refuse a previous version equal to the active one.

### boundaries

`src/boundaries.rs`. The seam traits and their native implementations.

- **`FileSystem`.** Twelve operations: exists, read, list directories, validate a regular file, create directories, copy, write atomically, rename, remove a file, remove a tree, restrict the access list, and restrict it for one user.
- **`OperationLock`.** One process mutex plus one named Windows mutex. It waits up to 30 seconds, then fails. The guard releases and closes the handle on drop.
- **`ServiceManager`.** Five operations: install or update, start, stop, remove, and test for installation.
- **`RuntimeControl`.** Five operations: back up the database, import a legacy tree, migrate, restore, and wait for readiness.
- **`SecretSource`.** One operation that fills a buffer with random bytes.
- **`NativeFileSystem`.** It walks every path part and refuses a symbolic link or a reparse point. An atomic write goes through a `.new` file and one rename.
- **Access lists.** `icacls.exe` breaks inheritance and grants the system account and the administrators group. The user variant resolves the caller when the name carries no domain.
- **`OsSecrets`.** It reads from the random source of the operating system.
- **`ScServiceManager`.** It drives `sc.exe`. A start waits up to 120 seconds for the running state and fails early on a stop. A stop and a delete tolerate the missing-service codes.
- Every trait keeps the installer testable. The tests supply their own file system, services, runtime, and secrets.

### supervisor

`src/supervisor.rs`. The managed child processes of the House.

- **`Processes` trait.** Five operations: spawn, test readiness, request a stop, wait for exit, and kill a verified child.
- **Start order.** `run` starts each child in order, reports a checkpoint, then waits up to 90 seconds for readiness. A failure stops every child it already started.
- **Stop order.** `stop` reverses the start order. It requests a graceful stop, waits up to 30 seconds, then kills the child.
- **Verified kill.** The supervisor refuses to kill a name it does not own. It never touches an unverified process identifier.
- **Graceful stop.** On Windows it sends a break event to the process group of the child.
- **Readiness.** A child is ready when a loopback port accepts a connection, or when a readiness file exists. A stale readiness file is removed before the spawn.
- **`runtime_plan`.** It builds the child list: PostgreSQL in managed mode only, NATS with JetStream, the delivery worker with a readiness file, then one Host for each room.
- **Plan validation.** It requires loopback addresses, an absolute rooms root, at least one room, safe and unique room keys, and unique ports that avoid the database and broker ports.
- **Host environment.** Each Host child receives the token, the room directory, the state directory, the house identifier, the room, the spirit, the session, and the bind address.
- **`prepare_service_console`.** It ignores console control events, so a break sent to a child never stops the service.

### native_runtime

`src/native_runtime.rs`. The runtime seam over the installed substrate binary.

- **`backup_database`.** It runs the substrate backup, keeps 3 backups, then returns the newest manifest.
- **`migrate_database`.** It runs the substrate migrations inside a temporary database session.
- **`restore_database`.** It reads the database name out of the address and passes it as an explicit confirmation.
- **`wait_ready`.** It runs the health command and skips the embedding check.
- **`import_legacy`.** It copies a legacy tree with a bounded depth of 8 and a bounded entry count. It skips `node_modules`, `target`, `.venv`, and `__pycache__`.
- **`with_database`.** In managed mode it initializes the data directory when needed, starts PostgreSQL, creates the database on first use, runs the action, then always stops PostgreSQL. External mode runs the action directly.
- **`initialize_postgres`.** It runs `initdb` with the `scram-sha-256` method and UTF-8 encoding. The password file is removed straight after use.
- **Address.** An external address wins. Otherwise the managed loopback address is built from the stored password.
- **Staging override.** `ATHANOR_INSTALL_STAGING_BIN` points the pre-upgrade backup at the staged substrate, because the installed one may not know the current lineage.

### component

`src/component.rs`. The manifest and the pointer for the OMP adapter.

- **`ComponentManifest`.** It holds the format, the component name, the version, the release identifier, the compatibility, and the artifacts.
- **Validation.** It requires format 1, the `omp-adapter` name, a safe version, at least one artifact, safe relative paths, lowercase digests, sorted paths, and no duplicate path.
- **Release identity.** The release identifier is the version plus a SHA-256 digest over a canonical identity block. A manifest that names a different identifier is refused.
- **Compatibility.** A component matches a native release when the host, substrate, and delivery interface numbers agree and the schema version agrees.
- **`ComponentPointer`.** It validates the format and both release identifiers, and it refuses a pointer that names its active release as the previous one.
- **`read_verified_component`.** It reads the manifest, validates it, then checks the size and the digest of every artifact on disk.
- Typed errors name each failure exactly. Nothing degrades into a generic message.

### manifest

`src/manifest.rs`. The release manifest for the native product.

- **Pinned versions.** The module pins the schema, the platform, PostgreSQL, pgvector, NATS, and Godot.
- **Validation.** It requires format 1, the `the-athanor` product, a safe version, the `windows-x64` platform, and the exact required schema.
- **Compatibility.** All eight compatibility fields must match the pinned values exactly. A mismatch reports every observed value.
- **Rollback contract.** The manifest must require a database restore and must retain at least 2 versions.
- **Artifacts.** Paths must be relative, must hold only normal parts, and must not use a backslash. Digests must be 64 hexadecimal characters. Duplicate paths are refused.
- **`verify_bytes`.** It compares the size first, then the SHA-256 digest without regard to case.

### service

`src/service.rs`. The Windows service host, held in the inline `windows` module.

- **`dispatch`.** It hands the service name and the entry function to the service control dispatcher.
- **`run`.** It registers the control handler, reports a pending start, prepares the console, reads the pointer, the configuration, and the secrets, builds the runtime plan, then starts the supervisor.
- **Checkpoints.** Each started child advances the start checkpoint, so a slow start never looks hung.
- **Control handler.** Stop and shutdown signal the channel. Interrogate succeeds. Anything else reports that it is not implemented.
- **Shutdown.** The service waits on the channel, reports a pending stop, stops every child, then reports the stopped state.
- **Status.** Every status carries the type, the state, the accepted controls, the checkpoint, and a 30-second wait hint while pending.
- Outside Windows `dispatch` fails with a clear message.

### omp

`src/omp.rs`. The projection and the loader registration for the operator client.

- **`ClientProjection`.** It holds the format, the house identifier, the host token, the state root, the default room, and one endpoint for each room.
- **Validation.** It requires a complete identity, an absolute state root, an endpoint for the default room, and a loopback `ws://` address for every endpoint.
- **`register_extension`.** It finds the `extensions:` block, removes every entry owned by the Athanor, appends the stable loader, then keeps the original line ending. A missing block is appended.
- **`unregister_extension`.** It removes only the exact loader line and leaves every other entry untouched.
- **Ownership test.** An entry counts as owned when it names an adapter entry file under a known Athanor path, or when it names the stable loader.

### layout

`src/layout.rs`. Every install path in one place.

- **Roots.** The program root and the data root both sit under `Solarisael/Athanor`.
- **Program paths.** Versions, one version, the manager binary, the loader, the current pointer, the adapter root, the adapter versions, one adapter version, and the adapter pointer.
- **Data paths.** The runtime configuration, the secrets, the backups, the PostgreSQL data, the NATS data, the logs, the rooms, the Host state, and the legacy backup.
- **Names.** The service name, the display name, the pointer file name, and the three legacy directory names.
- **`safe_version`.** A version holds at most 128 characters, starts with a letter or a digit, uses only letters, digits, dots, hyphens, and plus signs, and never holds two dots together.

### lib

`src/lib.rs`. The module list and the `doctor` report.

- **`doctor`.** It takes the operation lock, then runs seven checks and returns one report with an overall verdict.
- **Checks.** The current pointer, the release manifest, the artifact digests, the adapter pointer, the adapter integrity, the adapter compatibility, the previous adapter release, the Windows service, and the persistent data.
- **Honesty.** A missing or unparsable file becomes a failed check with the reason. `doctor` never guesses a version.
- The report carries the installed version, the service state, the data state, and every check with its own detail.

### main

`src/main.rs`. The `athanor-manage` binary.

- **Commands.** `install`, `update`, `install-omp-adapter`, `rollback-omp-adapter`, `gui`, `doctor`, `rollback`, `uninstall`, `purge`, and `service`.
- **Wiring.** It builds the layout from `ProgramFiles` and `ProgramData`, then hands the four native seams to the installer.
- **Install flags.** `--staging` and `--manifest` are required. `--external-database-file`, `--house-config-file`, and the three operator flags are optional.
- **Operator flags.** `--omp-config`, `--client-config`, and `--operator-principal` must arrive together.
- **`gui`.** It reads and validates the client projection, picks the room or the default one, then starts the installed Godot client with the token, the house, the address, the room, and the spirit.
- **Output.** Every command prints one JSON result. `doctor` fails the process when a check fails.
- **Help.** `help` prints the command list with every flag.
