# Vocabulary

Status: current. Verified against the code at commit `a6ab453` on 2026-10-04. Each row names its evidence. The rows from `harness` to `quest` were added on 2026-11-21 and name evidence at commit `3acbbf9`. Rows marked **Not re-verified at a6ab453** name the code that would decide them. Planned work lives in [`ROADMAP.md`](./ROADMAP.md). What the code does lives in [`ARCHITECTURE.md`](./ARCHITECTURE.md). The earlier naming record is [`history/2026-10-04-PRODUCT_ARCHITECTURE.md`](./history/2026-10-04-PRODUCT_ARCHITECTURE.md).

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
| spirit | The active room identity in native state. Embodiment does not rename the configured agent. Person-level authentication remains separate. | `crates/host/src/room_state.rs`; `crates/host/src/store.rs` |
| operator | The person who keeps the House. Each room has exactly one operator name. No person identity exists. | `room.ts:77-84`; [`LIMITATIONS.md`](./LIMITATIONS.md#4-identity) |
| Host | The one `athanor.exe` process that serves every room on loopback. | `crates/host/src/house.rs:86-105,166-202` |
| Pulse | The operator's web surface. `pulse.exe` or `bun gui-prototype/serve.ts` serves one room on `127.0.0.1:4175`. | `serve.ts:17-18,32-41`; `gui-desktop/src/main.rs:54-56` |
| keeper | `athanor.exe keeper` runs one OMP session for one room. Exit code 87 counts as an armed restart. | `crates/omp-keeper/src/keeper.rs:194-547,863-918` |
| substrate | Native storage and domain execution. The Host calls the libraries directly; the binary serves administration and keeper operations. | `crates/akasha/src/native/`; `crates/host/src/organ/` |
| organs | The House tools a spirit calls. 43 are registered. | `adapters/omp/house-proof/tools.ts:491-2011`; `restart-door.ts:574` |
| Vault | The file-backed storage profile. **Not re-verified at a6ab453**: `crates/vault` decides. Every supported install runs PostgreSQL. | `databaseMode` is `managed` or `external` (`installer.rs:1229-1250`) |
| AKASHA | The PostgreSQL storage profile: canon, memories, lessons, and hybrid retrieval. Retrieval internals are **Not re-verified at a6ab453** (`crates/akasha`). | PostgreSQL 18.4-2 and pgvector 0.8.6 (`installer/dependencies.json:1-28`) |
| GIGA | Optional cognitive work above AKASHA. The Host owns explicit enablement and worker lifetime. | `crates/host/src/organ/giga.rs`; `adapters/omp/giga.ts` |
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
| Anamnesis | The Cabinet of counsel drawn from lived repetition. The Cabinet holds drawers (pillars and cycles) and repetitions for each room. Counsel is never authority. | `anamnesis` (`tools.ts:1500`); `anamnesis_write` (`tools.ts:1533`); `crates/akasha/src/anamnesis.rs:1-2` |
| spellbook | A shared House page with room-key overrides. It carries the common organ guide and named familiars. | `crates/host/src/routing.rs`; `routing/spellbook.rs`; `server/context_session/assembly.rs` |
| familiars and lanes | Four worker lanes exist: `smol-scout`, `smol-executor`, `tester`, and `verifier`. The effective spellbook binds familiars to lanes. Dispatch returns a packet. | `crates/host/src/routing.rs`; `routing/dispatch.rs` |
| harness | The agent program that runs a session, for example OMP. The harness runs the agents, the workers, and the tools. The Athanor is an extension inside it. | [`HARNESS_EXTENSION.md`](./HARNESS_EXTENSION.md); `crates/athanor-install/src/harness/mod.rs:1-12` |
| adapter | The component that connects one harness to the Host. The OMP adapter registers the organs, translates events, and puts context into each turn. | `adapters/omp/README.md:8` |
| Bellows | The House word for an adapter: the connection that brings air to the furnace. Each harness has one bellows. The OMP bellows is `adapters/omp`. Code and folder names keep the word `adapter`. | Accepted by the operator on 2026-11-21 (issue #19) |
| Hearth | The crate of pure domain rules that the other crates share. It does no input or output. The interface documents also use the word for the control center. | `crates/hearth/src/lib.rs:1`; `docs/ARCHITECTURE.md:288` |
| Origami | The crate of House message shapes: paper boats, cranes, and Hallways. PostgreSQL keeps each body. | `crates/origami/src/lib.rs:1-4` |
| crane | A message that moves to a destination. It carries a pointer and never a body. The outbox and the NATS broker deliver it. | `crates/origami/src/cranes/FEATURES.md:6,40` |
| Orihon | The append-only book of conversation records for each room. It is provenance. It is not memory and not canon. | `crates/origami/src/orihon/FEATURES.md:5,82-85`; `orihon` (`tools.ts:420`) |
| fold | A summary node above exact Orihon records. No current path makes new folds; the tool `unfold` reads the old ones. | `crates/origami/src/orihon/folds/mod.rs:1,19-24`; `crates/origami/src/orihon/FEATURES.md:87-98` |
| zoom, unfold | Two organs that open the Orihon. `zoom` opens one line of the chat view. `unfold` opens one fold into its two children or reads the original records. | `tools.ts:450`; `tools.ts:538` |
| Jev | The remote judgment model, `jev-latest`. It scores Recall candidates, selects a Recall plan, and classifies turns. It runs only when the room has a grant. | `crates/akasha/src/typesafe.rs:6-7`; `crates/host/src/judgment.rs:23-24` |
| TypeSafe | The external provider that serves Jev. A judgment sends private conversation text to it. | `crates/akasha/src/typesafe.rs:6` |
| Laya | A judgment provider on loopback. A room grant can select it in place of TypeSafe. | `crates/host/src/judgment.rs:220-226` |
| Nemotron | The local embedding model, `Nemotron-3-Embed-1B`, with 2048 dimensions. Ollama runs it. | `crates/akasha/src/config.rs:27`; `substrate/migrations/0002_nemotron_2048.sql:1` |
| canon | A typed fact entity in PostgreSQL. Its authority state is active, superseded, or archived. A correction supersedes the old entity and does not overwrite it. | `crates/hearth/src/canon.rs:5-10` |
| lesson | A durable rule in a typed registry, separate from memory. The families are coding, project, writing, design, and audio. | [`LESSONS.md`](./LESSONS.md); `lessons` (`tools.ts`) |
| trigger | The `condition` (regular expression) or `ast_condition` (ast-grep pattern) of a lesson. A trigger is active only when the lesson has the tag `ttsr-approved`. | `crates/hearth/src/lesson_triggers.rs:3-4`; `crates/host/src/server/context_session/lessons.rs` (function `rule`) |
| working set | The current set of Recall results that the Host puts into the context of a session. Each new set replaces all earlier sets. | `crates/host/src/server/context_session/recall.rs` (the Recall block text); `crates/hearth/src/recall.rs:13` |
| mode door | The adapter hook that adds one line for the resolved Recall mode to a turn. The commands `/magus` (work), `/bard` and `/warlock` (conversation), `/rogue` (quiet), and `/warrior` (auto) set the requested mode. | `adapters/omp/house-proof/mode-door.ts:1-23` |
| boat door | The adapter hook that asks the spirit to write its paper boat with `sleep` before a handoff compacts the session. | `adapters/omp/house-proof/boat-door.ts:1-15` |
| kitten, dragonling | The 16 named familiar agents. Each one gets the shared role lessons and the lessons of its role at each wake. | `crates/hearth/src/worker_lifecycle/familiar.rs:3-10,68-85`; [`LESSONS.md`](./LESSONS.md) |
| `gu` | A swarm agent. Its name starts with `gu-`. The adapter joins the results of one swarm into a short list of links. | `adapters/omp/house-proof/swarm-delivery/index.ts:84` |
| quest | The unit of delegated work on the Docket. A draft binds nobody. Activation freezes the quest, and a claim is a separate consent. | `substrate/migrations/0023_docket.sql:223-227`; `crates/akasha/src/docket/mod.rs:1` |

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
| Substrate CLI | `athanor-substrate.exe` in the Windows payload | `installer/build-native-release.ps1`; `crates/akasha/src/main.rs` |
| Windows service | `SolarisaelAthanor`, display name `Solarisael Athanor` | `installer.rs:334-339`; `layout.rs:5-6`; `boundaries.rs:479-509` |
| Program root | `%ProgramFiles%/Solarisael/Athanor` | `crates/athanor-install/src/layout.rs:4,22-27,39-45` |
| Data root | `%ProgramData%/Solarisael/Athanor` | `layout.rs:4,22-27,39-45` |
| House id, reference House | `solarisael` | `C:/ProgramData/Solarisael/Athanor/config/runtime.json:1-37` |
| House id, no config supplied | `local` | `installer.rs:1167-1183` |
| Room marker | `.athanor-room.json`, laid over `<rooms>/house/.athanor-house.json`. A room key replaces the House value whole. `room` is never inherited. Native startup migrates legacy names before reading room state. | `crates/akasha/src/room_marker.rs`; `crates/host/src/store.rs` |
| Environment prefix | `ATHANOR_`, including Host, state, and native runtime settings | `crates/host/src/config.rs`; `adapters/omp/house-proof/host.ts` |

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
