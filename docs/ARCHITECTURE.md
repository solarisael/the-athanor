# Architecture, as built

Status: the base census covers commit `a6ab453` on 2026-10-04.
The Host API 2 ownership sections include the isolated proofs recorded on 2026-10-06.
The API 2 source is verified but not deployed.

The earlier Windows repairs were deployed separately.
The Pulse proxy guard was installed on 2026-10-04.

This document names current ownership and the source that implements it.
Unchanged sections retain their earlier census scope.
Later boundary proofs appear in [Evidence](./EVIDENCE.md).
Planned work appears in [Roadmap](./roadmap.md).
Limits appear in [Limitations](./LIMITATIONS.md).

## 1. The shape

```text
 Browser ──HTTP──▶ Pulse proxy (127.0.0.1:4175) ──HTTP + Bearer──▶ Host (127.0.0.1:8787)
                   serve.ts or pulse.exe             /room/<room>/athanor/v1/...
                                                            │
                                                            ├── PostgreSQL (127.0.0.1:5432)
                                                            └── NATS JetStream (127.0.0.1:4222)
 OMP ──loads──▶ Athanor adapter ──WebSocket + Bearer──▶ Host (same process, same port)
 Native CLI and keeper ──▶ shared Rust domain execution ──▶ PostgreSQL
 athanor.exe (no arguments) starts the Host for every room, then one keeper per room.
 Each keeper starts and watches one OMP session.
 The Windows service `SolarisaelAthanor` owns only PostgreSQL and NATS.
```

Three rules hold the shape together:

1. Every listener binds loopback. The Host refuses a non-loopback bind (`crates/host/src/config.rs:92-120`). The adapter refuses a Host URL that is not `ws:` on `127.0.0.1`, `localhost`, or `::1` (`adapters/omp/house-proof/host.ts:88-90`). Both Pulse proxies bind `127.0.0.1` (`gui-prototype/serve.ts:44`; `gui-desktop/src/main.rs:63`).
2. One bearer token serves the whole House. The Host checks it once, at the WebSocket upgrade or on each HTTP door (`crates/host/src/server.rs:282,301-323`). The token proves reach, never identity.
3. PostgreSQL owns durable AKASHA records. The Host owns shared room-state writes and derived context. OMP supplies observations and performs harness actions.

## 2. Processes on one machine

| Process | Binary and arguments | Started by | Binds | Needs |
|---|---|---|---|---|
| Service | `athanor.exe service` | Windows SCM | nothing | `current.json`, `runtime.json` |
| PostgreSQL | `versions/<v>/runtime/postgresql/bin/postgres.exe` | the service, only when `databaseMode` is `managed` | `127.0.0.1:databasePort` | `data/postgresql` |
| NATS | `versions/<v>/runtime/nats/nats-server.exe -js -c <protected-config>` | the service, always | `127.0.0.1:natsPort` | `data/nats`, `secrets/nats-server.conf` |
| Host | `athanor.exe` with no arguments | the operator or `athanor start` | `127.0.0.1:hostPort` and a control door on `127.0.0.1:0` | the service running, `runtime.json`, `runtime-secrets.json`, `harnesses.json` |
| Keeper | `athanor.exe keeper --config <room>/.omp/runtime/omp-keeper.json` | the Host, for each `autoStart` harness | nothing | the keeper config, the installed substrate |
| OMP | `ompLaunch` from the keeper config | the keeper | nothing | OMP provider auth |
| Native administration | `athanor-substrate.exe` | operator or keeper operations | nothing | Explicit native configuration |
| Pulse proxy | `pulse.exe` or `bun gui-prototype/serve.ts` | the operator | `127.0.0.1:4175` | `runtime.json`, `runtime-secrets.json` |

Start order. The service starts PostgreSQL first when it is managed, then NATS. It waits up to 90 seconds for each port and reports progress every 5 seconds (`crates/athanor-install/src/supervisor.rs:30-33,334-382,417-459`). The Host calls `service::ensure_running` before it binds (`crates/athanor-install/src/app.rs:88`). On a non-Windows system that call fails (`crates/athanor-install/src/service.rs:319-322`). After the Host binds, it starts each `autoStart` harness in file order and collects failures without stopping (`app.rs:104-113`). When the Host exits, it stops only the harnesses it started (`app.rs:131-133`).

The service runs with no logged-in user. The Host, the keepers, OMP, and Pulse are user processes.

## 3. The Host

### 3.1 One process, many rooms

