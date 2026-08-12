# The Athanor — OMP adapter

This directory is a component of the [The Athanor](../../README.md) repository.
It is not a separate checkout and not a separate release.

The adapter connects The Athanor to [Oh My Pi (OMP)](https://github.com/can1357/oh-my-pi).
In an installed tree it lives under the active immutable product version.

**Do not install from this directory.** The one installation door is
[`INSTALL.md`](../../INSTALL.md) at the repository root.

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
| `solarisael-house-proof/` | Named tool schemas and thin Host/Rust adapters |
| `starter-room/` | Example room material |

Packaging, installation, migration, update, rollback, doctor, uninstall, and
purge are owned by `crates/athanor-install` and `installer/`, not this adapter.
The native release builder copies only the modules required at runtime; tests,
historical portable builders, and development assets are excluded.

## Build the native release

Run the checksum-pinned Windows assembly from a Visual Studio x64 developer
environment:

```powershell
pwsh -File installer/build-native-release.ps1 `
  -Version 0.9.3 `
  -OutDir dist/native
```

The script builds Rust into an isolated staging target, compiles pgvector into
the bundled EnterpriseDB tree, imports the Godot project against its
GDExtension, rejects Godot load errors, and writes a byte-size/SHA-256 manifest.
Inno Setup then compiles the single Windows installer from
`installer/athanor.iss`.

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
