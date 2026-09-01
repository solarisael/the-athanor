# The Athanor — OMP adapter

This directory is a component source of the [The Athanor](../../README.md)
repository. It is not a separate checkout and it does not define a native
product version.

The adapter connects The Athanor to [Oh My Pi (OMP)](https://github.com/can1357/oh-my-pi).
An installed adapter lives independently at
`components/omp-adapter/versions/<releaseId>`. It is not copied into a native
`versions/<version>` directory.

Install and rollback are owned by the native manager. Do not write Program
Files from this directory or from a PowerShell builder.

## Where things live

| Document | Owns |
|---|---|
| [`INSTALL.md`](../../INSTALL.md) | Install, migrate, update, verify, remove |
| [`README.md`](../../README.md) | What The Athanor is and which profile to choose |
| [`USAGE.md`](../../USAGE.md) | Daily use |
| [`IDENTITY_GUIDE.md`](../../IDENTITY_GUIDE.md) | Co-authoring a room identity |
| [`docs/ARCHITECTURE.md`](../../docs/ARCHITECTURE.md) | Component ownership and installed layout |

## What this adapter adds

- OMP lifecycle hooks for room context and end-of-session continuity;
- 26 named Athanor organs for state, retrieval, memory, typed lessons, counsel,
  Paper Boats, GIGA, and routing;
- Host-owned Recall Policy presentation and bounded working-set injection;
- automatic bounded-worker lineage joined across OMP progress/lifecycle events;
- room-local conversation logging and compact live context;
- long-lived Rust transports for Vault and AKASHA behavior.

TypeScript owns only the harness-facing registration, room discovery, lifecycle
translation, bounded context presentation, and Rust transport. Rust owns domain
validation, authority, retrieval, persistence, Recall Policy state, delivery,
health, migrations, backup/restore, and native lifecycle.

The adapter fails open for conversation continuity. A configured but unhealthy
AKASHA dependency reports `degraded`; it never invents healthy state or silently
becomes a second authority.


## Runtime modules

| File or directory | Role |
|---|---|
| `index.ts` | OMP extension entrypoint |
| `athanor-root.ts` | Installed/development root resolution |
| `discovery.ts` | Rust executable discovery |
| `rust-transport.ts` | Bounded long-lived JSONL transport |
| `giga.ts` | OMP-side GIGA event bridge; Rust owns claims and transitions |
| `kitten-lineage.ts` | OMP task-lineage observation |
| `house-proof/` | Named tool schemas and thin Host/Rust adapters |
| `starter-room/` | Example room material |

Packaging, installation, migration, update, rollback, doctor, uninstall, and
purge are owned by `crates/athanor-install`. PowerShell builds source artifacts
only. Rust validates installed artifacts, writes component pointers, and changes
installed state.

## Build and deploy the adapter component

Build a component bundle with the shared builder:

```powershell
. installer/omp-adapter-component.ps1
New-OmpAdapterComponentBundle -RepositoryRoot . -Destination dist/omp-adapter
```

The output root contains `component-manifest.json` and all declared artifacts.
The manifest records a content-derived `releaseId`, exact native compatibility
fields, and SHA-256 and size values for every artifact. The manager stages and
validates this root before an atomic component-pointer update.

For the adapter-only speed path, run:

```powershell
bun run deploy:omp-adapter
```

That command runs the adapter tests, builds a temporary component bundle, and
calls the stable manager. It does not rebuild, replace, or mutate a native
product release.

Deploy an already-built component with the stable manager:

```powershell
& "$env:ProgramFiles\Solarisael\Athanor\bin\athanor-manage.exe" `
  install-omp-adapter --source <component-root>
```

This adapter-only path does not rebuild, replace, or mutate a native product
release. It reuses an identical valid component release when one exists.

Roll back the adapter without changing the native product:

```powershell
& "$env:ProgramFiles\Solarisael\Athanor\bin\athanor-manage.exe" `
  rollback-omp-adapter

& "$env:ProgramFiles\Solarisael\Athanor\bin\athanor-manage.exe" `
  rollback-omp-adapter --release-id <releaseId>
```

Rollback refuses a target whose four compatibility fields do not exactly match
the active native product. If an activation fails, the manager restores the
prior component and native activation state.

## Build the native release

Run the checksum-pinned Windows assembly from a Visual Studio x64 developer
environment:

```powershell
pwsh -File installer/build-native-release.ps1 `
  -OutDir dist/native
```

The native release assembles its fallback `components/omp-adapter` bundle
through the shared component builder. It does not make the adapter part of the
native version identity.

## Verify an installed tree

```powershell
& "$env:ProgramFiles\Solarisael\Athanor\bin\athanor-manage.exe" doctor
```

Doctor validates the activation pointer, release manifest and every installed
artifact, service registration, and persistent data root. It exits nonzero on a
failed contract. Runtime surfaces report exactly `Vault`, `AKASHA`, or
`degraded`; no adapter fallback fabricates a healthier mode.

## Retrieval evaluation

The sanitized [`2026-07-22 room retrieval pilot`](./evals/2026-07-22-room-retrieval-pilot.json)
measured exact-title lookup across ten unique active room-owned memories in each
of two rooms. It observed 95% combined viewport recall and 80% combined top-1
recall.

That pilot is a small favorable-phrasing calibration. It is not a paraphrase or
answer-quality benchmark. Raw prompts, memory identifiers, excerpts, and
telemetry remain private.

## Test

```text
bun test
```

Licensed under Apache-2.0. Original project and design by Sol; see
[`NOTICE`](../../NOTICE).
