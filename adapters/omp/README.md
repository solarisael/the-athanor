# The Athanor — OMP adapter

This directory is a component source of [The Athanor](../../README.md). It is
not a separate checkout. It has its own version and release id
(`adapters/omp/package.json`, `installer/omp-adapter-component.ps1:221-224`).

The adapter connects The Athanor to [Oh My Pi (OMP)](https://github.com/can1357/oh-my-pi).
The as-built map of the adapter is in
[`docs/ARCHITECTURE.md`](../../docs/ARCHITECTURE.md#4-the-omp-adapter). Its
known limits are in [`docs/LIMITATIONS.md`](../../docs/LIMITATIONS.md).

## Where things live

| Document | Owns |
|---|---|
| [`INSTALL.md`](../../INSTALL.md) | Install, migrate, update, roll back, verify, remove |
| [`README.md`](../../README.md) | What The Athanor is and which profile to choose |
| [`USAGE.md`](../../USAGE.md) | Daily use and the full tool table |
| [`IDENTITY_GUIDE.md`](../../IDENTITY_GUIDE.md) | Co-authoring a room identity |
| [`docs/ARCHITECTURE.md`](../../docs/ARCHITECTURE.md) | Component ownership and installed layout |

## How OMP loads the adapter

There are two entry points.

- **Source checkout.** `index.ts` is the extension. Its default export
  `solarisaelHouseProof(pi, release)` sets the label `The Athanor`. It registers
  four slash commands: `jev-shadow`, `jev-recall`, `jev-lessons`, and `insula`.
  It installs the lesson TTSR bridge, the semantic judgment shadow, and the boat
  door. Then it calls `registerSolarisaelTools(pi, release)`
  (`index.ts:941-944,1010-1056,2313`). It also exports `ADAPTER_API_VERSION = 1`
  (`index.ts:109-120`).
- **Installed House.** OMP loads the default export
  `installedAthanor(pi, options)` of `installed-loader.ts`. The installer ships
  this file as `bin/athanor-omp-loader.ts` under the program root. The loader
  reads `%USERPROFILE%/.omp/agent/athanor/client.json`. It checks
  `current.json`, the release manifest, the adapter component pointer, the
  component manifest, and every listed artifact. It writes the Host and
  substrate variables into the environment. It probes Host health, then imports
  the hash-verified `index.ts` and `hygiene.ts`. It calls
  `athanor.default(pi, {releaseId, previousReleaseId})`, then
  `hygiene.default(pi)`. Then it watches the Host every 30 seconds
  (`installed-loader.ts:511,530-552,566-599,616-663`).

`client.json` has format 2 with the keys `houseId`, `hostToken`, `stateRoot`,
`hostUrl`, `defaultRoom`, and `rooms` (`installed-loader.ts:473-508,619-626`).
The loader refuses room keys outside `[a-z0-9-]`, the key `house`, and keys
that contain `--`. `defaultRoom` must be one of `rooms`. The loader uses
`defaultRoom` only to build the health endpoint
`/room/<defaultRoom>/health` (`installed-loader.ts:442-450,480-493,641,657-660`).

The loader does not start the Host. If the Host is absent, the loader warns
and loads the adapter anyway (`installed-loader.ts:514-528,647-650`).

The loader refuses to load when one of these is true
(`installed-loader.ts:69-90,103-185,216-279,291-406,408-499,603-609,652-654`):

- a program, release, component, or profile folder is a symlink, is not a
  directory, or escapes its physical root;
- a pointer or manifest has unknown fields, a wrong format, an unsafe version or
  release id, or names the active release as the previous one;
- the native manifest is not product `the-athanor`, does not match the pointer
  version, or is not platform `windows-x64`;
- the component release id differs from its pointer or from the SHA-256 of its
  manifest;
- `hostApi`, `substrateApi`, `deliveryApi`, or `schemaVersion` differ between
  the component and the native release;
- an artifact has the wrong size or SHA-256, an unsafe or duplicate path, or the
  manifest lacks `index.ts` or `hygiene.ts`;
- the `client.json` `hostUrl` is not `ws://` to `127.0.0.1`, `::1`, `[::1]`, or
  `localhost` with path `/` and no credentials, query, or hash;
- the `client.json` `stateRoot` is not absolute;
- an entry module has no default export function.

TODO(census): which OMP config names `index.ts` or the installed loader? The
census shows only `package.json` `main: index.ts` (`package.json:13`).

## Installed layout

An installed adapter is an independent component release at
`components/omp-adapter/versions/<releaseId>`. The pointer
`components/omp-adapter/current.json` selects the release
(`installed-loader.ts:566-599`).

The native release also carries a fallback adapter payload inside
`versions/<version>/components/omp-adapter/`
(`installer/build-native-release.ps1:164-166`).

## Install, roll back, and verify

The native manager is the only installed writer. To build and install an
adapter component, follow [`INSTALL.md`](../../INSTALL.md). To roll back the
adapter without a change to the native product, follow
[`INSTALL.md`](../../INSTALL.md). To check an installed tree with
`athanor.exe doctor`, follow [`INSTALL.md`](../../INSTALL.md).

## Environment variables

The adapter reads these variables
(`adapters/omp/index.ts`, `athanor-root.ts`, `discovery.ts`,
`installed-loader.ts`; host and substrate meanings from
`house-proof/host.ts` and `house-proof/substrate.ts`):

| Name | Default | Meaning |
|---|---|---|
| `ATHANOR_REPLAY_MODE` | Unset | Any value other than `1` keeps capture on (`index.ts:1505,2302`). |
| `ATHANOR_DISABLE_AUTO_RECALL` | Unset | Automatic recall stays on unless the value is `1` (`index.ts:1784`). |
| `ATHANOR_ENV_FILE` | Installed: `<programRoot>/state/athanor.env`. Source checkout: `<repo>/../state/athanor.env` | Not a variable you set. The directory layout fixes the location (`athanor-root.ts:34-44`). The file loads when `athanor-root.ts` loads. A missing or unreadable file is silent. |
| `ATHANOR_STATE_DIR` | Empty | State root. Health passes `--env-file <ATHANOR_STATE_DIR>/substrate/.env` (`substrate.ts:313-321`). `athanor.env` and the installed loader fill it only when it is empty. |
| `ATHANOR_SUBSTRATE_ROOT` | Empty | Substrate directory for the health probe. It must be an absolute directory that exists; otherwise health reports `degraded` (`substrate.ts:136-145`). `athanor.env` and the installed loader fill it only when it is empty. |
| `ATHANOR_SUBSTRATE_EXE` | Unset | Path of `athanor-substrate`. When set, it must point to a regular file, or loading throws (`discovery.ts:28-38,60-61`). Health needs an absolute path (`substrate.ts:156-159`). The installed loader fills `<nativeRoot>/bin/athanor-substrate.exe`. |
| `ATHANOR_AUTO` | Unset: no discovery | When `1`, discovery tries `<adapter root>/bin/<platform>/athanor-substrate[.exe]`, then `PATH` (`discovery.ts:62-77`). `athanor.env` can fill it. |
| `ATHANOR_ROOT / INSTALLED_COMPONENT` | No variable | The root comes from `import.meta.url`. The adapter counts as installed when its parent folders are `omp-adapter` then `components`; on win32 the check ignores case (`athanor-root.ts:14-32`). |
| `PATH` | Empty string | Search path for `ATHANOR_AUTO=1`. On win32 each name is also tried with `.exe` (`discovery.ts:41-48`). |
| `USERPROFILE` | Required by the installed loader | Locates `client.json`. If it is missing, the loader throws `no USERPROFILE` (`installed-loader.ts:565`). |
| `PG_BIN_DIR` | Empty | The installed loader fills `<nativeRoot>/runtime/postgresql/bin` (`installed-loader.ts:628-634`). |
| `ATHANOR_HOST_HOUSE_ID` | Required for Host commands | House id in every Host command (`host.ts:124`). The installed loader fills it from `client.json`. |
| `ATHANOR_HOST_TOKEN` | Required for Host commands | Bearer token in the `Authorization` header (`host.ts:153,182`). The installed loader fills it from `client.json`. |
| `ATHANOR_HOST_URL` | `ws://127.0.0.1:8787` | Host address. It must be loopback; see [The two wires](#the-two-wires). The installed loader fills it from `client.json` only when it is empty (`installed-loader.ts:634`). |

`athanor.env` accepts only `ATHANOR_STATE_DIR`, `ATHANOR_SUBSTRATE_ROOT`,
`ATHANOR_SUBSTRATE_EXE`, and `ATHANOR_AUTO`. It ignores unknown keys and
`SOLARISAEL_*` keys (`athanor-root.ts:51-56,90-105,113-128`).

The installed loader writes its variables before it imports `index.ts`
(`installed-loader.ts:646-651`). So `athanor.env` cannot override
`ATHANOR_SUBSTRATE_EXE` on an installed House.

Some organs read more variables:

| Name | Default | Meaning |
|---|---|---|
| `ATHANOR_NATS_URL` | Filled from `C:/ProgramData/Solarisael/Athanor/config/runtime.json` `natsHost` and `natsPort` | Passed to the substrate child. If the file cannot be read, nothing is filled (`rust-transport.ts:57-68,564-567`). |
| `ATHANOR_VAULT_ROOT` | `~/Solarisael` | Parent of `default-room`, the fallback room (`house-proof/constants.ts:7-9`; `room.ts:108-110`). |
| `ATHANOR_GIGA_ENABLED` | Unset | GIGA tools refuse with `giga_disabled` unless the value is `1` (`giga.ts:107-108`). |
| `ATHANOR_DISABLE_KITTEN_LINEAGE` | Unset | Read by `kitten_lineage_status` (`tools.ts:1155`). |
| `ATHANOR_ROOM_CAPABILITY` | `<room>/.omp/runtime/room-capability` | Docket capability for `quest_post`, `quest_claim`, and `quest_report` (`room.ts:147-155`). |
| `ATHANOR_OMP_KEEPER_CONFIG` | `<room>/.omp/runtime/omp-keeper.json` | Keeper config that `request_restart` reads (`restart-door.ts:195-213`). |
| `ATHANOR_RESTART_EXIT_CAPABILITY` | `<room>/.omp/runtime/restart-exit-capability` | Restart-exit capability (`restart-door.ts:49-71`). |
| `ATHANOR_RESTART_INTENT_ID`, `ATHANOR_RESTART_SUCCESSOR_PROOF` | Set by the keeper on relaunch (`crates/omp-keeper/src/keeper.rs:828-853`) | The successor session uses them to verify a restart (`restart-door.ts:474-524`). |
| `ATHANOR_TEST_SUBSTRATE_HEALTH_SCRIPT` | Unset | Test hook. It replaces the health command with `process.execPath <script>` (`substrate.ts:316-318`). |

## Files the adapter reads and writes

| Path | Use |
|---|---|
| `<USERPROFILE>/.omp/agent/athanor/client.json` | Installed loader: required (`installed-loader.ts:619-626`). |
| `<programRoot>/current.json`, `versions/<v>/release-manifest.json`, `components/omp-adapter/current.json`, `components/omp-adapter/versions/<releaseId>/component-manifest.json`, and every listed artifact | Installed loader: all required. `programRoot` is `<loader dir>/..` (`installed-loader.ts:561-599,616`). |
| `<room>/.athanor-room.json` | Room marker: read (`room.ts:86-127`). |
| `<room>/.omp/runtime/athanor-house-state.json` | Room state: read, and written by `saveRoomState` (`room.ts:130,201,221-222`). |
| `<room>/active_spirit.md` | Spirit name: read, and rewritten by `writeActiveSpiritSnapshot` (`room.ts:226-265`). |
| `<room>/.omp/runtime/turn-additions/<hash>.json` | Turn additions: read, and written through a `.<pid>.tmp` file and a rename (`index.ts:687-695,709-758`). |
| `.solarisael-room.json`, `solarisael-house-state.json` | Legacy names: renamed in place (`room.ts:20-28`). |

Every hook resolves its room with `roomContext(ctx.cwd)`. `agent_end` falls back
to `process.cwd()` when `ctx.cwd` is empty (`index.ts:2280`). A directory that
is not a room falls back to `<ATHANOR_VAULT_ROOT>/default-room`
(`room.ts:105-110`).

## Hooks

`index.ts` makes 24 `pi.on` registrations and 2 `pi.events.on` registrations
(`index.ts:1057-2311`). It registers no handler for `input` or
`before_provider_request`. The census did not read hooks inside
`installBoatDoor`, `installLessonTtsrBridge`, `installSemanticJudgmentShadow`,
or `registerSolarisaelTools`.

| # | Hook | Handler | Source |
|---|---|---|---|
| 1 | `session_start` | `showReadyFeedback`: adopts the top-level session, shows House feedback, starts the Hallway Knock and chat doormen | `index.ts:1057-1071` |
| 2 | `session_switch` | Clears the active prompt, retires stale Insula sessions, registers the top-level session, calls `showReadyFeedback` | `index.ts:1072-1079` |
| 3 | `session_shutdown` | Clears the prompt, retires the Insula session and the top-level fence, stops the Knock and chat doormen | `index.ts:1080-1088` |
| 4 | `before_agent_start` | Holds the turn prompt in `activeTurnPrompts`, capped at 128 entries | `index.ts:1097-1112` |
| 5 | `tool_call` | Opens an Insula tool span | `index.ts:1117-1139` |
| 6 | `tool_result` | Closes the Insula tool span and records a result point | `index.ts:1141-1170` |
| 7 | `message_start` | `noteChatMessageStart` | `index.ts:1176-1178` |
| 8 | `message_update` | `noteChatMessageUpdate` | `index.ts:1179-1181` |
| 9 | `message_end` | `noteChatMessageEnd` | `index.ts:1182-1184` |
| 10 | `tool_execution_start` | `noteChatToolStart` | `index.ts:1185-1187` |
| 11 | `tool_execution_end` | `noteChatToolEnd` | `index.ts:1188-1190` |
| 12 | `turn_start` | `openInsulaRequest(…, 'provider_replaced')` | `index.ts:1195-1202` |
| 13 | `auto_retry_start` | `openInsulaRequest(…, 'provider_retried')` | `index.ts:1204-1211` |
| 14 | `turn_end` | Settles the Insula request, then the presence contract; refuses a turn with no assistant text | `index.ts:1213-1268` |
| 15 | `agent_end` | Fallback settlement of the Insula request | `index.ts:1270-1290` |
| 16 | `pi.events` `task:subagent:progress` | `stampAttemptId`, `noteKittenProgress` | `index.ts:1293-1306` |
| 17 | `pi.events` `task:subagent:lifecycle` | `noteKittenLifecycle`, `settleQuestLifecycle`, `recordKittenQuest` | `index.ts:1308-1345` |
| 18 | `tool_call` (task only) | `cacheKittenTaskRoom` | `index.ts:1347-1356` |
| 19 | `tool_call` (mutating tools) | `markToolEvidence` | `index.ts:1358-1369` |
| 20 | `tool_call` | `blockLessonRefusal`; on error it notifies and returns `undefined` | `index.ts:1370-1381` |
| 21 | `message_start` (`athanor-hallway-knock`) | `noteHallwayKnockTurnStart` | `index.ts:1382-1392` |
| 22 | `context` | `composeContextAdditions` inside `settleAutomaticContextWithinBudget` | `index.ts:2183-2212` |
| 23 | `session_compact` | Drops recall context from the memo, calls `RecallPolicyHostClient.invalidateAfterCompaction` | `index.ts:2215-2236` |
| 24 | `tool_result` (task only) | `normalizeQuestMemories`, `recordKittenQuest` | `index.ts:2237-2261` |
| 25 | `shutdown` | Closes the recall, remember, paper-boat, Anamnesis, and GIGA transports and Insula; stops the kitten listeners | `index.ts:2264-2277` |
| 26 | `agent_end` | `noteChatTurnEnd`, `logConversationWindow`, `ingestGigaLoggedTurnsDetached`, `noteHallwayKnockTurnEnd` | `index.ts:2279-2311` |

## Tools by wire

The adapter registers 43 tools: 42 in `house-proof/tools.ts:491-2011` and
`request_restart` in `house-proof/restart-door.ts:574`. The full tool table is
in [`USAGE.md`](../../USAGE.md).

| Wire | Count | Tools |
|---|---|---|
| Substrate child only | 31 | `canon_read`, `canon_write`, `remember`, `delete_lesson`, `update_lesson`, `wake`, `lessons`, `design_doc`, `design_doc_write`, `anamnesis`, `anamnesis_write`, 7 `giga_*`, 7 `hallway_*`, 5 `quest_*`, `request_restart` |
| Host socket only | 5 | `house_lane_status`, `familiar_status`, `familiar_dispatch`, `house_dispatch`, `recall_policy` |
| Both | 2 | `recall` (fails without the Host, `tools.ts:526-532`), `sleep` (degrades without the Host; the boat is still written) |
| Room files | 4 | `room_state`, `set_room_state`, `house_routing_mode`, `house_model_default` |
| In process | 1 | `kitten_lineage_status` |

## The two wires

**Substrate child.** The adapter spawns `athanor-substrate` locally with
`stdio: pipe` and `shell: false` (`rust-transport.ts:569-575`). The child
starts on the first request. It inherits the OMP environment and working
directory unless the caller passes others (`rust-transport.ts:564-570`). Each
request is one JSON line `{protocol: 1, id, method, params}`. Each reply is one
JSON line of at most 1 MiB (`rust-transport.ts:416,460,641-666`). The default
request timeout is 120 s. The transport never restarts a dead child. The caller
builds a new transport on the next request (`rust-transport.ts:585-599`;
`substrate.ts:22-31`). Health runs the binary once:
`athanor-substrate health --substrate-dir <ATHANOR_SUBSTRATE_ROOT>
--skip-embedding`. It accepts only `ok: true`, `mode: "full"`, and
`substrateApi: 1`; otherwise it reports `degraded` (`substrate.ts:313-321,381-389`).

**Host socket.** Each Host command opens one WebSocket to
`ws://<host>/room/<room>/athanor/v1/ws`, sends once, waits for the reply whose
`correlation_id` equals the `message_id`, and closes (`host.ts:82-97,157-206`).
The default timeout is 3 s, bounded to 250–30,000 ms (`host.ts:8,150,154-156`).
Insula events and vitals use Host HTTP at `/athanor/v1/insula/events` and
`/athanor/v1/insula/vitals` (`insula.ts:26,417`; `vitals.ts:8,182`).

**Loopback rule.** The Host URL must use scheme `ws:` and hostname `127.0.0.1`,
`localhost`, `::1`, or `[::1]`. The adapter refuses every other URL, including
`wss:` and LAN addresses (`host.ts:74,87-90`). Each OMP process must run on the
same machine as its Host.

The adapter fails open for conversation continuity. An unhealthy AKASHA
dependency reports `degraded`. The adapter never reports a healthy state that
it did not receive.

## The chat doorman

The doorman polls the Host every 2,000 ms with `athanor.chat.subscribe` and
reads the whole chat ring (`house-proof/chat.ts:43,126-153,197-221`). It skips a
tick when the session is not the room's top-level session, a say is pending,
the boat door is open, or OMP is not idle (`chat.ts:140-158`). It injects the
oldest unanswered operator say with
`pi.sendMessage(..., { deliverAs: 'nextTurn', triggerTurn: true })` as custom
type `athanor-chat-say` (`chat.ts:110-124,160-168,187`). At `agent_end` it sends
the settled answer as `athanor.chat.turn` with idempotency key
`chat-turn:<sayId>` (`chat.ts:226-316`). A chat-born turn is not native and
carries no operator authority (`turn-origin.ts:7-29`).

## The restart door

`request_restart` refuses with `no_restart_owner` unless the room has a
provisioned keeper: `omp-keeper.json` and the capability it names
(`house-proof/restart-door.ts:171-214,597-790`). At `agent_end` the door
re-reads the exit capability, transitions the intent to `exiting`, and exits
with code 87 so the keeper relaunches OMP (`restart-door.ts:17-19,852-904`). The
successor verifies and continues only when `ctx.mode === 'tui'`; otherwise it
returns silently (`restart-door.ts:474-571`).

The keeper side is in [`crates/omp-keeper/README.md`](../../crates/omp-keeper/README.md).

## TypeScript and Rust

TypeScript owns harness registration, room discovery, lifecycle translation,
bounded context presentation, and the Rust transports. TypeScript also writes
room files: `.omp/runtime/athanor-house-state.json`, `active_spirit.md`, and
the turn-additions files (see
[Files the adapter reads and writes](#files-the-adapter-reads-and-writes)).
The Host owns Recall Policy state.

## Runtime modules

| File or directory | Role |
|---|---|
| `index.ts` | OMP extension entry point |
| `installed-loader.ts` | Installed entry point, shipped as `bin/athanor-omp-loader.ts` |
| `hygiene.ts` | Loaded after `index.ts` by the installed loader |
| `athanor-root.ts` | Installed or source root resolution and `athanor.env` loading |
| `discovery.ts` | Substrate executable discovery |
| `rust-transport.ts` | Bounded long-lived JSONL transport |
| `giga.ts` | OMP-side GIGA event bridge |
| `kitten-lineage.ts` | OMP task-lineage observation |
| `house-proof/` | Tool schemas, Host and substrate clients, chat, presence, room, and doors |
| `starter-room/` | Example room material |

## Platform

The installed loader requires platform `windows-x64` and `USERPROFILE`
(`installed-loader.ts:376,565`). Discovery also supports `linux-x64` and
`linux-arm64` (`discovery.ts:17-26`). See
[`docs/LIMITATIONS.md`](../../docs/LIMITATIONS.md).

## Retrieval evaluation

Not re-verified at a6ab453. The sanitized
[`2026-07-22 room retrieval pilot`](./evals/2026-07-22-room-retrieval-pilot.json)
measured exact-title lookup across ten active room-owned memories in each of two
rooms. It reports 95% combined viewport recall and 80% combined top-1 recall.

That pilot is a small calibration with favorable phrasing. It is not a
paraphrase or answer-quality benchmark. Raw prompts, memory identifiers,
excerpts, and telemetry stay private.

## Test

Run the adapter suite from this directory:

```text
bun run test
```

The script runs `bun test --max-concurrency 1 --isolate` (`package.json:15`).

Licensed under Apache-2.0. Original project and design by Sol; see
[`NOTICE`](../../NOTICE).