`house::start(Vec<HostConfig>)` runs one thread, one Tokio runtime, and one listener. Each room becomes its own `server::Host`, nested under `/room/<room>` in one axum router (`crates/host/src/house.rs:86-105,166-202`). Every room must share the bind address, the bearer token, the House id, `DATABASE_URL`, and the NATS URL (`house.rs:205-234`). The spirit and the session may differ per room.

The production builder makes one `HostConfig` per `runtime.json` `rooms[]` entry (`app.rs:30-58`):

| Field | Source |
|---|---|
| `bind` | `127.0.0.1` plus `runtime.json` `hostPort` |
| `bearer_token` | `runtime-secrets.json` `hostToken` |
| `room`, `spirit` | `rooms[].room`, `rooms[].spirit` |
| `room_dir` | `roomsRoot/<room>` |
| `state_dir` | `data/state/host/<room>` |
| `house_id` | `runtime.json` `houseId` |
| `session` | `app:<room>` |
| `database_url` | the secrets file, always set |
| `nats_url` | `nats://natsHost:natsPort`, always set |
| `nats_auth` | The persistent Host service identity in `runtime-secrets.json`; credentials never enter the URL |
| `knock_autonomy` | environment variable `ATHANOR_HOST_KNOCK_AUTONOMY`: `claim` (default) or `off` (`config.rs:4,21-41`) |

The Host owns runtime resources per House: a lazy PostgreSQL pool of 8 connections, the Insula emitter, a NATS `DeliveryService` task, and a cancellation token (`house.rs:19,171-181,235-248`).

### 3.2 Doors

HTTP doors, all `POST` unless noted, all under `/room/<room>`:

| Door | Owner | Auth | Notes |
|---|---|---|---|
| `GET /health` | `server.rs:252,261-275` | **none** | status, schema version, WebSocket path, cursor, delivery health, Insula health |
| `/athanor/v1/ws` (upgrade) | `server.rs:253,277-299` | bearer at upgrade | the command socket, see 3.3 |
| `/athanor/v1/chat/snapshot`, `chat/say`, `room/state` | `crates/host/src/surface.rs:13-15,29-37` | bearer | the Pulse surface |
| `/athanor/v1/docket/board`, `docket/evidence`, `hallway/inbox`, `hallway/messages`, `memory/timeline`, `memory/read`, `lesson/timeline` | `crates/host/src/panel.rs:37-43,109-150` | bearer | read only; identity comes from `HostConfig`, never from the request (`panel.rs:247-325`) |
| `/athanor/v1/insula/events`, `vitals`, `trace`, `retention`, `spans`, `unverified-exit` | `crates/host/src/insula.rs:27-38,272-284` | bearer | the only database write is `ingest_batch` into `insula.log` and `insula.vitals_minute` |

Panel and surface doors refuse a query string, hold 4 concurrent operations, and cap the body at 64 KiB (`panel.rs:45-46,132-141,199-212`). Insula doors cap the body at 512 KiB and the batch at 128 events (`insula.rs:41-55`).

### 3.3 The command socket

Text frames carry JSON commands. Binary frames receive a refusal.
The parser validates the envelope before execution.
The Host checks the House, room, current embodied spirit, scope, and recipient.
The shared bearer does not establish person-level identity.
`sender_session` remains a caller-supplied identifier.
The implementation is `crates/host/src/server.rs`, including `validate_command` and `execute_command`.

The command families use these projections:

| Projection | Commands |
|---|---|
| `recall_policy` | Subscribe, Resync, Acknowledge, SetRequestedMode, JudgedMode, Evaluate, CompleteRefresh, FailRefresh, InvalidateAfterCompaction |
| `context` | AnalyzeContext, ApplyRecallViewport, ContextLessonPlan, PrepareContext |
| `hallway` | ProjectHallwayInbox, ClaimHallwayKnock, SettleHallwayKnock |
| `akasha` | AkashaRecallQuery, AkashaLessonQuery |
| `presence` | PresenceOpen, PresenceCompile, PresenceSettle, PresenceClose |
| `routing` | RoutingStatus, RoutingDispatch, FamiliarStatus |
| `lineage` | NormalizeLineage, SettleLineage |
| `shell` | LogConversation, PlanTriggerLessons, BraidTriggerLessons |
| `chat` | ChatSubscribe, ChatSay, ChatTurn, ChatDraft |
| `paper_boat_receipt` | PaperBoatReceiptSubscribe |
| `room` | RoomState |
| `organ` | OrganCall, with a closed operation enum |
| `judgment` | JudgmentRun |
| `lifecycle` | LifecyclePlan |

