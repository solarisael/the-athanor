# Install `current.json`

Status: `census` (2026-11-06).

## What it is

Two pointer files select the installed release.
The native pointer `<program>/current.json` names the active native version, the previous version, and the database backup for a rollback.
The component pointer `<program>/components/omp-adapter/current.json` names the active OMP adapter release and the previous release.
The installer in `crates/athanor-install` is the only writer of both files.
The service, the native runtime, the OMP keeper, the OMP loader, and `athanor doctor` read them.
One install or one rollback changes the native pointer, the component pointer, and the database schema in sequence.
No journal on disk records this sequence; the undo plan, `InstallRollback` (`installer.rs:287`), lives only in memory.

## States

### Native pointer (`crates/athanor-install/src/installer.rs`)

`CurrentRelease` (`installer.rs:43-48`) holds the state in fields:

- `version`: the active native release.
- `previous_version`: the release that a rollback activates. It is optional.
- `rollback_backup`: the database backup that a rollback restores. It is optional, with `serde(default)` at `installer.rs:46`.

The path is `layout.rs:65-67`, with the file name `CURRENT_POINTER` at `layout.rs:7`.
The effective states are `Absent`, `Fresh` (no previous version), and `Paired` (a previous version and a backup).
The type also permits `previous_version` without `rollback_backup`, and `rollback_backup` without `previous_version`.

### Component pointer (`crates/athanor-install/src/component.rs`)

`ComponentPointer` (`component.rs:40-46`) holds the state in fields:

- `format`: always `COMPONENT_FORMAT = 1` (`component.rs:10`).
- `release_id`: the active adapter release, `<version>-<sha256>`.
- `previous_release_id`: the release that a component rollback activates. It is optional.

The path is `layout.rs:77-79`.
The effective states are `Absent`, `Fresh`, and `Paired`.

### Readers

| Reader | File | Lineage check |
|---|---|---|
| Installer, native | `read_current`, `installer.rs:1308-1325` | yes, `installer.rs:1316-1323` |
| Installer, component | `read_component_pointer`, `installer.rs:653-695` | yes, `ComponentPointer::validate` at `installer.rs:662`, and both releases verified at `installer.rs:663-693` |
| Doctor | `lib.rs:48-88`, `lib.rs:138-227` | yes, `lib.rs:52-55` and `lib.rs:146` |
| Native runtime | `current_root`, `native_runtime.rs:19` | **no** |
| Service start | `service.rs:321` | **no** |
| OMP keeper | `pointer_version`, `crates/omp-keeper/src/resolve.rs:45-62` | yes, `resolve.rs:54-59` |
| OMP loader | `parseNativePointer`, `parsePointer`, `adapters/omp/installed-loader.ts:243-279`; used at `installed-loader.ts:579-603` | yes, and component compatibility at `installed-loader.ts:616-622` |

## Transitions today

| Pointer | Move | Writer |
|---|---|---|
| Native | `Absent`/`Fresh`/`Paired` → `Fresh`/`Paired` (install) | `installer.rs:436-441` |
| Component | → new release (install, when the active adapter is not compatible) | `component_transition_for`, `installer.rs:912-943`; written at `installer.rs:442-444` |
| Native | `Paired` → `Paired` with the two versions swapped (rollback) | `installer.rs:526-531` |
| Component | → fallback of the older release (rollback) | `installer.rs:517-523`, `installer.rs:538-546` |
| Native | restore after a failed rollback step | `restore_activation_pointers`, `installer.rs:951-968` → `restore_native_pointer`, `installer.rs:945-950` (writes at `installer.rs:947`, removes at `installer.rs:948`) |
| Component | restore after a failed rollback step | `installer.rs:960` → `restore_component_pointer`, `installer.rs:705-710` (writes at `installer.rs:707`, removes at `installer.rs:708`) |
| Both | restore after a failed install, raw bytes | `restore_failed_install`, `installer.rs:1072-1080` → `restore_file_snapshot`, `installer.rs:1056-1061` |
| Component | → new release (`install-omp-adapter`) | `installer.rs:593-601` |
| Component | → named or previous release (`rollback-omp-adapter`) | `installer.rs:640-648` |

