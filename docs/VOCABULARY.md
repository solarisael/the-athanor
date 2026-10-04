# Vocabulary

Status: current. Verified against the code at commit `a6ab453` on 2026-10-04. Each row names its evidence. Rows marked **Not re-verified at a6ab453** name the code that would decide them. Planned work lives in [`ROADMAP.md`](./ROADMAP.md). What the code does lives in [`ARCHITECTURE.md`](./ARCHITECTURE.md). The earlier naming record is [`history/2026-10-04-PRODUCT_ARCHITECTURE.md`](./history/2026-10-04-PRODUCT_ARCHITECTURE.md).

## 1. Naming rules

- A name that a machine compares is a contract. Change it only with a migration that rewrites every stored copy.
- A name that a human reads is register. Change it freely, in one release, everywhere.
- The formal brand is **The Athanor**, with the capital article. Do not write bare "Athanor" as the product label.
- An athanor is an alchemical furnace that keeps steady heat through a long transformation. The House keeps identity steady while models change.
- Write **House** when the word names the continuity domain. Write "house" for the ordinary meaning.
- Do not use "mode" as a generic word. Use profile, capability, layer, route, policy, or custody.
- The website keeps its alchemical stages (Nigredo, Albedo, Citrinitas, Rubedo, Codex) and the names Cinza and Suul. Do not reuse them for profiles, workers, or policies.

## 2. House nouns