Knock claim and settlement check the current room identity.
Routing, familiar lookup, and conversation logging resolve filesystem access against the configured room.
Conversation logging refuses a foreign directory before writing (`server.rs`, `resolve_room_dir` and `log_conversation`).

### 3.4 The chat ring

Each room has one bounded `ChatLog`. It retains at most 256 messages and eight drafts.
Atomic `chat.json` and `chat-drafts.json` checkpoints live under the configured Host state directory.
Draft generations preserve settlement ordering without rewriting the history ring during streaming updates.
Write failures are refused and rolled back in memory. A corrupt checkpoint refuses room startup.
Real Windows process restarts preserved messages, drafts, sequence, and retry identity (`crates/host/src/chat.rs`).

A message has `sequence`, `author` (`Operator` or `Spirit`), `author_name`, `text`, `at`, `turn_id`, `steps`, `thinking`, and `outcome` (`crates/protocol/src/host.rs:705-723`). The Host stamps `author_name` from the room-state file: operator lines get `identity.operator`, spirit lines get `identity.spirit` (`server.rs:999-1012`; `surface.rs:80-86`; `crates/host/src/store.rs:51-75`). The caller never supplies a name. A say is `{room, text, say_id}` (`protocol/src/host.rs:743-748`).

Three callers append. The operator line comes from `chat/say` over HTTP or `ChatSay` over the socket. The spirit line comes from `ChatTurn`. `ChatDraft` replaces the live draft. `turn` retires the draft and appends one spirit line; a later draft for a settled turn changes nothing (`chat.rs:54-124`). Dedupe is by `(author, turn_id)`.

### 3.5 Presence and durable Host state

Presence sessions are keyed by the authenticated binding session (`crates/host/src/presence.rs:155,174-188`). Each keeps a frame, an authoritative ledger, an active contract, and at most 16 receipts. A replay ledger of 64 entries refuses an idempotency key reused with a different body. Presence rows reload from PostgreSQL on Host start (`server.rs:1129-1136`).

The Host serializes room mutations and Recall policy writes under the same room mutex.
It preserves unknown state fields and the manual spirit body.
Policy checkpoints remain under the configured Host state directory.
Prepared turns use `context-turns/<digest>.json`.
Native code also owns the optional room-local Recall export.
Sources: `crates/host/src/room_state.rs`, `store.rs`, and `server/context_session/`.

Presence advertises capabilities from configuration: `RoomState` always, `Akasha` when a database or NATS URL exists, `Receipts` when enabled (`presence.rs:421-436`).

### 3.6 Projections

The Recall viewport compacts candidates to fixed ceilings: excerpt 900 characters, 5 kept candidates in automatic mode, 6 canon rows, warnings 8×300 in automatic mode (`crates/host/src/viewport.rs:89-128,278-446`). Automatic mode suppresses `zero-terms`, `glue-only`, `insufficient-evidence`, and `saturated` candidates (`viewport.rs:484-508`).

Recall policy is Host-owned. A refresh needs a non-Quiet mode and one of: explicit lookup, technical project, or pending recovery (`crates/host/src/policy.rs:220-221`). Refresh reasons rank from `post-compaction-recovery` down to `stale-working-set` at 8 turns or 4,096 tokens (`policy.rs:245-273`).

Dispatch is validation and packaging. The Host never spawns a worker: every receipt carries `executed: false` and an accepted packet is `spawnPacket { tool: "task", args }` (`crates/host/src/routing/dispatch.rs:176-177,440-456`). Four lanes exist: `smol-scout`, `smol-executor`, `tester`, `verifier` (`crates/host/src/routing.rs:100-157`). The spellbook lives at `<room_dir>/familiars/spellbook.json` or `litters.json` (`routing.rs:300-301`).

## 4. The OMP adapter

### 4.1 Loading and compatibility

OMP loads `adapters/omp/index.ts` in a source checkout.
An installed House loads the hash-verified component through `bin/athanor-omp-loader.ts`.
The native installer registers that loader in OMP's extension list.

Adapter `0.10.0` requires Host API `2`.
The envelope schema remains `1`, and the database schema remains `32`.
The loader checks component compatibility and the running Host's `hostApi`.
It refuses an incompatible running Host before loading the component.

Sources: `adapters/omp/installed-loader.ts`, `installer/omp-adapter-component.ps1`, and `crates/athanor-install/src/omp.rs`.

### 4.2 Shared behavior and harness behavior

