# Architecture, as built

Status: current. Verified against the code at commit `a6ab453` (`dev/next`) on 2026-10-04.

This document describes what the code does today. Every claim names a file and a line range. Sixteen read-only censuses produced the evidence; they live in the census folder named at the end. Layers the census did not walk are marked **not re-verified**. Planned work lives in [`ROADMAP.md`](./ROADMAP.md). Limits live in [`LIMITATIONS.md`](./LIMITATIONS.md). Words live in [`VOCABULARY.md`](./VOCABULARY.md).

## 1. The shape

```text
 Browser ──HTTP──▶ Pulse proxy (127.0.0.1:4175) ──HTTP + Bearer──▶ Host (127.0.0.1:8787)
                   serve.ts or pulse.exe             /room/<room>/athanor/v1/...
                                                            │
                                                            ├── PostgreSQL (127.0.0.1:5432)
                                                            └── NATS JetStream (127.0.0.1:4222)
 OMP ──loads──▶ Athanor adapter ──WebSocket + Bearer──▶ Host (same process, same port)
                      │
                      └──JSONL over stdio──▶ athanor-substrate child ──▶ PostgreSQL
 athanor.exe (no arguments) starts the Host for every room, then one keeper per room.
 Each keeper starts and watches one OMP session.
 The Windows service `SolarisaelAthanor` owns only PostgreSQL and NATS.
```

Three rules hold the shape together:

1. Every listener binds loopback. The Host refuses a non-loopback bind (`crates/host/src/config.rs:92-120`). The adapter refuses a Host URL that is not `ws:` on `127.0.0.1`, `localhost`, or `::1` (`adapters/omp/house-proof/host.ts:88-90`). Both Pulse proxies bind `127.0.0.1` (`gui-prototype/serve.ts:44`; `gui-desktop/src/main.rs:63`).
2. One bearer token serves the whole House. The Host checks it once, at the WebSocket upgrade or on each HTTP door (`crates/host/src/server.rs:282,301-323`). The token proves reach, never identity.
3. PostgreSQL is authoritative for durable records. The Host keeps a few files and in-memory rings. The adapter keeps no durable state of its own.

## 2. Processes on one machine

| Process | Binary and arguments | Started by | Binds | Needs |
|---|---|---|---|---|
| Service | `athanor.exe service` | Windows SCM | nothing | `current.json`, `runtime.json` |
| PostgreSQL | `versions/<v>/runtime/postgresql/bin/postgres.exe` | the service, only when `databaseMode` is `managed` | `127.0.0.1:databasePort` | `data/postgresql` |
| NATS | `versions/<v>/runtime/nats/nats-server.exe -js` | the service, always | `127.0.0.1:natsPort` | `data/nats` |
| Host | `athanor.exe` with no arguments | the operator or `athanor start` | `127.0.0.1:hostPort` and a control door on `127.0.0.1:0` | the service running, `runtime.json`, `runtime-secrets.json`, `harnesses.json` |
| Keeper | `athanor.exe keeper --config <room>/.omp/runtime/omp-keeper.json` | the Host, for each `autoStart` harness | nothing | the keeper config, the installed substrate |
| OMP | `ompLaunch` from the keeper config | the keeper | nothing | OMP provider auth |
| Substrate child | `athanor-substrate.exe` | the adapter, on first request | nothing | `DATABASE_URL`, optional embeddings |
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

Text frames carry JSON commands. Binary frames are refused. Each command passes JSON parse, a semantic hash, `parse_client_command`, then `validate_command` (`server.rs:525-574`). `validate_command` requires the claimed `house_id`, `sender_room`, `sender_spirit`, scope, and recipient to equal the Host configuration, `visibility` to be `operator`, and `authority_class` to be `room_state` (`server.rs:2537-2583`). `sender_session` is caller-chosen and unauthenticated. `Subscribe` and `PaperBoatReceiptSubscribe` pass with a blank binding.

32 command arms exist (`server.rs:576-963`). Grouped by projection:

