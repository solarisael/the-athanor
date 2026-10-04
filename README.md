# The Athanor

**Make AI tools better at real work.**

The Athanor gives a tool-capable AI bounded access to the projects, decisions,
lessons, and prior work it needs for the current task. Retrieved material keeps
its source attached. Corrected knowledge can replace stale guidance without
erasing history. The model or provider carrying the work can change without
making the project start from zero again.

## Status

- The only supported target is native Windows x64 (`.github/workflows/release.yml:17-18`; `installer/athanor.iss:22`).
- OMP is the only supported harness. The runtime refuses the retired `driver` field (`crates/athanor-install/src/harness/config.rs:44-45,132-141`).
- The product version is `0.5.4` ([`package.json`](./package.json):3).
- The reference House runs `0.5.4+dev.202609282201.0e4ea3c` (`%ProgramFiles%/Solarisael/Athanor/current.json:2`).
- One Rust workspace holds the Host, the substrate, Vault, AKASHA, delivery, and the installer ([`Cargo.toml`](./Cargo.toml):3-15).

The docs marked "as built" were checked against the code at commit `a6ab453` on 2026-10-04.
A section that carries the label **Not re-verified at a6ab453** keeps an older claim that the census did not check.

## Choose your entrance

| You want to… | Start here |
|---|---|
| Understand what The Athanor is, in plain words | [Explaining The Athanor](./docs/EXPLAINING_THE_ATHANOR.md) |
| Install the supported release | [Install](./INSTALL.md) |
| Use rooms, organs, and Pulse every day | [Usage](./USAGE.md) |
| See how the processes fit together | [Architecture, as built](./docs/ARCHITECTURE.md) |
| Know what it cannot do today | [Limitations](./docs/LIMITATIONS.md) |
| Know what comes next | [Roadmap](./docs/ROADMAP.md) |
| Check the trust boundary | [Security](./docs/SECURITY.md) |
| Look up a House word | [Vocabulary](./docs/VOCABULARY.md) |
| Read the measurements | [Evidence](./docs/EVIDENCE.md) |
| Find a known defect | [Bugs](./BUGS.md) |

## What runs today

One Host process serves every room of the House.
It runs one listener and nests each room under `/room/<room>` (`crates/host/src/house.rs:86-105,166-202`).
The Host refuses a bind that is not loopback (`crates/host/src/config.rs:92-120`).
One bearer token guards the Host. It proves reach, not identity (`crates/host/src/server.rs:282,301-323`).