The Host owns room mutations, lesson selection, judgments, context preparation, memo persistence, and shared lifecycle decisions.
OMP owns registration, observed turn facts, context insertion, TTSR installation, model selection, and presentation.
OMP also performs message injection, interruption, compaction, handoff, and process exit.

The public tool names remain unchanged.
House tools use Host commands; `kitten_lineage_status` remains a local observation.
The adapter no longer starts substrate children.
The native CLI and Host share execution code in `crates/akasha/src/native/`.
Administrative migrations remain outside the Host organ operation list.

Sources: `adapters/omp/index.ts`, `house-proof/tools.ts`, `house-proof/organ.ts`, and `crates/host/src/organ/`.

### 4.3 Context, replay, and identity

`athanor.context.lesson_plan` refreshes guards before `athanor.context.prepare`.
That ordering also applies to replay and timeout paths.
Native context preparation returns harness-neutral blocks.
The adapter anchors those blocks beside the correct OMP turn.

Replay requires the current identity, native frame, and active contract.
Pending settlement information survives adapter reconstruction.
Acknowledged contracts do not become pending again.
Identity changes invalidate derived blocks and retire the old active contract.
Conversation history remains unchanged.

`agentName` stays fixed when `embodiedSpirit` changes.
Worker, chat, Knock, and restart text cannot apply implicit operator directives.
Legacy cache adoption never guesses identity from rendered text.
Invalid legacy material causes an explicit rebuild, and the legacy file remains unchanged.

Sources: `crates/host/src/server/context_session/`, `crates/host/src/presence.rs`, and `adapters/omp/house-proof/context.ts`.

### 4.4 Judgments and lifecycle

Native code owns Recall reranking, lesson sieving, mode proposals, and turn verdicts.
The adapter supplies a transient credential only when an eligible native plan requires one.
Native privacy checks precede provider calls.
The credential never enters stored context or diagnostic receipts.

The Host selects chat work, determines answer ownership, and applies Knock deadlines.
OMP supplies normalized observations and performs the requested harness actions.
The Host applies the boat safety margin to OMP's observed compaction threshold.
Restart authorization remains native, while OMP owns the exit-code handshake.
TUI and non-TUI successors both verify and enqueue their continuation.

Sources: `crates/host/src/judgment.rs`, `lifecycle.rs`, and `adapters/omp/house-proof/`.

### 4.5 Native resources

The Host owns room-bound GIGA workers and House-level retention.
Explicit enablement preserves the operator's GIGA settings without process-wide environment mutation.
Worker replacement waits for observed exit.
Replay cannot ingest events or stop another producer.
Read-only GIGA queries do not start workers.

The Host now publishes Hallway pointers with its own broker credentials.
The generated broker policy permits that exact subject family.
Broker reload is part of deployment.

Sources: `crates/host/src/organ/giga.rs`, `crates/akasha/src/native/retention.rs`, and `crates/athanor-install/src/installer.rs`.

The complete adapter contract is in [its README](../adapters/omp/README.md).

## 5. The keeper

The keeper runs `ompLaunch[0]` with `ompLaunch[1..]` from `omp-keeper.json`; the binary is not hard-coded (`crates/omp-keeper/src/config.rs:22,128-134`). It refuses `--continue`, `-c`, `--resume`, `-r` in that list; the keeper alone decides resume or fresh (`config.rs:70-75`). The first spawn is always fresh. A relaunch with a `Resume` intent appends `--resume <session_id>` (`crates/omp-keeper/src/keeper.rs:188,812-823`). The working directory is `config.workspace`. The child inherits the environment, minus two restart variables that the keeper sets on relaunch (`keeper.rs:33-34,828-853`).

While OMP runs, the keeper polls `try_wait` every 200 ms and asks the House for `restart_status` every `watchIntervalSecs` (default 30). It kills OMP only when an intent is `exiting` past its deadline (`keeper.rs:863-918`). After OMP exits, exit code 87 counts as an armed restart; the keeper claims the intent, relaunches, and watches for `verified` every second (`keeper.rs:194-547`). When the House is unreachable the keeper waits forever, retrying every 2 s and logging every 60 s (`keeper.rs:202-207,653-736`).

The config fields are `ompLaunch`, `workspace`, `programRoot`, `stateRoot`, `watchIntervalSecs`, `claimant`, and exactly one of `capability` or `capabilityPath` (`config.rs:19-34,53-59`). The substrate path resolves through `programRoot/current.json` to `versions/<v>/bin/athanor-substrate` and refuses symlinks and path escapes (`crates/omp-keeper/src/resolve.rs:11-145`).