| Projection | Commands |
|---|---|
| `recall_policy` | Subscribe, Resync, Acknowledge, SetRequestedMode, JudgedMode, Evaluate, CompleteRefresh, FailRefresh, InvalidateAfterCompaction |
| `context` | AnalyzeContext, ApplyRecallViewport |
| `hallway` | ProjectHallwayInbox, ClaimHallwayKnock, SettleHallwayKnock |
| `akasha` | AkashaRecallQuery, AkashaLessonQuery |
| `presence` | PresenceOpen, PresenceCompile, PresenceSettle, PresenceClose |
| `routing` | RoutingStatus, RoutingDispatch, FamiliarStatus |
| `lineage` | NormalizeLineage, SettleLineage |
| `shell` | LogConversation, PlanTriggerLessons, BraidTriggerLessons |
| `chat` | ChatSubscribe, ChatSay, ChatTurn, ChatDraft |
| `paper_boat_receipt` | PaperBoatReceiptSubscribe |

Knock claim and settle pass an extra gate: `knock_authority` compares the sender with `config.spirit` (`server.rs:1649-1665`). `RoutingDispatch` and `FamiliarStatus` resolve the room directory on the server (`server.rs:2067-2093`). `LogConversation` writes transcript files under the `room_dir` the caller sends, with no check (`server.rs:2131-2235`). This is a known defect; see [`LIMITATIONS.md`](./LIMITATIONS.md).

### 3.4 The chat ring

The ring is `ChatLog { entries: VecDeque<ChatMessage>, drafts, next_sequence }` in memory, one per room (`crates/host/src/chat.rs:25-30`; `server.rs:138,218`). It keeps at most 256 entries, 8 drafts, 64 steps per line, and 32,768 characters of text or thinking per line (`chat.rs:16-23`). A Host restart empties the ring and restarts the sequence at 0.

A message has `sequence`, `author` (`Operator` or `Spirit`), `author_name`, `text`, `at`, `turn_id`, `steps`, `thinking`, and `outcome` (`crates/protocol/src/host.rs:705-723`). The Host stamps `author_name` from the room-state file: operator lines get `identity.operator`, spirit lines get `identity.spirit` (`server.rs:999-1012`; `surface.rs:80-86`; `crates/host/src/store.rs:51-75`). The caller never supplies a name. A say is `{room, text, say_id}` (`protocol/src/host.rs:743-748`).

Three callers append. The operator line comes from `chat/say` over HTTP or `ChatSay` over the socket. The spirit line comes from `ChatTurn`. `ChatDraft` replaces the live draft. `turn` retires the draft and appends one spirit line; a later draft for a settled turn changes nothing (`chat.rs:54-124`). Dedupe is by `(author, turn_id)`.

### 3.5 Presence and durable Host state

Presence sessions are keyed by the authenticated binding session (`crates/host/src/presence.rs:155,174-188`). Each keeps a frame, an authoritative ledger, an active contract, and at most 16 receipts. A replay ledger of 64 entries refuses an idempotency key reused with a different body. Presence rows reload from PostgreSQL on Host start (`server.rs:1129-1136`).

The Host writes two kinds of files. In `room_dir/.omp/runtime/athanor-house-state.json` it writes only `recallPolicy` and `lastUpdatedAt`, by atomic replace (`store.rs:77-135`). In `state_dir` it keeps `recall-policy-cursor.json`, `recall-policy-receipts.json` (at most 512), and `recall-policy-sessions.json` (`store.rs:285-441`).

Presence advertises capabilities from configuration: `RoomState` always, `Akasha` when a database or NATS URL exists, `Receipts` when enabled (`presence.rs:421-436`).

### 3.6 Projections

The Recall viewport compacts candidates to fixed ceilings: excerpt 900 characters, 5 kept candidates in automatic mode, 6 canon rows, warnings 8×300 in automatic mode (`crates/host/src/viewport.rs:89-128,278-446`). Automatic mode suppresses `zero-terms`, `glue-only`, `insufficient-evidence`, and `saturated` candidates (`viewport.rs:484-508`).

Recall policy is Host-owned. A refresh needs a non-Quiet mode and one of: explicit lookup, technical project, or pending recovery (`crates/host/src/policy.rs:220-221`). Refresh reasons rank from `post-compaction-recovery` down to `stale-working-set` at 8 turns or 4,096 tokens (`policy.rs:245-273`).

