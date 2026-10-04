# Roadmap

Status: planned work only. Checked against the census of commit `a6ab453` (`dev/next`) on 2026-10-04.

This page lists work that is not done. It does not describe the current system. For what the code does, read [`ARCHITECTURE.md`](./ARCHITECTURE.md). For the limits of the code, read [`LIMITATIONS.md`](./LIMITATIONS.md). For words, read [`VOCABULARY.md`](./VOCABULARY.md). For measured results, read [`EVIDENCE.md`](./EVIDENCE.md).

The product version is `0.5.4` (`package.json:3`). The labels `0.9.6`, `0.11.0`, and `1.0.0-rc.3` are history. They are numerically above the current version, so do not read them as progress.

## Status guide

Each row on this page has exactly one of these states.

| State | Meaning |
|---|---|
| Built | A census row confirms the code at `a6ab453`. Any later work is named in the row. |
| Partly built | A census row confirms some of the code. The row names the missing part. |
| Planned | The roadmap includes the work. No code exists for it. |
| Specified | An accepted technical contract exists. No code exists for it. |
| Research | The idea needs product and safety work before a contract. |
| Parked | The work is kept, but it is not part of the active product. |
| Not re-verified | The census did not reach the code that decides the state. The row names that crate. |

A Built or Partly built state is a census result for one slice. It does not mean that the full promise is delivered or that its benefit is measured.

## Feature map