| Word | Meaning | Evidence |
|---|---|---|
| The Athanor | The product. It installs, runs, and updates Houses on one machine. | Start menu entry `The Athanor` (`installer/athanor.iss:35-37`) |
| House | One continuity domain. It holds rooms, spirits, memories, lessons, and canon. One Host serves exactly one House. | Every room must share `house_id`, the bearer, `DATABASE_URL`, and NATS (`crates/host/src/house.rs:205-234`) |
| room | A directory that holds `.athanor-room.json`, `active_spirit.md`, or the House state file. The key is the marker's `room`, else the folder name in lowercase. | `adapters/omp/house-proof/room.ts:86-127` |
| spirit | The identity that governs a room. It comes from the marker's `trueName`, then `active_spirit.md`, then the persisted `embodiedSpirit`. The identity is self-asserted, not authenticated. | `room.ts:77-84,112-119`; `tools.ts:818-847` |
| operator | The person who keeps the House. Each room has exactly one operator name. No person identity exists. | `room.ts:77-84`; [`LIMITATIONS.md`](./LIMITATIONS.md#4-identity) |
| Host | The one `athanor.exe` process that serves every room on loopback. | `crates/host/src/house.rs:86-105,166-202` |
| Pulse | The operator's web surface. `pulse.exe` or `bun gui-prototype/serve.ts` serves one room on `127.0.0.1:4175`. | `serve.ts:17-18,32-41`; `gui-desktop/src/main.rs:54-56` |
| keeper | `athanor.exe keeper` runs one OMP session for one room. Exit code 87 counts as an armed restart. | `crates/omp-keeper/src/keeper.rs:194-547,863-918` |
| substrate | The `athanor-substrate` child process. The adapter speaks to it with one JSON line per request over stdio. | `adapters/omp/rust-transport.ts:569-575` |
| organs | The House tools a spirit calls. 43 are registered. | `adapters/omp/house-proof/tools.ts:491-2011`; `restart-door.ts:574` |
| Vault | The file-backed storage profile. **Not re-verified at a6ab453**: `crates/vault` decides. Every supported install runs PostgreSQL. | `databaseMode` is `managed` or `external` (`installer.rs:1229-1250`) |
| AKASHA | The PostgreSQL storage profile: canon, memories, lessons, and hybrid retrieval. Retrieval internals are **Not re-verified at a6ab453** (`crates/akasha`). | PostgreSQL 18.4-2 and pgvector 0.8.6 (`installer/dependencies.json:1-28`) |
| GIGA | Optional cognitive workers above AKASHA. The tools refuse unless `ATHANOR_GIGA_ENABLED=1`. | `adapters/omp/giga.ts:107-108`; 7 `giga_*` tools (`tools.ts:1377-1575`) |
| Hippocampus | The first GIGA worker. It proposes candidate memories and lessons. A candidate is never authority. **Not re-verified at a6ab453**: the GIGA crate decides. | — |
| Curios | Candidates that a governing spirit keeps for later review. **Not re-verified at a6ab453**: the GIGA store in `crates/akasha` decides. | — |
| Striatum | The GIGA slice that keeps reviewed lessons warm while a work state persists. **Not re-verified at a6ab453**: `crates/akasha` and `crates/hearth` decide. | Nearest row: the lesson TTSR bridge (`adapters/omp/index.ts:1033-1056`) |
| paper boat | The letter that `sleep` writes and `wake` delivers to the next session. | `adapters/omp/boat-door.ts:76-83,129-133,321-343`; `substrate.ts:541-556` |
| Hallway | A shared surface between rooms. It never merges their spirits or histories. | 7 `hallway_*` tools (`tools.ts:1596-1776`); one ChatLog per room (`house.rs:184-189`) |
| Bell | The automatic unread-inbox notice for Hallways. It is a projection, not a tool, and it needs the Host. | `adapters/omp/index.ts:1720`; `hallway.ts:26-30` |
| Knock | A request for one bounded turn in a peer room. The Host claims and settles it behind `knock_authority`. | `crates/host/src/server.rs:1649-1665`; `knock.ts:77-114` |
| Docket | The House quest board: offers, claims, receipts, and evidence. | 5 `quest_*` tools; room capability (`room.ts:147-155`) |
| Presence | One frame per session. It compiles the prompt, recalled set, lessons, and directives for each turn. | `adapters/omp/presence.ts:72-81,117-170`; `crates/host/src/presence.rs:155,174-188` |
| Recall | Retrieval of canon, memories, and chunks. The Host owns the proactive Recall policy. | `crates/host/src/policy.rs:130-347`; `recall` needs the Host (`tools.ts:526-532`) |
| Insula | The House's body sense: events, vitals, spans, and retention. | `crates/host/src/insula.rs:27-38,272-284` |
| Anamnesis | The Cabinet of counsel drawn from lived repetition. Counsel is never authority. | `anamnesis` (`tools.ts:1290`); `anamnesis_write` (`tools.ts:1321`) |
| familiars and lanes | Four worker lanes exist: `smol-scout`, `smol-executor`, `tester`, `verifier`. A room's spellbook binds named familiars to them. Dispatch returns a spawn packet. The Host never spawns a worker. | `crates/host/src/routing.rs:100-157,300-301`; `routing/dispatch.rs:176-177,440-456` |

## 3. Planned names

These names have no code. Do not describe them as current. Each one waits in [`ROADMAP.md`](./ROADMAP.md).

- **OMEGA** (Organizational Memory, Encryption, Governance, and Access): governance for several Houses under one organization. Today one Host serves one House (`house.rs:205-234`).
- **Relay**: transient remote compute that returns output to an operator-owned House. Today every runtime address must be loopback (`crates/athanor-install/src/supervisor.rs:250-265`).
- **ANON** (Attested Nonpersistent One-shot Node): one attested private remote job with no retained state. No attestation code exists in the census.

## 4. Identifiers in the code

| Identifier | Value | Evidence |
|---|---|---|
| Repository | `solarisael/the-athanor` | `.git/config:9` |
| Release manifest product | `the-athanor` | `crates/athanor-install/src/manifest.rs:7-11` |
| Release artifact | `The-Athanor-<version>-windows-x64.exe` | `.github/workflows/release.yml:66-74` |
| Executable | `athanor.exe`: Host, service, installer, keeper, chat, and status | `installer/athanor.iss:27`; `installer.rs:307-318`; `layout.rs:57-61` |
| Substrate | `athanor-substrate.exe` on Windows; `athanor-substrate` on `linux-x64` and `linux-arm64` | `installer/build-native-release.ps1:146-167`; `adapters/omp/discovery.ts:17-26` |
| Windows service | `SolarisaelAthanor`, display name `Solarisael Athanor` | `installer.rs:334-339`; `layout.rs:5-6`; `boundaries.rs:479-509` |
| Program root | `%ProgramFiles%/Solarisael/Athanor` | `crates/athanor-install/src/layout.rs:4,22-27,39-45` |
| Data root | `%ProgramData%/Solarisael/Athanor` | `layout.rs:4,22-27,39-45` |
| House id, reference House | `solarisael` | `C:/ProgramData/Solarisael/Athanor/config/runtime.json:1-37` |
| House id, no config supplied | `local` | `installer.rs:1167-1183` |
| Room marker | `.athanor-room.json`. The legacy `.solarisael-room.json` is renamed in place. | `adapters/omp/house-proof/room.ts:17-28` |
| Environment prefix | `ATHANOR_`, for example `ATHANOR_PROGRAM_ROOT`, `ATHANOR_DATA_ROOT`, `ATHANOR_HOST_KNOCK_AUTONOMY`, `ATHANOR_HOST_TOKEN`, `ATHANOR_SUBSTRATE_EXE` | `layout.rs:29-37`; `crates/host/src/config.rs:4,21-41`; `host.ts:153`; `discovery.ts:58-78` |

Two exceptions to the prefix matter:

- The Host requires the unprefixed `DATABASE_URL` (`crates/host/src/house.rs:205-234`).
- The service ignores the `ATHANOR_PROGRAM_ROOT` and `ATHANOR_DATA_ROOT` overrides. `athanor` and `athanor start` honor them (`crates/athanor-install/src/service.rs:234-240`; `layout.rs:29-37`).

The census also shows other unprefixed variables: `PG_BIN_DIR` (`adapters/omp/installed-loader.ts:554-556,628-634`) and `PULSE_ROOM` and `PULSE_HOST_PORT` (`serve.ts:17,32,36-39`).

**Solarisael House** names only the reference House. It is not a product name. The service name, the install roots, and the reference House id keep the `Solarisael` token. Machines compare them, so they are contracts. The installed layout is in [`ARCHITECTURE.md`](./ARCHITECTURE.md#7-installed-layout).

## 5. Retired words

Use the word on the right in new text. A stored record can keep an old name only when its history labels it.

| Old word | Use |
|---|---|
| Solarisael House, as the platform name | The Athanor |
| Base, Base House | Vault |
| Full, Full House | AKASHA |
| Giga Mode, Giga House, Giga profile | GIGA |
| akashic write | durable write |
| organizational House | OMEGA (planned) |
| transient cloud worker | Relay (planned) |
| zero-retention worker | ANON (planned) |

`runtime.json` carries no storage-profile field (`installer.rs:1229-1250`). Whether other code still uses `Base` or `Full` ids is **Not re-verified at a6ab453**: a grep of `crates/` and `adapters/` would decide it.