Dispatch is validation and packaging. The Host never spawns a worker: every receipt carries `executed: false` and an accepted packet is `spawnPacket { tool: "task", args }` (`crates/host/src/routing/dispatch.rs:176-177,440-456`). Four lanes exist: `smol-scout`, `smol-executor`, `tester`, `verifier` (`crates/host/src/routing.rs:100-157`). The spellbook lives at `<room_dir>/familiars/spellbook.json` or `litters.json` (`routing.rs:300-301`).

## 4. The OMP adapter

### 4.1 Load path

OMP loads `adapters/omp/index.ts`. Its default export `solarisaelHouseProof(pi, release)` sets the label `The Athanor`, registers four slash commands (`jev-shadow`, `jev-recall`, `jev-lessons`, `insula`), installs the lesson TTSR bridge, the semantic judgment shadow, and the boat door, then calls `registerSolarisaelTools` (`index.ts:941-944,1010-1056,2313`).

An installed House loads `bin/athanor-omp-loader.ts` instead. It reads `%USERPROFILE%/.omp/agent/athanor/client.json` (format 2: `houseId`, `hostToken`, `stateRoot`, `hostUrl`, `defaultRoom`, `rooms`), `current.json`, and the hash-verified component manifests. It writes `ATHANOR_STATE_DIR`, `ATHANOR_SUBSTRATE_ROOT`, `ATHANOR_SUBSTRATE_EXE`, `PG_BIN_DIR`, `ATHANOR_HOST_HOUSE_ID`, `ATHANOR_HOST_TOKEN`, and `ATHANOR_HOST_URL` into the environment, probes `/health` for the default room, warns when the Host is absent, then imports `index.ts` and `hygiene.ts` (`adapters/omp/installed-loader.ts:511,530-552,554-556,619-663`). It requires platform `windows-x64` (`installed-loader.ts:376`) and `USERPROFILE` (`installed-loader.ts:565`).

### 4.2 Hooks

The adapter registers 24 `pi.on` handlers and 2 `pi.events` handlers (`index.ts:1057-2311`):

| Hook | What it does |
|---|---|
| `session_start`, `session_switch`, `session_shutdown` | adopts the top-level session, shows House feedback, starts and stops the Knock and chat doormen |
| `before_agent_start` | holds the turn prompt, capped at 128 entries |
| `tool_call` ×4, `tool_result` ×2 | Insula spans, kitten task rooms, tool evidence marks, block-lesson refusal |
| `message_start` ×2, `message_update`, `message_end` | chat message tracking, Knock turn tracking |
| `tool_execution_start`, `tool_execution_end` | chat step drafts |
| `turn_start`, `auto_retry_start` | Insula request points |
| `context` | `composeContextAdditions`, the Presence and Recall injection inside a time budget (`index.ts:1396-2181`) |
| `session_compact` | drops the Recall working set and invalidates the Host policy |
| `agent_end` ×2 | chat settlement, conversation log, GIGA ingest, Knock turn end |
| `shutdown` | closes the transports and stops the listeners |
| `task.subagent.lifecycle`, `task` events | kitten lineage |

### 4.3 Tools and wires

43 tools are registered: 42 in `adapters/omp/house-proof/tools.ts:491-2011` plus `request_restart` in `restart-door.ts:574`.

| Wire | Count | Tools |
|---|---|---|
| Substrate child only | 31 | `canon_read`, `canon_write`, `remember`, `delete_lesson`, `update_lesson`, `wake`, `lessons`, `design_doc`, `design_doc_write`, `anamnesis`, `anamnesis_write`, 7 `giga_*`, 7 `hallway_*`, 5 `quest_*`, `request_restart` |
| Host socket only | 5 | `familiar_status`, `familiar_dispatch`, `house_dispatch`, `recall_policy`, `house_lane_status` |
| Both | 2 | `recall` (substrate result, then Host viewport; fails without the Host, `tools.ts:526-532`), `sleep` (Host presence close, then substrate boat; degrades) |
| Room files | 4 | `room_state`, `set_room_state`, `house_routing_mode`, `house_model_default` |
| In process | 1 | `kitten_lineage_status` |