The OMP adapter registers 43 tools (`adapters/omp/house-proof/tools.ts:491-2011`; `restart-door.ts:574`).
Each tool delegates its behavior to the Rust substrate child or to the Host.
[Architecture § Tools and wires](./docs/ARCHITECTURE.md#43-tools-and-wires) names the wire for each tool.

The Host starts one keeper for each harness with `autoStart` set.
The keeper starts OMP, watches it, and relaunches it after an armed restart (`crates/omp-keeper/src/keeper.rs:194-547`).
The service supervisor starts the packaged PostgreSQL and NATS with JetStream (`crates/athanor-install/src/supervisor.rs:417-459`).

### Pulse

Pulse is the web operator surface in `gui-prototype/`.
Pulse is not read-only. It reads room state and it sends chat says.
The proxy allows only `POST` to `/live/*` routes (`gui-prototype/serve.ts:41,44,49-52,58`).
The allowlist includes `/live/chat/say` (`gui-prototype/live-routes.json:15`).
`chat.js` posts a say to that route (`gui-prototype/chat.js:146`).
The Host writes the say and publishes a delta (`crates/host/src/surface.rs:60-95`).

Start Pulse from the repository root:

```powershell
bun gui-prototype/serve.ts
```

The proxy binds `127.0.0.1:4175` and serves one room, `kodo` by default.
It reads `runtime.json` and the Host token from fixed `C:/ProgramData` paths (`serve.ts:16-18,32-33`).
`gui-desktop` ships the same surface as `pulse.exe` (`gui-desktop/src/main.rs:30,65-67`).
Read [Architecture § Pulse](./docs/ARCHITECTURE.md#6-pulse) for the full contract.

**Warning:** the chat ring lives in Host memory only.
A Host restart empties the ring and the drafts. The sequence starts again at 0 (`crates/host/src/chat.rs:6-7,25-30`; `server.rs:218`).
Only idempotency receipts, sessions, and the recall-policy cursor persist (`crates/host/src/store.rs:285-441`).

## Vault and AKASHA

**Not re-verified at a6ab453.** The census did not cover `crates/vault` or `crates/akasha`.

Vault is the lightweight profile. It searches Markdown, JSON, JSONL, and plain text in configured project roots.
It needs no PostgreSQL, embeddings, or GPU.
AKASHA adds a PostgreSQL authority layer for canon, memories, typed lessons, and GIGA candidates.
The release ships pgvector 0.8.6 for AKASHA (`installer/dependencies.json:1-28`).
Read [Retrieval](./docs/RETRIEVAL.md), [Lessons](./docs/LESSONS.md), and [Hippocampus](./docs/HIPPOCAMPUS.md) for the contracts.

The adapter treats a folder as a room when it holds `.athanor-room.json`, `active_spirit.md`, or `.omp/runtime/athanor-house-state.json` (`adapters/omp/house-proof/room.ts:86-127`).

The optional local workspace search adapter has its own manual in [`adapters/workspace-search/README.md`](./adapters/workspace-search/README.md).

## Install

One release owns the substrate, the Host, delivery, the OMP adapter, and the installer (`installer/build-native-release.ps1:146-167`).
The package is one checksum-published Windows x64 installer (`.github/workflows/release.yml:38-50,66-74`):

```text
The-Athanor-<version>-windows-x64.exe
The-Athanor-<version>-windows-x64.exe.sha256
```

The installer carries EnterpriseDB PostgreSQL 18.4-2, pgvector 0.8.6, and NATS Server 2.14.4 (`crates/athanor-install/src/manifest.rs:11-13,31-38`).
The service needs no WSL, Python, Bun, Cargo, or separate database or broker (`supervisor.rs:417-459`).

- Immutable versions live under `%ProgramFiles%\Solarisael\Athanor\versions` (`crates/athanor-install/src/layout.rs:4,22-27`).
- The database, backups, configuration, logs, and ACL-restricted secrets live under `%ProgramData%\Solarisael\Athanor`. They survive an ordinary uninstall.
- Rooms live under `roomsRoot` from `runtime.json`. The reference install uses `C:/Solarisael/Obsidian/obsidian`. `%ProgramData%\Solarisael\Athanor\rooms` is only the default when no configuration exists (`crates/athanor-install/src/installer.rs:1167-1183`).
- An advanced mode uses an external PostgreSQL on loopback (`supervisor.rs:282-299,322-327`).

The installer verifies every staged artifact before activation (`installer.rs:289-305,914-918,1035-1056`).
It backs up the database before an upgrade, runs migrations, and waits for service readiness (`installer.rs:279-281,333-340`).
Rollback uses the retained version pointer and the pre-change database backup (`installer.rs:43-48,324-329`).

Read [Install](./INSTALL.md) for checksum verification, rollback, uninstall, and purge.
Read [Architecture § Installed layout](./docs/ARCHITECTURE.md#7-installed-layout) for every installed file.

## Components

Every workspace member and every shipped surface lives in [`solarisael/the-athanor`](https://github.com/solarisael/the-athanor) and ships in one release.

| Component | Owns | Evidence |
|---|---|---|
| `crates/hearth` | provider-neutral domain types and the substrate schema version | Not re-verified at a6ab453 |
| `crates/protocol` | the wire contracts: client commands, events, chat and Presence types | `crates/protocol/src/host.rs` |
| `crates/summoning` | Presence request and result types, Anamnesis, paper boat bodies | `protocol/src/host.rs:9-12` imports `summoning::presence`. The rest is not re-verified |
| `crates/host` | the one authenticated multi-room Host, the chat ring, snapshots and deltas | `crates/host/src/house.rs:86,184-189,205-234` |
| `crates/akasha` | PostgreSQL authority: canon, memory, lessons, GIGA, Docket, Insula | Not re-verified at a6ab453 |
| `crates/origami` | paper boats, cranes, Hallways, NATS delivery | Not re-verified at a6ab453 |
| `crates/vault` | database-free file retrieval | Not re-verified at a6ab453 |
| `crates/athanor-install` | `athanor.exe`: service, supervisor, installer, updater, rollback, doctor, uninstall, purge | `crates/athanor-install/src/service.rs:113-308`; `cli/manage.rs:32-134` |
| `crates/omp-keeper` | the keeper that starts, watches, and relaunches OMP for one room | `crates/omp-keeper/src/keeper.rs:188,812-823` |
| `crates/interactive-process` | the keeper's new-window process launch | `crates/omp-keeper/src/keeper.rs:843-856` |
| `gui-desktop` | `pulse.exe`, a desktop window that embeds `gui-prototype` | `gui-desktop/src/main.rs:30,65-67` |
| `gui-prototype` | Pulse, the web operator surface, and its loopback proxy `serve.ts` | `gui-prototype/serve.ts:41-58` |
| `adapters/omp` | OMP hooks and 43 tools that delegate to the substrate or the Host | `adapters/omp/house-proof/tools.ts:491-2011` |
| `adapters/workspace-search` | optional zvec-grep search over repository files | Not re-verified at a6ab453 |
| `installer/` | the Inno Setup script for the Windows installer | `installer/athanor.iss:22,32-33` |

**Not re-verified at a6ab453:** the public interface specimen at <https://solarisael.github.io/the-athanor/>.
It is built from `site/` by `scripts/build-pages.mjs` with browser-only fixtures. It connects to no House.

## Documentation

Every current document is in this list.
Dated records live in [`docs/history/`](./docs/history/). They are provenance, not current contracts.

- [Install](./INSTALL.md): the supported release, checksums, rollback, and purge.
- [Usage](./USAGE.md): rooms, organs, and Pulse in daily work.
- [Identity guide](./IDENTITY_GUIDE.md): how to write a room identity.
- [House](./HOUSE.md): the personal reason behind The Athanor.
- [Agents](./AGENTS.md): rules for agents that change this repository.
- [Bugs](./BUGS.md): known defects.
- [Changelog](./CHANGELOG.md): release history, including retained RC builds.
- [Lesson map](./LESSON_MAP.md): repository lessons by number. The PostgreSQL registry stays authoritative.
- [Explaining The Athanor](./docs/EXPLAINING_THE_ATHANOR.md): the concept map.
- [For latent-space explorers](./docs/FOR_EXPLORERS.md): the architectural argument.
- [Vocabulary](./docs/VOCABULARY.md): House words and product names.
- [Architecture, as built](./docs/ARCHITECTURE.md): processes, doors, and installed layout.
- [Limitations](./docs/LIMITATIONS.md): what the code cannot do today.
- [Roadmap](./docs/ROADMAP.md): planned work.
- [Security](./docs/SECURITY.md): the trust boundary.
- [Evidence](./docs/EVIDENCE.md): measurements and dated proof.
- [Retrieval](./docs/RETRIEVAL.md): Recall and Vault contracts.
- [Lessons](./docs/LESSONS.md): typed lessons.
- [Hippocampus](./docs/HIPPOCAMPUS.md): GIGA candidates and their authority.
- [OMP adapter](./adapters/omp/README.md)
- [Workspace search adapter](./adapters/workspace-search/README.md)
- [OMP keeper](./crates/omp-keeper/README.md)
- Crate features: [hearth](./crates/hearth/FEATURES.md), [protocol](./crates/protocol/FEATURES.md), [summoning](./crates/summoning/FEATURES.md), [host](./crates/host/FEATURES.md), [akasha](./crates/akasha/FEATURES.md), [vault](./crates/vault/FEATURES.md), [athanor-install](./crates/athanor-install/FEATURES.md)
- Pulse maps: [navigation](./gui-prototype/NAVIGATION_MAP.md), [lessons](./gui-prototype/LESSONS_MAP.md)

## License

The Athanor uses the Apache License 2.0. See [LICENSE](./LICENSE) and
[NOTICE](./NOTICE).