| Feature | Promise | State | Evidence and remaining work |
|---|---|---|---|
| GIGA Hippocampus Stage 1 | Notice possible memories and lessons, then keep them non-authoritative until review | Built | Seven `giga_*` tools, including review and promote (`adapters/omp/house-proof/tools.ts:1377-1575`). Off unless `ATHANOR_GIGA_ENABLED=1` (`giga.ts:107-108`). Useful classification, consolidation, and later benefit are not proved. |
| Curios | Keep selected hunches until later context makes them meaningful | Not re-verified | Not re-verified at `a6ab453`. Deciding crate: `crates/akasha`. Automatic resurfacing is Planned. |
| GIGA Striatum | Keep the right reviewed lessons warm while a work state persists | Not re-verified | Not re-verified at `a6ab453`. Deciding crate: `crates/akasha`. The nearest census row is the adapter lesson bridge (`adapters/omp/index.ts:1033-1056`). Learned work-state behavior is Planned after Docket, shadow first. |
| Web operator surface | Show House state and send chat through a loopback proxy (`serve.ts` or `pulse.exe`) | Built | Not read-only: the allow-list includes `/live/chat/say` (`gui-prototype/live-routes.json:15`; `chat.js:146`). See [Pulse](./ARCHITECTURE.md#6-pulse). Login and authenticated writes are Planned. |
| Athanor Host | Give clients one authenticated snapshot, delta, and resync surface with restart-safe cursors and idempotency | Built | Resync (`crates/host/src/server.rs:2551-2566`). Replay ledger of 64 entries (`presence.rs:12-14`). Recall cursor and receipts persist (`store.rs:15,285-441`). Chat persistence is Planned, see [the chat ring](./ARCHITECTURE.md#34-the-chat-ring). |
| Session Recall Policy | Make proactive retrieval visible and mode-aware | Built | `crates/host/src/policy.rs:130-347`; `recall_policy` tool (`tools.ts:1176`). |
| GIGA integrity and refinement transactions | Build candidates from explicit fresh evidence and compare predicted outcomes with observed results | Not re-verified | Not re-verified at `a6ab453`. Deciding crate: `crates/akasha`. |
| PostgreSQL outbox and NATS delivery | Deliver bounded opaque pointers with explicit duplicate windows and durable PostgreSQL idempotency | Partly built | Host `DeliveryService` (`crates/host/src/house.rs:235-248`). The Host also consumes Hallway post projections from JetStream (`server.rs:2855-2894`). The substrate outbox is not re-verified at `a6ab453`; deciding crate: `crates/akasha`. |
| Paper Boat sleep, wake, and delivery receipt | Commit the boat and outbox together, wake from PostgreSQL, and show only sanitized receipt metadata | Partly built | `sleep` closes Presence, then calls the substrate `paper_boat_sleep`, with backup on by default (`tools.ts:996`). `wake` calls `paper_boat_wake`. The single-transaction commit is not re-verified at `a6ab453`; deciding crates: `crates/origami`, `crates/akasha`. |
| Presence and restart | Keep a session oriented across close, reopen, process restart, and resumed work | Partly built | Presence reloads from PostgreSQL (`server.rs:1129-1133`). Generated-turn origins (`adapters/omp/house-proof/turn-origin.ts:7-29`). Host session attribution is not re-verified at `a6ab453`; deciding crate: `crates/host`. |
| Worker routing and familiars | Carry bounded work through room-owned lanes with evidence of its disposition | Built | Every receipt carries `executed: false` and a `spawnPacket` (`crates/host/src/routing/dispatch.rs:176-177,440-456`). Linking execution and evidence to Docket attempts is Planned. |
| Dynamic model and room execution | Choose local or hosted model bodies for workers, familiars, reflections, and live room dialogue | Partly built | Fixed `model_role` per lane (`crates/host/src/routing.rs:100-157`). Room default through `pi.setModel` (`tools.ts:1222,1278`). Dynamic routing is not built. |
| Incremental Prolog/Datalog derivations | Index code changes in the background and answer common queries from precomputed authorized relations | Not re-verified | Not re-verified at `a6ab453`. Deciding crate: none found. A census grep found no code row; that does not prove absence. |
| Lean-backed lesson obligations | Check selected invariants inside a resource-limited wrapper | Not re-verified | Not re-verified at `a6ab453`. Deciding crate: none found. |
| GIGA Cingulate | Detect workflow divergence and missing proof before a regression is accepted | Not re-verified | Not re-verified at `a6ab453`. Deciding crate: none found. Depends on reliable lifecycle and outcome evidence. |
| Bounded e-graph/egglog normalization | Canonicalize one small typed IR under reviewed rewrites | Not re-verified | Not re-verified at `a6ab453`. Deciding crate: none found. |
| Optional Z3 backend | Check SMT-shaped obligations and keep formulas, counterexamples, and inconclusive outcomes | Not re-verified | Not re-verified at `a6ab453`. Deciding crate: none found. |
| Bounded SyGuS repair | Synthesize small approved functions from reviewed grammars, then test and canary them | Not re-verified | Not re-verified at `a6ab453`. Deciding crate: none found. |
| Proof-guided repair trajectories | Feed counterexamples into bounded repair and keep reviewed trajectories for offline training | Not re-verified | Not re-verified at `a6ab453`. Deciding crate: none found. |
| Optional Wasmtime sandbox | Run compatible untrusted helpers with empty default capabilities and hard limits | Not re-verified | Not re-verified at `a6ab453`. Deciding crate: none found. |
| pgvector HNSW boundary | Keep semantic search in pgvector until a measured ceiling | Partly built | pgvector 0.8.6 is pinned (`crates/athanor-install/src/manifest.rs:11-13`; `installer/dependencies.json:1-28`). The HNSW index is not re-verified at `a6ab453`; deciding crate: `crates/akasha`. |
| In-world Godot client | Preserve the spatial presentation specification | Parked | The payload carries no Godot (`build-native-release.ps1:146-167`). The client lives in the private repository `solarisael/athanor-godot`, not re-verified. |
| Companion room sovereignty | Let governing companions create child rooms inside constitutional grants | Not re-verified | Not re-verified at `a6ab453`. Deciding crate: not named by the census. Related limit: adding a room needs a new release (`crates/athanor-install/src/installer.rs:217-223,240-274`). |
| Companion-authored models | Let companions start governed local model or LoRA training with lineage, evaluation, and rollback | Not re-verified | Not re-verified at `a6ab453`. Deciding crate: not named by the census. |
| BM25F lexical retrieval | Rank structured memory fields with a field-aware sparse baseline | Not re-verified | Not re-verified at `a6ab453`. Deciding crate: `crates/akasha`. |
| Nemotron-controlled lexical bridge | Expand through at most three stored concepts into a lower-priority BM25F lane | Not re-verified | Not re-verified at `a6ab453`. Deciding crate: `crates/akasha`. |
| Learned-sparse retrieval successor | Add a local learned lexical model only if measured misses justify it | Not re-verified | Not re-verified at `a6ab453`. Deciding crate: `crates/akasha`. The model choice is open. |
| Vault file-authoritative retrieval | Search attributed local files without PostgreSQL or a second truth store | Partly built | `recall` calls the substrate `recall/vault_recall` (`adapters/omp/house-proof/recall.ts:150`). Running without PostgreSQL is not re-verified at `a6ab453`; deciding crate: `crates/vault`. |
| Hallway | Let private rooms share messages and state without merging identities | Built | Seven `hallway_*` tools (`tools.ts:1596-1776`). Bell projection (`hallway.ts:22-43`). Knock doorman (`knock.ts:73-440`). Idle and headless recipient delivery is Planned. |
| OMEGA | Give organizations shared knowledge with separate company, team, and personal spirits | Planned | Of 18 OMEGA items, 4 are Built, 3 Partly built, 10 Planned, and 1 Not re-verified. No person identity exists; see [Identity](./LIMITATIONS.md#4-identity). See [OMEGA](#omega-organization-layer). |
| ANON | Use dedicated remote compute without leaving job content in the service | Planned | Not built. Every runtime address must be loopback (`crates/athanor-install/src/supervisor.rs:250-265`). The census found no attestation code. See [ANON](#anon-execution-policy). |
| Relay | Borrow remote compute while durable storage stays with the operator | Planned | Not built. Every runtime address must be loopback (`supervisor.rs:250-265`), and the adapter refuses a non-loopback Host URL (`adapters/omp/house-proof/host.ts:87-90`). See [Relay](#relay-processing-route). |
| Group rooms | Give an approved chatroom its own queryable spirit and shared memory | Not re-verified | Not re-verified at `a6ab453`. Deciding crate: not named by the census. Related limit: one Pulse process serves one room, see [Rooms and sessions](./LIMITATIONS.md#5-rooms-and-sessions). |
| Embodied rooms | Add approved voice, avatar, expression, and room packages | Not re-verified | Not re-verified at `a6ab453`. Deciding crate: not named by the census. |
| Typed signed marketplace | Distribute personality seeds, presentation packages, models, and skills with provenance, permissions, revocation, and rollback | Not re-verified | Not re-verified at `a6ab453`. Deciding crate: not named by the census. |

The full contracts for the formal backends and for companions are in [`history/2026-10-04-SYNTHESIS_ARCHITECTURE.md`](./history/2026-10-04-SYNTHESIS_ARCHITECTURE.md) and [`history/2026-10-04-COMPANION_ECOSYSTEM.md`](./history/2026-10-04-COMPANION_ECOSYSTEM.md).

## The path to 1.0

`1.0.0` adds supported ordinary-user installation around a stable Host and UI. It must keep existing Houses through installation, upgrade, backup, and recovery. Each step below starts from the code at `a6ab453`. Read [ARCHITECTURE.md](./ARCHITECTURE.md) for the full starting point.

Three facts change the older plan:

- One Host process serves every room on one port. Each room is nested under `/room/<room>` (`crates/host/src/house.rs:86-105,166-202`). See [One process, many rooms](./ARCHITECTURE.md#31-one-process-many-rooms).
- The installer ships NATS 2.14.4 (`installer/dependencies.json`). The broker carries more than `boat.ready`: the Host consumes Hallway post projections from JetStream (`crates/host/src/server.rs:2855-2894`).
- The chat ring is in memory only. A Host restart empties it (`crates/host/src/chat.rs:6-7,25-30`). See [the chat ring](./ARCHITECTURE.md#34-the-chat-ring).

### 1. Freeze the boundary

The `1.0.0` program includes Rust convergence, the NATS lane, known fixes, hardening, the usable GUI, installation, migration, and release evidence.

Do not add Prolog/Datalog, Lean, Z3, SyGuS, marketplace behavior, new cognitive organs, broker expansion, companion bodies, the GPU-particle constellation, or in-world surfaces.

### 2. Record a parity baseline and close known fixes

The `0.11.0` runtime label is history, not a target. Inventory each TypeScript, Python, and Rust capability with its owner, callers, tests, persistence effects, and failure behavior. Not re-verified at `a6ab453`: no census row covers a baseline record.

Record retrieval results, correction and room-isolation behavior, p50 and p95 latency, install steps, and restart, backup, restore, update, and rollback behavior.

Close the defects in [Known defects](./LIMITATIONS.md#8-known-defects-in-the-current-code) and [`BUGS.md`](../BUGS.md) first. A green test suite does not replace a run of the real path.

### 3. Make the Host the only client boundary

State: Planned. Today 32 tools go to the substrate child directly, and 4 tools write room files locally (`adapters/omp/house-proof/room.ts:221-222,264`). See [Tools and wires](./ARCHITECTURE.md#43-tools-and-wires).

The TypeScript adapter also holds the doormen, Presence, the boat door, the restart door, and room identity (`adapters/omp/house-proof/chat.ts`; `presence.ts`; `room.ts:86-127`).

- Move validation, policy, idempotency, and identity into Rust behind the Host.
- Keep the OMP adapter as registration, lifecycle translation, transport, and presentation.
- Define one common record envelope in `hearth`. Not re-verified at `a6ab453`: deciding crate `crates/hearth`. The wire `CommandMeta` exists (`crates/protocol/src/host.rs:789-809`).
- Prove scope and authorization before ranking with one conformance corpus for Vault and AKASHA. Not re-verified at `a6ab453`: deciding crates `crates/vault`, `crates/akasha`.

### 4. Converge Vault and AKASHA on the Rust core

State: Partly built. The build includes the Host, AKASHA, and install crates (`build-native-release.ps1:136`). TypeScript behavior remains in the adapter. Python behavior is not re-verified at `a6ab453`.

Move one complete vertical path at a time:

1. Find the current contract, owner, callers, and tests.
2. Run the Rust path through the real boundary.
3. Prove Vault and AKASHA parity where they share behavior.
4. Move every caller and remove orphans.
5. Delete the old owner.

The Vault-to-AKASHA migration is a one-way authority handoff. Not re-verified at `a6ab453`: deciding crate `crates/vault`.

### 5. Close the NATS gate

State: Partly built. The Host runs a `DeliveryService` (`crates/host/src/house.rs:235-248`) and dead-letters bad Hallway projections (`server.rs:2895-2901`).

Warning: the broker has no credentials. NATS starts with `-js -a 127.0.0.1` and no auth (`crates/athanor-install/src/supervisor.rs:417-459`). The NATS URL carries no credentials (`app.rs:17-58`).

The older gate said: no second lane before credentials and subject ACLs. The Hallway lane already exists, so the gate is broken. Fix the gate before any further lane:

- Add service credentials and subject ACLs.
- Decide whether the Hallway projection lane stays before 1.0. TODO(census): who produces `HallwayPostProjection`?
- Prove commit and publish order, durable idempotency, the duplicate window, rejection of bad pointers, restart, redelivery, and dead-letter recovery.
- Show delivery receipts in the GUI. No receipt route exists among the 15 Pulse routes (`gui-prototype/live-routes.json:2-16`).

Vault does not need NATS.

### 6. Complete the operator surface

State: Partly built. Two proxies share one contract: `serve.ts` under Bun and `pulse.exe`, a Tauri window (`gui-desktop/src/main.rs:30,65-67`). See [Pulse](./ARCHITECTURE.md#6-pulse).

Routes exist for chat, room state, Docket, Hallway, memory, lessons, Insula, and health. The 1.0 gate still needs:

- persistent chat, so a Host restart keeps the conversation;
- a session list and resume (`crates/protocol/src/restart/mod.rs:36-38`);
- more than one room per Pulse process (`serve.ts:34,41`; `proxy.rs:32`);
- GIGA review and dispatch lineage views;
- live data in place of dated fixtures (`gui-prototype/app.js:66-93`);
- login, `Origin` checks, and authenticated writes, see [Network](./LIMITATIONS.md#6-network);
- delivery, backup, migration, and version state that an operator can act on.

Every view must read Host projections. The GUI must not keep a second truth.

Companion bodies, the spatial Hallway, and the memory constellation do not block 1.0.

### 7. Make installation boring

State: Partly built. The installer provisions managed PostgreSQL (`crates/athanor-install/src/installer.rs:275-277`), the layout (`layout.rs:4-45`), rooms (`installer.rs:240-274`), and the auto-start service (`installer.rs:334-339`). It keeps at least two generations (`manifest.rs:52-55`) and waits for readiness (`installer.rs:334-340`). External PostgreSQL is an option (`installer.rs:1229-1250`).

Remaining:

- Sign the release. The release workflow has no signing step (`.github/workflows/release.yml`).
- Add a Vault install mode. The installer offers only managed or external PostgreSQL (`installer.rs:1229-1250`).
- Remove Bun. `serve.ts` and the CI OMP job still need it (`.github/workflows/ci.yml:34-50`).
- Add a room without a new release, see [Rooms and sessions](./LIMITATIONS.md#5-rooms-and-sessions).
- Prove the drain of the old generation. Not re-verified at `a6ab453`: deciding crate `crates/athanor-install`.
- Prove clean install, restart, failed replacement, update, backup, restore, and rollback.

### 8. Publish evidence and cut 1.0

State: Planned. The release workflow builds on `windows-latest` (`.github/workflows/release.yml`). A clean install proof is not re-verified at `a6ab453`.

The release evidence compares Vault, AKASHA, and the pre-cutover runtime:

- one conformance corpus against both profiles;
- exact, paraphrase, entity, date, and thread retrieval;
- correction, supersession, and room isolation;
- Vault-to-AKASHA migration with record, source, and authority checks;
- clean install, restart, replacement, backup, restore, and rollback;
- p50 and p95 latency with corpus and hardware details;
- bounded context and Recall Policy behavior;
- NATS delivery, privacy, idempotency, and recovery receipts;
- rendered GUI operation and degraded states.

AKASHA must outperform Vault where the product claims it. Private memory never becomes a public fixture. See [`EVIDENCE.md`](./EVIDENCE.md).

Before `1.0.0`, build every artifact from a clean checkout. Install both profiles on clean Windows x64 machines. Make every public document agree.

## After 1.0: the communication spine

Do these steps in order. PostgreSQL owns records, permissions, idempotency, and application receipts. NATS carries bounded pointers. The Host authenticates, applies, and projects.

| Step | Work | State |
|---|---|---|
| 1 | Add broker credentials, subject ACLs, stream readiness, and true receipt names | Planned. No NATS auth exists (`supervisor.rs:417-459`; `app.rs:17-58`). |
| 2 | Replace the memory-only Crane reference with a typed authority reference | Not re-verified at `a6ab453`. Deciding crate: `crates/origami`. |
| 3 | Add crease handlers and PostgreSQL application receipts | Not re-verified at `a6ab453`. Deciding crate: `crates/origami`. |
| 4 | Prove recipient consumers, dead-letter replay, and NATS rebuild from PostgreSQL | Not re-verified at `a6ab453`. Deciding crate: `crates/host`. |
| 5 | Replace the polled Knock with recipient-scoped NATS wake hints after steps 1-4 | Partly built, out of order. Knock polls every 2 s (`knock.ts:143`). Hallway posts already use JetStream (`server.rs:2855-2894`). |
| 6 | Add project identity, membership, and typed project records before project notifications | Planned |
| 7 | Add addressed kitten work only for a proved dormant-worker need | Planned. Kitten lineage uses the Host socket (`adapters/omp/house-proof/lineage.ts:59-113`). |
| 8 | Announce committed turns to asynchronous subscribers; keep live chat on the Host socket | Planned. Chat uses the Host socket (`adapters/omp/house-proof/chat.ts:43,126-153`). |
| 9 | Add GIGA wake hints only if several dormant workers need them | Planned |

Later threads, in order:

1. Broader GIGA refinement and more workers.
2. Origami, Pawprints, boat application, and Crane delivery through the spine only.
3. Model routing for workers, familiars, reflections, and live dialogue.
4. Prolog/Datalog derivation and Cingulate.
5. Synthesis, optional Z3, and selected Lean proofs.
6. The spatial Hallway, the memory constellation, companions, OMEGA, Relay, and ANON.

## OMEGA: organization layer

State: Planned.

OMEGA means **O**rganizational **M**emory, **E**ncryption, **G**overnance, and **A**ccess. It governs one or more Houses for an organization. OMEGA is not a storage profile. The first OMEGA release needs AKASHA.

Starting point: one Host serves one House. The Host refuses rooms that disagree on House id, token, bind, database, or NATS URL (`crates/host/src/house.rs:205-234`). No person identity exists. One bearer proves reach, not identity, see [Identity](./LIMITATIONS.md#4-identity).

Warning: the `house` scope is one shared commons today, with no per-source permissions. Do not put a company's private corpus behind it. See [Organizational boundary](./LIMITATIONS.md#14-organizational-boundary).

Authorization before retrieval and ranking is Planned. The only gate before retrieval is the shared bearer at the WebSocket upgrade (`crates/host/src/server.rs:282,432-483`). Hidden sources must not change visible scores or candidate counts.

Each OMEGA item against the code at `a6ab453`:

| Item | State | Evidence and remaining work |
|---|---|---|
| House scope | Built | One House per Host and install (`crates/host/src/config.rs:55-68`; `house.rs:205-234`). Several Houses per Host are refused. |
| Rooms and room scopes | Built | One `HostConfig` and one chat ring per room (`house.rs:184-189`). Scope string `room:{room}:recall_policy` (`config.rs:71-84`). |
| Hallways | Built | `hallway_*` tools (`adapters/omp/house-proof/tools.ts:1596-1776`). Knock claim and settle behind `knock_authority` (`server.rs:1651-1665`). |
| Private histories not merged | Built | Room level only: a chat ring per room (`house.rs:184-189`); Hallway deltas filtered by session (`server.rs:375-377`). |
| Projects | Partly built | A trusted project scope for project lessons (`tools.ts:1575`). The source of that scope is not re-verified at `a6ab453`. |
| Access policy | Partly built | Shared bearer (`server.rs:282,301-323`), equality checks (`server.rs:2551-2581`), and Knock policy per Hallway (`tools.ts:1677`). A policy engine is Planned. |
| Audit history | Partly built | Docket ledger and the Presence ledger (`presence.rs:12-14,62-69,373-419`). An organization audit is Planned. |
| Tenants | Planned | No tenant field in `HostConfig` (`config.rs:55-68`) or `runtime.json`. |
| Users | Planned | No command names a person (`server.rs:2551-2573`). One shared Host token (`gui-prototype/serve.ts:62`). |
| Roles | Planned | One bearer per House (`house.rs:205-234`). |
| Teams | Planned | No team field exists. |
| Key hierarchy | Planned | One flat `runtime-secrets.json` (`crates/athanor-install/src/installer.rs:1254-1286`). |
| Retention policy | Planned | Only fixed caps: 256 chat entries (`crates/host/src/chat.rs:16-23`), 512 receipts (`store.rs:285-320`). |
| Managed and dedicated deployment | Planned | One Windows machine, loopback only (`crates/athanor-install/src/boundaries.rs:471-475`; `supervisor.rs:250-265`). |
| Company and team spirits | Planned | One spirit per room configuration (`config.rs:87-120`). No team entity exists. |
| Personal spirits and consent | Planned | Needs person identity first. |
| Source permissions | Not re-verified | Not re-verified at `a6ab453`. Deciding crate: `crates/akasha`. |
| Authorization before retrieval and ranking | Planned | Per-source ranking is not re-verified at `a6ab453`. Deciding code: `crates/akasha` and `crates/host/src/viewport.rs`. |

### Organization spirit topology

An organization gets residents, not masks.

- A **company spirit** keeps the shared continuity of the organization.
- **Team spirits** keep the continuity of each team.
- **Personal spirits** keep one person's relationship. A personal spirit needs consent from the person and from the spirit.

A person can use a team or company spirit without a personal spirit. OMEGA shares approved organization sources. It does not merge private room histories into one identity. An archetype can seed many spirits. It never makes them one identity. One assistant must not become fifty hidden masks.

## Relay processing route

State: Planned.

Relay uses remote compute from The Athanor. The operator keeps durable storage. Relay can process one bounded request for AKASHA or GIGA and return validated output to the operator's House.

| Property | Relay | Managed processing |
|---|---|---|
| Durable House storage | Operator-controlled | Can use managed storage |
| Request retention | Transient contract | Managed service contract |
| Result destination | Operator House | Operator or managed House |
| Long-lived server state | No content state by default | Allowed under explicit retention |
| Primary use | Weak devices and phones | Complete hosted service |

Warning: Relay is not confidential computing. The remote worker sees plaintext while it processes the request.

## ANON execution policy

State: Planned.

ANON means **A**ttested **N**onpersistent **O**ne-shot **N**ode. It is a strict private policy for one bounded remote job.

An ANON worker must:

- prove its worker image through attestation;
- receive one encrypted job;
- hold no durable customer key;
- decrypt only inside isolated worker memory;
- disable content logs and persistent caches;
- encrypt the result for the client;
- erase plaintext and job state after success, failure, cancellation, or timeout.

The service keeps no job content after the job ends. ANON can protect AKASHA, GIGA, or OMEGA work.

Warning: ANON does not give network anonymity. The service can still see timing and payload size.

## Rules that stay true

- The operator controls House custody, physical resources, outer security, and constitutional grants.
- A model invocation is not an identity.
- Shared memory does not merge private selves.
- Generated pointers do not become truth without review.
- A delivery broker never becomes memory or authority.
- A solver result cannot approve or install its own candidate.
- Hidden sources do not change visible retrieval scores.
- Managed services must support complete export.
- Privacy claims name their limits.

## Release rule

Do not change the version because a document sounds finished. Change it when the named behavior runs through the release artifact, survives a restart, shows its evidence, and matches the public claim.

The dated planning record of 2026-09-06 and the older roadmap are in [`history/2026-10-04-roadmap.md`](./history/2026-10-04-roadmap.md) and [`history/2026-08-06-roadmap-snapshot.md`](./history/2026-08-06-roadmap-snapshot.md).