## 6. Pulse

Pulse is one static module. `gui-prototype/index.html:248` loads `app.js`, which imports every other module. `app.js` holds one connected room at a time (`app.js:1514-1528`); `say(text)` takes no room argument (`chat.js:129-148`). It has no identity field (`app.js:140-173`). `New session` is unavailable (`app.js:733-743`).

Two proxies share one contract. `gui-prototype/serve.ts` runs under Bun for development. `gui-desktop/src/proxy.rs` ships as `pulse.exe`, a Tauri window; `pulse --serve-only` serves the embedded assets with no window (`gui-desktop/src/main.rs:30,65-67`). Both:

- bind `127.0.0.1` on port 4175 by default;
- serve one room per process, from `PULSE_ROOM` or `--room`, default `kodo`;
- read `C:/ProgramData/Solarisael/Athanor/config/runtime.json` for `hostPort` and `rooms[]`, and `secrets/runtime-secrets.json` for `hostToken`, at fixed paths (`serve.ts:17-18,32-41`; `main.rs:54-56`);
- forward the 15 routes in `gui-prototype/live-routes.json` to `http://127.0.0.1:<hostPort>/room/<room>/...` and add the bearer server-side (`serve.ts:58-65`; `proxy.rs:97-102`);
- in the 2026-10-04 source repair, require the canonical loopback Host and reject foreign Origin headers before route handling.

The source repair also requires Origin for requests other than GET or HEAD.
It refuses cross-site Fetch Metadata. The separate Pulse installation passed its installed check on 2026-10-04.

`pulse.exe` also answers `POST /local/repair/status` and `/local/repair/start`, which run `athanor.exe status` or `athanor.exe start`; `service: true` raises a UAC prompt (`proxy.rs:43-68,163-232`).

The chat flow. `say` posts `{text, sayId}` to `/live/chat/say` and shows an optimistic line (`chat.js:129-148`). Then it polls `/live/chat/snapshot` every 2,000 ms, or 400 ms while a draft exists, until each operator row has a spirit row with the same `turnId` (`chat.js:69-81`). A 400 from the Host removes the line with no retry; a transport error marks it undelivered and hands the clock to `health.js`, which retries at 2, 4, 8, then 15 seconds (`chat.js:150-159`; `health.js:23,78-82`). Retry of a say is manual and reuses the same `sayId`.

The other modules read only: the Docket board and evidence, the Hallway inbox and messages, Insula vitals, retention, spans, and trace, and the memory and lesson timelines (`board/index.js`, `pulse.js`, `sediment/index.js`). `pulse.js` and `mechanics.js` still show dated fixtures until a live round answers (`pulse.js:14-54,671-673`; `mechanics.js:23-25`).

## 7. Installed layout

`P` is `%ProgramFiles%/Solarisael/Athanor`. `D` is `%ProgramData%/Solarisael/Athanor`. `ATHANOR_PROGRAM_ROOT` and `ATHANOR_DATA_ROOT` override both when set together (`crates/athanor-install/src/layout.rs:4,22-47`). The service ignores those overrides (`service.rs:234-240`).

| Path | Content |
|---|---|
| `P/bin/athanor.exe`, `P/bin/athanor-omp-loader.ts` | the one executable and the stable loader |
| `P/current.json` | `version`, `previousVersion`, `rollbackBackup` |
| `P/versions/<version>/` | `bin/`, `runtime/postgresql/`, `runtime/nats/`, `components/omp-keeper/`, `components/omp-adapter/`, `compatibility.json`, `release-manifest.json` |
| `P/components/omp-adapter/current.json`, `versions/<releaseId>/` | the adapter component pointer and payloads |
| `D/config/runtime.json` | `databaseMode`, `databaseHost`, `databasePort`, `natsHost`, `natsPort`, `hostPort`, `schemaVersion`, `houseId`, `roomsRoot`, `operatorStateRoot`, `defaultRoom`, `rooms[{room, spirit}]`, `ompConfigPath`, `clientConfigPath` |
| `D/config/harnesses.json` | `format: 1`, `harnesses[{harnessId, autoStart, label, program, arguments, workspace, console}]` (`crates/athanor-install/src/harness/config.rs:15-59`) |
| `D/secrets/runtime-secrets.json` | `hostToken`, `postgresPassword`, `externalDatabaseUrl`; restricted ACL |
| `D/data/postgresql`, `D/data/nats`, `D/state/host/<room>`, `D/backups`, `D/logs` | runtime data and Host state |
| `<roomsRoot>/<room>/.omp/runtime/athanor-house-state.json` | operator, `embodiedSpirit`, `recallPolicy`; written once if missing |
| `<roomsRoot>/<room>/.omp/runtime/omp-keeper.json` | the keeper config |
| `%USERPROFILE%/.omp/agent/athanor/client.json` | format 2 client projection with a copy of `hostToken` |
| `%USERPROFILE%/.omp/agent/config.yml` | the OMP extension registration, edited in place |