The validated writers are `write_current` (`installer.rs:1327-1338`) and `write_component_pointer` (`installer.rs:697-703`).
Both use `write_atomic`, so each single file changes atomically.

The cross-file order in an install:

1. Native pointer: `installer.rs:441`.
2. Component pointer: `installer.rs:443`.
3. Database migration: `installer.rs:445`.
4. Service start and ready wait: `installer.rs:446-452`.

The cross-file order in a rollback:

1. Service stop: `installer.rs:525`.
2. Native pointer: `installer.rs:531`.
3. Component pointer: `installer.rs:539`.
4. Database restore: `installer.rs:547`.
5. Service start and ready wait: `installer.rs:555-558`.

A failure in an install step starts `restore_failed_install` (`installer.rs:484`).
A failure in a rollback step starts an inline restore at `installer.rs:532`, `installer.rs:540`, `installer.rs:548`, or `installer.rs:561`.
Both restores run only in the process that made the change.

## Rules

| # | Rule | Status | Evidence |
|---|---|---|---|
| L1 | One installer operation runs at a time. | holds | `OperationLock::acquire` at `installer.rs:310`, `installer.rs:498`, `installer.rs:576`, `installer.rs:607`; doctor at `lib.rs:45` |
| L2 | `version` is safe, and `previous_version` is safe and differs from `version`. | holds at the writer; **broken at two readers** | Writer check `installer.rs:1328-1335`. See defect D5. |
| L3 | `previous_version` set implies `rollback_backup` set. | holds at the writers; the type permits a break | Install takes the backup at `installer.rs:392` when a release exists, and `native_runtime.rs:221-223` never returns `None`. Rollback refuses the break at `installer.rs:506-509`. |
| L4 | The native pointer names a retained, verified release. | holds | Verify before activation `installer.rs:415-417`, `installer.rs:510-513`; prune keeps current and previous `installer.rs:840-846` |
| L5 | The component pointer names a retained, verified release, and its previous release is retained. | holds | `installer.rs:663-693`; `component.rs:209-215`; prune keeps both `installer.rs:889-896` |
| L6 | The active adapter is compatible with the active native release. | holds in one process; **broken across a crash** | Writers check at `installer.rs:582`, `installer.rs:638`, `installer.rs:923`, `installer.rs:935`. See defect D1. |
| L7 | The database schema matches the native pointer's version. | **broken across a crash** | See defects D1 and D2. |
| L8 | Every pointer write goes through the validated writer. | **broken** | See defect D4. |

## Defects

- **D1. An install that stops between files leaves the pointers ahead of the database.**
  `installer.rs:441` writes the native pointer before `installer.rs:443` writes the component pointer.
  `installer.rs:445` migrates the database after both.
  The undo plan `InstallRollback` (`installer.rs:287`) is in memory only.
  If the process stops after `installer.rs:441`, no later run restores the prior state.
  The next service start (`service.rs:321`) then runs the new version.
  If the process stops between `installer.rs:441` and `installer.rs:443`, the adapter can be incompatible with the native release.
  The OMP loader refuses this case at `installed-loader.ts:616-622`, so the adapter fails closed.
  Whether the Host migrates the database on its own start is `unverified`.
- **D2. A rollback that stops between steps leaves the old pointer over the newer schema.**
  `installer.rs:531` activates the older release before `installer.rs:547` restores its database.
  If the process stops between these lines, the service stays stopped (`installer.rs:525`).
  The next start runs the older release against the newer schema.
- **D3. A second rollback is a roll-forward.**
  Rollback writes `previous_version = Some(current.version)` at `installer.rs:528`.
  It writes `rollback_backup = undo_backup` at `installer.rs:529`, a backup taken under the newer schema at `installer.rs:524`.
  A second `athanor rollback` therefore activates the newer release again and restores that backup.
  Data written between the two commands is lost.
  The field name `previous_version` does not tell the operator this.