The substrate wire. The adapter spawns `athanor-substrate` with `stdio: pipe`, `shell: false` (`adapters/omp/rust-transport.ts:569-575`). It finds the binary through `ATHANOR_SUBSTRATE_EXE`, or with `ATHANOR_AUTO=1` through the bundled `bin/<platform>/` folder then `PATH`; platforms are `windows-x64`, `linux-x64`, `linux-arm64` (`adapters/omp/discovery.ts:17-26,58-78`). One JSON line per request: `{protocol: 1, id, method, params}`; replies are JSON lines up to 1 MiB (`rust-transport.ts:416,460,641-666`). The default request timeout is 120 s. The transport never restarts a dead child; the caller rebuilds it on the next request (`rust-transport.ts:585-599`; `substrate.ts:22-31`). Health is a one-shot run of `athanor-substrate health --substrate-dir <ATHANOR_SUBSTRATE_ROOT> --skip-embedding` (`substrate.ts:313-321,428-513`).

The Host wire. One WebSocket per command: open, send, wait for the reply whose `correlation_id` equals the `message_id`, close (`host.ts:157-206`). Default timeout 3 s, bounded to 250–30,000 ms. Every command carries `scope: room:<room>:recall_policy` and `authority_class: room_state` (`host.ts:121-143`). The token is `ATHANOR_HOST_TOKEN`; the House id is `ATHANOR_HOST_HOUSE_ID`. Insula events and vitals use Host HTTP (`insula.ts:26,417`; `vitals.ts:8,182`).

### 4.4 The chat doorman

The doorman polls. Every 2,000 ms it sends `athanor.chat.subscribe` and reads the whole ring (`adapters/omp/house-proof/chat.ts:43,126-153,197-221`). It skips a tick when the session is not the room's top-level session, when a say is pending, when the boat door is open, or when OMP is not idle. A typed terminal turn therefore delays says.

The next say is the lowest-sequence operator line with no spirit line of the same `turnId` (`chat.ts:160-168`). The doorman injects it with `pi.sendMessage(message, { deliverAs: 'nextTurn', triggerTurn: true })` as custom type `athanor-chat-say` (`chat.ts:110-124,187`). A chat-born turn is `native: false` and carries no operator authority (`turn-origin.ts:7-29,73-93`).

At `agent_end` the doorman finds the settled assistant message and sends `athanor.chat.turn` with idempotency key `chat-turn:<sayId>` (`chat.ts:226-316`). Live drafts go out as `athanor.chat.draft`, throttled to 250 ms for text and immediate for tool steps (`chat.ts:333-380`). The doorman needs `ctx.setInterval`, `ctx.isIdle`, and `pi.sendMessage`. It needs no terminal.

### 4.5 Presence, boats, and restart

Presence opens one frame per session with `athanor.presence.open`, binding `{room, spirit, operator, session}` (`presence.ts:72-81,117-170`). Each turn it sends `athanor.presence.compile` with the prompt, the recalled set, the lessons, and the directives. Materials come from `<roomDir>/presence-pulse.md`, the paper boat, the Anamnesis excerpt, lessons, and Recall, capped at 16 items (`presence-materials.ts:15-119`). Every Presence call refuses a session that is not the room's top-level session (`presence.ts:310-314`).

The boat door intercepts typed input only when `event.source === 'interactive'`; says enter through `openSurfaceDoor` (`boat-door.ts:145-194`). A threshold compaction is cancelled and the boat is requested when the session is idle; a manual `/compact` is not vetoed (`boat-door.ts:196-219`). The House lines name the operator `Sol` by default; a room's `handoff-door.md` overrides them by section (`boat-door.ts:44-67,422-445`).

The restart door arms an exit with code 87 at `agent_end` after a verified intent (`restart-door.ts:852-904`). The successor verifies and continues only when `ctx.mode === 'tui'`; otherwise it returns silently (`restart-door.ts:474-571`).

### 4.6 Room and identity

A directory is a room when it holds `.athanor-room.json`, `active_spirit.md`, or `.omp/runtime/athanor-house-state.json` (`room.ts:86-127`). The room key is the marker's `room`, else the folder name lowercased. An unrecognized working directory falls back to `<ATHANOR_VAULT_ROOT or ~/Solarisael>/default-room` (`room.ts:105-110`).