The installer never writes `harnesses.json`. Adding a room means building and installing a new release; see [`LIMITATIONS.md`](./LIMITATIONS.md).

## 8. Contracts the code keeps

These rules come from the earlier runtime architecture. The code keeps them unless a note says otherwise.

### 8.1 Invariants

1. PostgreSQL is authoritative for AKASHA records, authority state, source references, review state, outcomes, and durable delivery records.
2. A transport owns delivery progress, never truth.
3. A model body is replaceable compute, never an identity.
4. Room authority needs an authenticated binding. The Host validates current room identity behind a shared bearer. Person-level authentication remains unimplemented.
5. Generated candidates remain proposals until a review or promotion contract grants authority.
6. Model inference starts from explicit evidence. Hidden token history never becomes an undeclared source.
7. User interfaces consume Host commands and events. They do not touch PostgreSQL, NATS, model endpoints, or harness state directly.
8. Memory reads use the configured room plus House commons. The bearer remains House-wide; person-level authorization is not implemented.
9. Every hard gate names its obligation and the proof it requires.
10. Failure in optional cognition or transport must not rewrite truth or block a healthy local conversation.

### 8.2 Command envelope

`CommandMeta` carries schema, request, sender, recipient, correlation, scope, expiry, hop, and projection fields.
It has no `sender_operator`.
The semantic hash ignores identifiers and timestamps.
Replay semantics belong to each native operation.
The generic organ door does not make every write an idempotent transaction.
An unknown write outcome requires reconciliation.

Sources: `crates/protocol/src/host.rs`, `server.rs`, and `adapters/omp/house-proof/organ.ts`.

Events carry `EventMeta` with `projection_id`, `sequence`, and `state_hash` (`protocol/src/host.rs:1751-1772`). A command asks one handler to attempt a transition. An event states that a transition occurred.

### 8.3 Snapshots and deltas

The Recall Policy projection sends one snapshot on `Subscribe` or `Resync` and typed deltas after it (`server.rs:584-605`; `protocol/src/host.rs:1775-1866`). Snapshot fields: `projection_id`, `schema_version`, `snapshot_id`, `version`, `sequence`, `state_hash`, `state`. Delta fields: `delta_id`, `projection_id`, `base_version`, `next_version`, `sequence`, `source_event_ids`, `mutations`, `coalesce_key`, `created_at`.

**Not built:** a general bounded event-replay window.
A client that misses a sequence requests a fresh snapshot.
Pulse polls chat snapshots.
The OMP doorman requests the next item through native lifecycle policy.

### 8.4 Hallway Knock timing

Knock claim and settlement use a 10-second Host timeout.
OMP polls every two seconds and applies transport backoff while the Host is unavailable.
Native lifecycle policy owns the 25-second settlement deadline and 60-second start deadline.
Pending Knocks remain claimable.
Sources: `crates/host/src/lifecycle.rs` and `adapters/omp/house-proof/knock.ts`.

## 9. Not re-verified at a6ab453

The census did not walk these layers. Their documents keep their older dates:
Later boundary proofs cover selected paths through these layers.
They do not replace a complete module census.

- `crates/akasha`: Recall lanes, memory and lesson storage, GIGA, the Hallway domain, the outbox. See [`RETRIEVAL.md`](./RETRIEVAL.md), [`LESSONS.md`](./LESSONS.md), [`HIPPOCAMPUS.md`](./HIPPOCAMPUS.md).
- `crates/origami`: boats, cranes, hallways. See the crate `FEATURES.md` files.
- `crates/hearth`, `crates/vault`, `crates/summoning`, `crates/interactive-process`.
- `adapters/workspace-search` and `scripts/build-pages.mjs`.

## 10. Evidence

The sixteen census reports and the sixteen documentation verdict tables live in `C:/Projects/the-athanor-dev/census-2026-10-04/` on the reference workstation. House memory 5432 holds the map. [`BUGS.md`](../BUGS.md) carries the defects the census found.