- **D4. The failed-install restore bypasses the validated writers.**
  `installer.rs:1072-1080` writes the captured bytes of both pointers through `restore_file_snapshot` (`installer.rs:1056-1061`).
  This skips the check in `write_current` (`installer.rs:1327-1335`) and `ComponentPointer::validate` (`installer.rs:698`).
  Today the bytes come from files that the same process read and checked at `installer.rs:319` and `installer.rs:917`.
  The risk is low; the path still makes a second owner of the pointer bytes.
- **D5. The lineage rule has five copies, and two readers skip it.**
  The copies are `installer.rs:1316-1323`, `installer.rs:1328-1335`, `lib.rs:52-55`, `resolve.rs:54-59`, and `installed-loader.ts:250-256`.
  `native_runtime.rs:19` and `service.rs:321` parse `CurrentRelease` and use `version` as a path segment with no `safe_version` check.
  `layout.version` at `layout.rs:54-56` joins the value directly.
- **D6. A failed pointer restore stops the rest of the rollback recovery.**
  `installer.rs:532`, `installer.rs:540`, `installer.rs:548`, and `installer.rs:561` call `restore_activation_pointers(...)?`.
  If that call fails, the function returns at once.
  At `installer.rs:548` and `installer.rs:561`, the undo database restore (`installer.rs:550`, `installer.rs:563`) and the service start do not run.
- **D7. The type permits contradictory combinations.**
  `CurrentRelease` permits `previous_version` without `rollback_backup` (`installer.rs:45-47`).
  A first install with an external database writes `rollback_backup` without `previous_version` (`installer.rs:432-439`).
  The writers keep the first case out today; the second case is harmless and has no reader.

## Target shape

1. **Single transition owner: Rust step function.**
   Make one module own both pointers and the database step.
   Give it one pure `step(state, event) → (state, effects)` over a tagged `InstallState`.
   The states are `Stable`, `Activating { from, to, phase }`, and `RollingBack { from, to, phase }`.
   No SQL trigger applies, because the state lives in files.
2. **Journal on disk.**
   Write `Activating` or `RollingBack` to an atomic journal file before the first pointer write.
   Clear it after the ready wait.
   The service start and the installer read the journal first, and finish or undo the open operation.
   This fixes D1, D2, and D6.
3. **One writer, one parser.**
   Route the restore at `installer.rs:1072-1080` through `write_current` and `write_component_pointer`.
   Move the lineage check into one `CurrentRelease::validate`, and call it from every Rust reader.
   This fixes D4 and D5 in Rust. The keeper and the loader keep their own copies, and a contract test compares them.
4. **Type.**
   Replace `previous_version` and `rollback_backup` with one optional `rollback: { version, backup }`.
   This fixes D7 and makes L3 hold by construction.
5. **Bend2 model.**
   Model the two pointers, the schema version, and a crash event at each step.
   Prove L6 and L7 after crash and recovery.
   The proof must fail on L6 and L7 against today's transitions; this shows that it catches real defects.
6. **Contract test.** CI runs the same random event sequences, crashes included, through the model and through `step`, and compares the states.

## Open rulings

- **R1. D3: keep or remove the roll-forward.** Proposal: keep the toggle, and rename the field to `rollback.version`. Show the target version and the backup time before each rollback. The toggle is useful, and the operator must see that it restores an older backup.
- **R2. Recovery after a crash.** Proposal: add the journal from Target shape step 2. On an open `Activating` journal, undo to the `from` release. On an open `RollingBack` journal, undo to the `from` release too. Undo is safer because the `from` release was the last ready state.
- **R3. Migration order.** Proposal: migrate the database before the native pointer write. A crash then leaves the old pointer over a newer schema, which equals D2. One recovery rule then covers install and rollback. This needs proof that the old release refuses a newer schema; that proof is `unverified` today.
- **R4. Loader and keeper copies of the lineage rule.** Proposal: keep the copies, because they run in other processes and languages. Add one shared fixture set of valid and invalid pointers, and test all three parsers against it.