The spirit comes from `marker.trueName`, then `active_spirit.md`, then the persisted `embodiedSpirit`. The operator comes from `marker.operator`, then the persisted `operator`, else `Operator`. Each room has exactly one operator (`room.ts:77-84,112-119`). `set_room_state` writes both fields; this is self-asserted identity, not authentication (`tools.ts:818-847`; `room.ts:221-265`).

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
- check no `Origin` or `Host` header (`proxy.rs:131-157`).

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
4. A room or spirit grants authority through an explicit authenticated binding, never through a working directory, process name, model name, or prompt claim. **Not yet met:** the adapter derives room, spirit, and operator from the working directory and room files (`room.ts:99-127`), and the Host accepts any claim that matches its configuration behind the shared bearer (`server.rs:2567-2581`).
5. Generated candidates remain proposals until a review or promotion contract grants authority.
6. Model inference starts from explicit evidence. Hidden token history never becomes an undeclared source.
7. User interfaces consume Host commands and events. They do not touch PostgreSQL, NATS, model endpoints, or harness state directly.
8. Cross-room access is explicit, scoped, attributable, and denied by default. **Caveat:** `LogConversation` accepts a caller-chosen `room_dir` (`server.rs:2131-2235`).
9. Every hard gate names its obligation and the proof it requires.
10. Failure in optional cognition or transport must not rewrite truth or block a healthy local conversation.

### 8.2 Command envelope

`CommandMeta` carries `schema_version`, `message_id`, `house_id`, `sender_room`, `sender_spirit`, `sender_session`, `recipient`, `correlation_id`, `causation_id`, `reply_target`, `idempotency_key`, `scope`, `visibility`, `authority_class`, `created_at`, `expires_at`, and `max_hops` (`crates/protocol/src/host.rs:789-809`). It has no `sender_operator`. The semantic hash ignores the identifiers and timestamps (`server.rs:504-512`). Mutation commands require an idempotency key; a replay returns the existing result or a stable conflict.

Events carry `EventMeta` with `projection_id`, `sequence`, and `state_hash` (`protocol/src/host.rs:1751-1772`). A command asks one handler to attempt a transition. An event states that a transition occurred.

### 8.3 Snapshots and deltas

The Recall Policy projection sends one snapshot on `Subscribe` or `Resync` and typed deltas after it (`server.rs:584-605`; `protocol/src/host.rs:1775-1866`). Snapshot fields: `projection_id`, `schema_version`, `snapshot_id`, `version`, `sequence`, `state_hash`, `state`. Delta fields: `delta_id`, `projection_id`, `base_version`, `next_version`, `sequence`, `source_event_ids`, `mutations`, `coalesce_key`, `created_at`.

**Not built:** a bounded replay window. A client that misses a sequence asks for a fresh snapshot. Pulse chat and the chat doorman poll whole snapshots.

### 8.4 Hallway Knock timing

A Knock claim and settle use a 10 s Host timeout. The Knock doorman polls every 2 s and backs off from 5 s to 60 s while the Host is unreachable, with one degradation warning (`knock.ts:73-116,143-146,297-378`). Pending Knocks stay claimable on the board.

## 9. Not re-verified at a6ab453

The census did not walk these layers. Their documents keep their older dates:

- `crates/akasha`: Recall lanes, memory and lesson storage, GIGA, the Hallway domain, the outbox. See [`RETRIEVAL.md`](./RETRIEVAL.md), [`LESSONS.md`](./LESSONS.md), [`HIPPOCAMPUS.md`](./HIPPOCAMPUS.md).
- `crates/origami`: boats, cranes, hallways. See the crate `FEATURES.md` files.
- `crates/hearth`, `crates/vault`, `crates/summoning`, `crates/interactive-process`.
- `adapters/workspace-search` and `scripts/build-pages.mjs`.

## 10. Evidence

The sixteen census reports and the sixteen documentation verdict tables live in `C:/Projects/the-athanor-dev/census-2026-10-04/` on the reference workstation. House memory 5432 holds the map. [`BUGS.md`](../BUGS.md) carries the defects the census found.
