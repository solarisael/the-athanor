# The Athanor — OMP adapter

This directory contains the OMP component of [The Athanor](../../README.md).
Adapter version `0.10.0` requires Host API `2`.
The WebSocket envelope still uses schema version `1`.
The worker lifecycle requires database schema `34`.

The adapter connects [Oh My Pi](https://github.com/can1357/oh-my-pi) to the native Host.
It no longer starts substrate children or writes shared room state.
The native CLI remains available for administration and keeper operations.

## Documentation

| Document | Purpose |
|---|---|
| [INSTALL.md](../../INSTALL.md) | Installation, update, rollback, verification, and removal |
| [USAGE.md](../../USAGE.md) | Daily use and public tools |
| [IDENTITY_GUIDE.md](../../IDENTITY_GUIDE.md) | Room identity |
| [Architecture](../../docs/ARCHITECTURE.md) | Runtime ownership and installed layout |
| [Evidence](../../docs/EVIDENCE.md) | Exercised contracts and verification limits |

## Ownership

| Owner | Responsibilities |
|---|---|
| OMP adapter | Tool registration, workspace observation, event translation, context insertion, TTSR installation, and presentation |
| OMP adapter | Model selection, message injection, abort, compaction, handoff, and the restart exit action |
| Host | Room-state mutation, prompt directives, and the derived spirit header |
| Host | Lesson selection, Recall policy, shared judgments, Presence, and context preparation |
| Host | Durable context caches, identity invalidation, and Recall diagnostic export |
| Host | Chat selection, answer ownership, Knock deadlines, and the boat safety margin |
| Host | Native organ execution, GIGA worker lifetime, and retention lifetime |
| Native domain libraries | Storage, validation, authorization, backups, and existing transaction rules |

The Host derives room paths and sender identity from its configuration and current room state.
Organ parameters cannot replace those fields.
Explicit House targets remain available for `canon_read`, `canon_write`, and `remember`.
Docket and restart operations retain their native capability checks.

## Entry points and installed layout

`index.ts` registers the source extension.
`hygiene.ts` supplies OMP-specific guards.
The native installer registers `bin/athanor-omp-loader.ts` in OMP's extension list.
The implementation lives in `crates/athanor-install/src/omp.rs`.

The loader reads `%USERPROFILE%/.omp/agent/athanor/client.json`.
It verifies release pointers, manifests, compatibility fields, file sizes, and SHA-256 values before importing either entry point.
It refuses unsafe paths, invalid room keys, foreign platforms, missing entry points, and incompatible components.

The component pointer selects `components/omp-adapter/versions/<releaseId>`.
Each native release also carries a fallback component payload.
The native manager is the only installed writer.

The loader projects the state root, House ID, Host token, and Host URL into the environment.
An explicit environment value takes precedence.
The loader does not project substrate executables or PostgreSQL tools into OMP.

The loader checks scoped Host health and its reported `hostApi`.
An absent Host produces a warning.
A running Host with an incompatible API produces a refusal before the component loads.
The health watch continues every 30 seconds.

## Host transport

Commands use authenticated loopback WebSockets at `/room/<room>/athanor/v1/ws`.
Each command opens a socket, sends its envelope, matches the correlated reply, and closes the socket.
Insula observations and vitals use authenticated Host HTTP endpoints.

The Host URL must use `ws:` with `127.0.0.1`, `localhost`, `::1`, or `[::1]`.
Remote addresses and `wss:` remain unsupported by this adapter.

The Host exposes these additional command families:

| Command | Purpose |
|---|---|
| `athanor.room.state` | Read, patch, or apply authorized prompt directives |
| `athanor.organ.call` | Execute a closed native operation |
| `athanor.context.lesson_plan` | Refresh native lesson decisions before context preparation |
| `athanor.context.prepare` | Prepare or replay the current turn's context |
| `athanor.judgment.run` | Execute an approved native judgment |
| `athanor.lifecycle.plan` | Decide shared lifecycle policy from normalized observations |

The organ operation list is closed in `crates/protocol/src/organ.rs`.
It excludes administrative migration and arbitrary process operations.
The Host and native CLI share domain execution code.

Clearly read-only calls cancel when their client closes.
Dispatched writes retain definitive execution.
A lost write response reports `outcome_unknown` and requires reconciliation before retry.

## Context and identity

OMP sends normalized turn facts, capability facts, and observed work evidence.
The Host refreshes lesson rules before context preparation, including replay and timeout paths.
OMP installs the returned rules into its TTSR manager.

The Host stores prepared turns under `<Host state directory>/context-turns/<digest>.json`.
It keeps at most 128 resident sessions.
Visible-turn pruning removes only turns that leave the supplied context window.

Repeated requests reuse stable context bytes while the identity and native Presence contract remain valid.
Pending settlement information accompanies replay, so a resumed adapter can complete the contract.
An acknowledged contract does not become pending again.

Room identity changes invalidate derived context and retire the old active contract.
The new frame carries the same room's historical ledger without reassigning old receipts.
A missing native frame or contract also causes an explicit rebuild.
The response names the invalidation reason and retains bounded provenance.

`EMBODY` changes `embodiedSpirit`, not `agentName`.
The Host preserves the manual body below the generated spirit header.
Generated chat, Knock, restart, and worker input cannot apply implicit operator directives.

The adapter reads legacy turn files only for native adoption.
It never writes another legacy turn cache.
Unverified identity material and invalid Unicode produce an explicit rebuild.
The legacy file remains unchanged.

## Judgments and credentials

Rust owns Recall reranking, lesson sieving, mode proposals, and turn verdicts.
The existing room grants remain opt-in.
Native packet checks precede external provider calls.

OMP resolves a Typesafe credential only when a native plan requires one.
Disabled, Laya-only, child-only, and valid replay paths do not request that credential.
The key remains transient and never enters a context cache, receipt, or diagnostic export.
An approved plan can request its local credential before a later packet receives a privacy refusal.

`jev-recall` and `jev-lessons` show completed-call coverage for the native room during the Host lifetime.
`jev-shadow` remains an OMP-side observation surface.
Its production installation has no eligibility provider and performs no semantic judgment.
`insula` shows the native observations.

## Environment and room inputs

| Input | Meaning |
|---|---|
| `ATHANOR_HOST_URL` | Loopback Host address; default `ws://127.0.0.1:8787` |
| `ATHANOR_HOST_TOKEN` | Required Host bearer token |
| `ATHANOR_HOST_HOUSE_ID` | Required House identity |
| `ATHANOR_STATE_DIR` | Installed state projection for operator and native commands |
| `USERPROFILE` | Required by the installed loader |
| `ATHANOR_VAULT_ROOT` | Parent of the fallback `default-room`; default `~/Solarisael` |
| `ATHANOR_REPLAY_MODE` | Value `1` disables capture and cannot change a shared GIGA producer |
| `ATHANOR_DISABLE_AUTO_RECALL` | Value `1` disables automatic Recall |
| `ATHANOR_GIGA_ENABLED` | Value `1` requests GIGA capture |
| `ATHANOR_HIPPOCAMPUS_ENABLED` | Value `1` requests classification when GIGA is enabled |
| `ATHANOR_RECALL_TELEMETRY` | Optional override for the native Recall export |
| `ATHANOR_DISABLE_KITTEN_LINEAGE` | Controls task lineage diagnostics; it does not disable the worker lifecycle |
| `ATHANOR_ROOM_CAPABILITY` | Optional override for the room's Docket capability |
| `ATHANOR_OMP_KEEPER_CONFIG` | Optional keeper configuration path |
| `ATHANOR_RESTART_EXIT_CAPABILITY` | Optional restart-exit capability |
| `ATHANOR_RESTART_INTENT_ID`, `ATHANOR_RESTART_SUCCESSOR_PROOF` | Keeper-supplied successor evidence |

The adapter no longer loads `athanor.env` for substrate discovery.
`ATHANOR_SUBSTRATE_ROOT`, `ATHANOR_SUBSTRATE_EXE`, and `ATHANOR_AUTO` no longer select an adapter child process.
Native administrative commands keep their own configuration contracts.

Native Recall export retains `.omp/runtime/recall-turns.jsonl`.
The marker's `recallTelemetry: true` enables it when no environment override exists.
The export preserves prompt hashes, route summaries, viewports, diagnostics, and outcomes.
It does not store provider credentials.

## Lifecycle boundaries

The Host selects the next unanswered chat item and the terminal answer.
OMP observes its own messages and performs the actual injection.
The Host owns Knock deadline decisions; OMP performs delivery and interruption.
OMP supplies its compaction threshold; the Host selects the boat safety margin.

The restart door still requires a provisioned keeper and operation capability.
OMP performs the final exit with code 87 after native authorization.
Both TUI and non-TUI successors verify and enqueue their continuation.
The keeper contract is in [its README](../../crates/omp-keeper/README.md).

The Host owns GIGA workers and waits for worker exit before replacement.
Top-level sessions send explicit enablement values.
Children and replay sessions cannot stop another producer.
Read-only GIGA queries do not start workers.

The Host publishes Hallway pointers with its own broker identity.
Its broker policy therefore permits `athanor.hallway.room.>` publication.
Other subject restrictions remain in place.

## Subagent context: Ask Parent

Children use their task packet and supplied lessons.
Automatic Recall remains off, including for depth-zero children.
Ordinary children ask their spawning parent for missing historical context or intent.
The adapter supplies the actual parent's `agent://` address.

Children continue independent work or return a blocked-context report.
A parent message can resume a worker that has not retired.
Children do not invent parent addresses or use `wait` only to await a reply.

The explicit `memory-research` agent type permits manual Recall.
Its automatic Recall remains off.
A task display name does not grant that exception.
This policy is not a process sandbox.

## Worker notebooks and retirement

The adapter enrolls each ordinary `task` child before the child starts.
Each child receives its own opening wake and task notebook.
A resumed child receives its checkpoint.
An explicit successor receives the named predecessor's handoff.
Children receive no room identity, room boat, Anamnesis, Hallway Bell, or Presence.
The Host also removes old room blocks before worker context replay.

At 40% context usage, the worker receives one warning to finish its work.
At 60%, the worker can only complete its handoff.
Two final provider requests remain available.
Context usage includes cached tokens.
Compaction cannot clear the retirement state.
A retired worker cannot resume work.
OMP can keep its parked session until normal cleanup.

The `worker_notebook` tool provides bounded reads, notes, acceptance, retained-evidence reads, handoff repair, explicit succession, and whole-swarm disposal.
The parent receives the exact successor action after retirement.
The successor action returns a task packet; the parent must call `task` to start it.
An incomplete handoff requires explicit parent repair before succession.
Enrolled results no longer create automatic permanent room memories.

PostgreSQL stores AKASHA notebooks.
Vault stores its journal under `<Host state directory>/worker-lifecycle/`.
The stores enforce the same ownership and lifecycle rules.
Vault permits one journal owner and completes interrupted disposal after restart.
Disposal preserves accepted evidence and receipts.
It does not erase backups.
No automatic expiry removes worker records.

Worker enrollment needs the Host.
An unavailable Host blocks the spawn.
Use the enrolled `task` tool for worker creation.
Eval and workpool spawns receive a parent-facing refusal.
Use `task` instead.
The `/tan` path has no supported enrollment route; its unbound children cannot work.
The worker boundary is logical ownership, not a process sandbox.

## Deployment and proof

Deploy matching native and adapter releases through `substrate/deploy-local.ps1`.
Restart the broker to load the Host's new publication permission.
Restart OMP after deployment because transport and tool state remain process-cached.

Do not install this component alone onto Host API 1.
Retained API 1 release metadata remains readable for rollback.
The component and selected native release must still have equal compatibility fields.

Run the adapter suite from this directory:

```text
bun run test
```

### OMP compatibility

The adapter test dependencies use OMP `18.8.7`.
Run the real CLI check after an OMP update or a lesson bridge change:

```text
bun run test:omp-compat
```

This command uses `omp` from `PATH`.
Bun can select the executable from this repository's dependencies.
To check the workstation installation, pass its executable explicitly.
For the standard Windows Bun installation, use PowerShell:

```powershell
bun run test:omp-compat --omp "$env:USERPROFILE/.bun/bin/omp.exe"
```

The check starts one RPC process with a temporary home and the development lesson bridge.
It makes no model or Host requests.
It checks native rules, active lessons, inactive lessons, prompt rebuilds, and new sessions.
It stops its process and removes its temporary files.
A missing executable, unsupported API, failed check, or timeout returns a nonzero exit code.
The default timeout is 60 seconds. Use `--timeout-ms` to change it.

OMP `18.8.7` exposes lesson trigger events without a public per-session rule registration API.
The adapter therefore retains a narrow bridge to the native matcher.
It patches each matcher once and uses the matcher's rule list as registration truth.
New sessions clear the previous session's active filter. Shutdown removes the session lookup.
An unsupported matcher leaves native context usage available and reports unavailable lesson guards.
This check does not prove Host, database, or deployment compatibility.


Native contract tests and live proof results are recorded in [Evidence](../../docs/EVIDENCE.md).
Adapter tests do not replace real Host, PostgreSQL, broker, and OMP checks.
Installed support remains Windows x64.
This cutover does not certify another harness or platform.

The [July retrieval pilot](./evals/2026-07-22-room-retrieval-pilot.json) remains a historical exact-title calibration.
It does not prove current paraphrase precision or answer quality.

Licensed under Apache-2.0. Original project and design by Sol; see [NOTICE](../../NOTICE).
