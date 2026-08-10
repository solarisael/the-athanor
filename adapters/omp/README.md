# The Athanor — OMP adapter

This directory is a component of the [The Athanor](../../README.md) repository.
It is not a separate checkout and not a separate release.

The adapter connects The Athanor to [Oh My Pi (OMP)](https://github.com/can1357/oh-my-pi).
In an installed tree it lives at `<target>/the-athanor/adapters/omp`.

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
- Athanor organs for room state, memory, recall, lessons, paper boats, and routing;
- native Vault retrieval over Markdown, JSON, JSONL, and text roots, with
  exact-content and field-aware BM25F lanes plus attributed excerpts;
- room-local conversation logging and compact live context;
- a long-lived Rust transport for authoritative AKASHA memory operations;
- a hygiene extension that keeps host-generated context out of authored continuity;
- state-conditioned Striatum activation that keeps up to six eligible coding or
  project lessons warm;
- a localhost administrative GUI;
- the portable bundle builder, the installer, the updater, and the verifier.

TypeScript owns the OMP lifecycle, room discovery, context shaping, packaging,
and installation. Rust owns the shared contracts and the authoritative AKASHA
process.

The adapter fails open. An absent substrate is valid Vault. A configured but
unhealthy database, embedder, or substrate binary reports `degraded`, never
healthy AKASHA.

## Modules

| File | Role |
|---|---|
| `index.ts` | OMP extension entrypoint; exports `ADAPTER_API_VERSION` |
| `hygiene.ts` | Second OMP extension entrypoint |
| `install-layout.ts` | The one description of installed topology and archive names |
| `installer.ts` | The public installation door; compiles to `install.exe` |
| `install-legacy.ts` | 0.10.x detection and the AKASHA backup gate |
| `updater.ts` | Release resolution and staged update; compiles to `update.exe` |
| `verify-install.ts` | The canonical verifier |
| `build-portable.ts` | Builds the two public archives |
| `build-release-manifest.ts` | Writes the single `release-manifest.json` |
| `harnesses.ts` | The public harness catalog |

`install-layout.ts` is the single source of path truth. Every other door reads
it, so a path can never mean two things in two files.

## Build the archives

Run from the repository root. Both profiles build for the host platform only.

```text
bun install
bun run adapters/omp/build-portable.ts --out-dir dist --profile all
```

That writes two archives:

```text
dist/the-athanor-<version>-windows-x64-vault.zip
dist/the-athanor-<version>-windows-x64-akasha.zip
```

The AKASHA build needs a substrate executable. Set `ATHANOR_SUBSTRATE_EXE` to
its absolute path, or set `ATHANOR_AUTO=1`. Build one with:

```text
cargo build --release --locked -p athanor-substrate
```

Use `--profile vault` or `--profile akasha` to build one archive.

The builder refuses to ship generated, cached, or credential-bearing files. The
Vault archive carries no substrate binary, no substrate operations, and no
PostgreSQL, embedding, WSL, or Rust runtime asset.

## Release pipeline

A `v*` tag or a manual dispatch runs the Windows x64 release job. It builds the
substrate binary, compiles `install.exe` and `update.exe`, builds both archives,
runs the tests, and publishes the release.

The job also writes one `release-manifest.json`. That manifest records the
channel, the version, the tag, the repository, the required substrate schema,
and each asset's name, SHA-256 hash, and byte size. The updater fetches it by
name.

The package version must match the requested release version. A stable release
rejects a prerelease version. A beta or experimental release requires the
matching prerelease marker.

## Verify a tree

```text
bun run verify-install.ts --room "<ABSOLUTE_ROOM_PATH>" --profile vault
```

Add `--profile akasha` for an AKASHA tree. Add `--require-manifest` to also check
the package manifest, the artifact hashes, and the installed topology. A
development checkout has no `rooms/` or `state/` sibling, so omit
`--require-manifest` there.

Read the `mode` field. It is exactly `Vault`, exactly `AKASHA`, or `degraded`.

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
