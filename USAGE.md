# Using The Athanor

Installation gives an AI a persistent room. Managed rooms and mutable state live
under `%ProgramData%\Solarisael\Athanor`; immutable product versions live under
`%ProgramFiles%\Solarisael\Athanor`. Read [`INSTALL.md`](./INSTALL.md) if you
have not installed yet. Daily use is a small loop: enter the room, work or live
together, preserve what matters, and leave a handoff when the session ends.

House loads identity and compact continuity automatically. Durable memory remains deliberate by default so important trails do not disappear inside indiscriminate transcript storage.

## Start the House

Run this command in PowerShell:

```powershell
& "$env:ProgramFiles\Solarisael\Athanor\bin\athanor.exe"
```

The Athanor starts these parts in this order:

1. Start the `SolarisaelAthanor` Windows service if it is stopped.
   Wait for the configured PostgreSQL and NATS ports to answer.
2. Start the room Hosts inside `athanor.exe`.
3. Start each harness with `"autoStart": true` in `config/harnesses.json`, in file order.
   An absent `autoStart` field means false.

The database has two modes. The `databaseMode` field in `runtime.json` is
`managed` or `external` (FaroRuntime runtime.json row; installer.rs:1229-1250).
In `managed` mode, the service starts PostgreSQL before NATS (supervisor.rs:417-459).
In `external` mode, the service does not start PostgreSQL. Your PostgreSQL server
must already answer on the configured port. The service waits for that port in
both modes (service.rs:40-111).

The command prints service progress every five seconds while it waits.
It stops with a named error if the service or ports are not ready within 90 seconds.
After startup, it prints one JSON object with the process ID, Host address,
control address, registry path, room list, `harnessesStarted`, and `harnessesFailed`.
Each failed harness has an `id` and a `reason`. A harness failure does not stop the Host.
On exit, The Athanor stops only the harnesses that it started.
The OMP adapter never starts The Athanor. If the Host is absent, OMP stays usable
and reports: `Athanor Host is not running at <endpoint>. Start the Athanor.`
Six tools fail without the Host. [Tools and the Host](#tools-and-the-host) names them.

## Everyday loop

1. **Enter the room.** Start OMP from the configured room under `%ProgramData%\Solarisael\Athanor\rooms`. Identity and compact continuity load with the session.
2. **Work or live together.** Talk normally. Use recall when older evidence matters.
3. **Keep what matters.** Record durable events, decisions, preferences, corrections, and reusable lessons.
4. **Leave a paper boat.** At a meaningful stopping point, write a compact handoff for the next session.

You do not need every tool every day.

## Open the Pulse desktop app

Not re-verified at a6ab453 (the `[Icons]` entry in `installer/athanor.iss` decides):
open **The Athanor** from the Start menu to start the Host.

Run these commands from the repository root:

```powershell
bun run build:pulse
bun run install:pulse
pulse
```

Use `pulse --room <key>` to select a configured room.
The default room is `kodo`.
Installation uses the current Windows profile.
Use `scripts/install-pulse.ps1 -Profile C:\Users\Solarisael -NoDesktop` to select that profile explicitly.
The app reads the live Host and sends chat through its authenticated proxy.
Other pages remain read-only.
Use `pulse --serve-only --dev-dir gui-prototype` for local web development.
The existing `bun gui-prototype/serve.ts` command remains available.
Run **Athanor Doctor** from the Start menu when you suspect a lifecycle fault.

## Recall older evidence

Ask for recall when the House may already know something relevant:

> Recall why we rejected the original architecture.

> Search our memories for what I said about this name.

> Check the House before answering; I think we decided this already.

Use distinctive terms, dates, entities, project names, or exact phrases. Follow the returned source paths, taxonomy, and related candidates when the first query reveals a nearby trail.

Explicit `recall` is broader than automatic context. Read [`docs/RETRIEVAL.md`](./docs/RETRIEVAL.md) for retrieval lanes, authority, corrections, archival, and debugging misses.

`recall` needs the Host. Without the Host, `recall` returns `Athanor recall failed`
and discards the substrate result (tools.ts:516, 526-532).

## Remember durable events

Ask the AI to remember an event, decision, realization, or preference that a later session should not have to rediscover:

> Remember why we chose PostgreSQL.

> Remember that this name matters to me and why.

> Keep today as a memory; this changed how I understand the project.

A useful memory preserves:

- what happened;
- the relevant people, project, or room;
- why it matters;
- the observable consequence;
- enough context for future recognition.

Save the event, not every sentence around it. Never place credentials or secrets in memory.

## Correct changed truths

When a current claim changes, record the new account and supersede the old one in the same write:

> That memory has the event right but the interpretation wrong. Preserve the event, record this correction, and supersede the old interpretation.

> My preference changed. Keep the history, but make the new preference current.

Supersession removes stale authority without deleting history. Narrative memories remain part of the trail.

## Store reusable lessons

Use a typed lesson when the durable content is a rule rather than an event:

- **Coding lesson:** transferable engineering or process craft.
- **Project lesson:** a rule or constraint owned by one project.
- **Writing lesson:** prose, voice, register, or taste.
- **Design lesson:** reusable design-system taste bound to a named design system.
- **Audio lesson:** reusable audio and speech-pipeline behavior.

Examples:

> Save this as a shared coding lesson: verify the extracted archive, not only the build script.

> This is an example-project project lesson, not a global coding rule.

> Keep this as a writing lesson for my voice.

Read [`docs/LESSONS.md`](./docs/LESSONS.md) for fields, scopes, proof patterns, imports, updates, and deletion. Use the `lessons` organ for typed retrieval.

When a specific assistant behavior repeats, use `/omfg <complaint>`. This is an
OMP harness command, not an Athanor command. The adapter does not register it
(index.ts:1010-1056). Add `ttsr-approved` only after positive and negative review.

## Sleep with a paper boat

A paper boat is the compact word from this session to the next one.

Ask:

> Sleep with what happened, what remains open, and the first next step.

> Close this session, but make sure tomorrow remembers the unresolved decision.

A useful boat contains:

- what happened;
- what changed;
- what remains unresolved;
- the emotional or working register when relevant;
- the first useful next action.

A boat orients the next session. It is not a transcript.

`sleep` degrades without the Host. The presence close fails with a warning, and
the substrate still writes the boat (tools.ts:996; FaroAdapterTools sleep row).

## Wake into the latest handoff

At the beginning of a later session, ask:

> Wake up and catch the latest boat.

> What did yesterday leave for us?

`wake` returns the latest paper boat for the current room. Not re-verified at
a6ab453 (the `substrate` crate decides): boats remain room-scoped.

## Consult the Anamnesis Cabinet

The Cabinet preserves load-bearing paths through things already lived.

- A **pillar** preserves a standing place.
- An **active cycle** preserves a pattern to verify against the present.

Use:

- `anamnesis` with `mode: "wake"` for the bounded startup view;
- `anamnesis` with `mode: "consult"` and a focused query for deliberate counsel;
- `anamnesis_write` to add a drawer or append a lived repetition.

Cabinet counsel is source-cited and advisory. An active cycle is not proof that the same pattern is happening now. Detailed retrieval behavior lives in [`docs/RETRIEVAL.md`](./docs/RETRIEVAL.md).

## Check room state

Use `room_state` to read the room state file:

> Check the room state. Who are you here, and whose room is this?

`room_state` returns `{path, state}` (tools.ts:810-813). The `state` object holds
`version`, `operator`, `agentName`, `embodiedSpirit`, `ignoredSpiritDirective`,
`lastSpiritChangeAt`, `lastUpdatedAt`, `routingMode`, `modelDefault`, and `room`
(room.ts:157-176, 194-212). It has no `mode` field. `room_state` reads only this
file and never probes the substrate (FaroAdapterTools room_state row).

Use `set_room_state` for safe identity metadata such as the operator name or spirit display name. Edit identity prose together in `active_spirit.md`; metadata changes do not replace the identity contract.
The `operator` and `trueName` fields in `.athanor-room.json` override the state
file (room.ts:112, 117).

## Work across rooms

Rooms remain separate by default. Keep identity, intimacy, and room-specific memory local.

Not re-verified at a6ab453 (the `substrate` crate decides read addressing):
cross a room boundary deliberately.

> Ask Kodo's room for the memory about April 10.

> Retrieve this exact cross-room memory address.

The adapter refuses a write to a sibling room (tools.ts:636).

Shared lessons and explicit shared House scopes are not the same as private cross-room memory.

## What House handles automatically

The OMP adapter handles these parts:

- active-room discovery;
- identity and compact continuity loading;
- live session context;
- conversation logging;
- bounded relevant-context injection;
- paper-boat recovery near session start;
- native attributed Vault retrieval when AKASHA is not configured.

Use explicit recall for load-bearing old decisions, names, promises, corrections, or important memories.

## Tools and the Host

The adapter registers 43 tools: 42 in `adapters/omp/house-proof/tools.ts:491-2011`
and `request_restart` in `restart-door.ts:574`. Each tool uses one or more of
these wires (FaroAdapterTools wire legend):

- **Substrate:** a spawned `athanor-substrate` child over JSONL stdio (tools.ts:339-359).
- **Host:** the loopback WebSocket `ws://127.0.0.1:8787/room/<room>/athanor/v1/ws`
  with the Host bearer token (host.ts:146-207).
- **Room file:** local files in the room directory, through `room.ts`.
- **In-process:** the adapter process only.

| Family | Tool | Wire and Host need |
|---|---|---|
| Continuity | `wake` | Substrate. No Host. |
| Continuity | `sleep` | Host, then substrate. Degrades without the Host: the boat is still written. |
| Continuity | `remember` | Substrate. No Host. |
| Continuity | `recall` | Substrate, then Host. **Fails without the Host.** |
| Continuity | `canon_read` | Substrate. No Host. |
| Continuity | `canon_write` | Substrate. No Host. |
| Lessons | `lessons` | Substrate. No Host. |
| Lessons | `update_lesson` | Substrate. No Host. |
| Lessons | `delete_lesson` | Substrate. No Host. |
| Lessons | `design_doc` | Substrate. No Host. |
| Lessons | `design_doc_write` | Substrate. No Host. |
| Anamnesis | `anamnesis` | Substrate. No Host. |
| Anamnesis | `anamnesis_write` | Substrate. No Host. |
| Room | `room_state` | Room file (read). No Host. |
| Room | `set_room_state` | Room files (write). No Host. |
| Room | `house_routing_mode` | Room file (read and write). No Host. |
| Room | `house_model_default` | Room file and the OMP model setting. No Host. |
| Hallway | `hallway_create` | Substrate. No Host. |
| Hallway | `hallway_join` | Substrate. No Host. |
| Hallway | `hallway_post` | Substrate. No Host. |
| Hallway | `hallway_knock_policy` | Substrate. No Host. |
| Hallway | `hallway_knock` | Substrate. No Host to record the Knock. Delivery needs the Host. |
| Hallway | `hallway_read` | Substrate. No Host. |
| Hallway | `hallway_inbox` | Substrate. No Host. |
| Docket | `quest_post` | Substrate. No Host. |
| Docket | `quest_board` | Substrate. No Host. |
| Docket | `quest_claim` | Substrate. No Host. |
| Docket | `quest_report` | Substrate. No Host. |
| Docket | `quest_evidence` | Substrate. No Host. |
| GIGA | `giga_candidate_list` | Substrate (GIGA child). No Host. |
| GIGA | `giga_health` | Substrate (GIGA child). No Host. |
| GIGA | `giga_queue_maintenance` | Substrate (GIGA child). No Host. |
| GIGA | `giga_review` | Substrate (GIGA child). No Host. |
| GIGA | `giga_promote_memory` | Substrate (GIGA child). No Host. |
| GIGA | `giga_promote_coding_lesson` | Substrate (GIGA child). No Host. |
| GIGA | `giga_promote_project_lesson` | Substrate (GIGA child). No Host. |
| Dispatch | `house_lane_status` | Host, then a substrate health probe. **Fails without the Host.** |
| Dispatch | `house_dispatch` | Host. **Fails without the Host.** |
| Dispatch | `familiar_status` | Host. **Fails without the Host.** |
| Dispatch | `familiar_dispatch` | Host. **Fails without the Host.** |
| Dispatch | `kitten_lineage_status` | In-process. No Host. |
| Recall policy | `recall_policy` | Host. **Fails without the Host.** |
| Restart | `request_restart` | Substrate. No Host. |

The GIGA tools refuse with `giga_disabled` unless `ATHANOR_GIGA_ENABLED=1`
(giga.ts:107-108). The Knock doorman and the inbox Bell projection also need the
Host (knock.ts:77-114; hallway.ts:26-30). [`docs/ARCHITECTURE.md`](./docs/ARCHITECTURE.md#43-tools-and-wires)
and [`docs/LIMITATIONS.md`](./docs/LIMITATIONS.md#7-doors-that-need-the-host) give the same split.

## Vault and AKASHA workflows

### Vault

Vault is the local transparent profile. `recall` uses the Vault lane when
`ATHANOR_SUBSTRATE_ROOT` is unset (recall.ts:123, 150). Vault still needs the
Host, and the Host needs PostgreSQL. `recall` fails without the Host
(tools.ts:516, 526-532). The Host starts only after the PostgreSQL port answers
(app.rs:88; service.rs:43-46, 83-85).

Not re-verified at a6ab453 (the `substrate` crate decides): the rest of this
section.

Vault searches the configured Markdown, JSON, JSONL, and text corpus. Results
keep the exact source path and Markdown heading, JSON pointer, or JSONL line
identity. Field-aware BM25F ranks paths, titles, headings, structured keys,
tags, metadata, and bodies. Exact identifiers, filenames, symbols, quoted
strings, UUIDs, and errors receive a separate direct-content lane.

The room directory is the default corpus. A room marker may point at several
projects:

```json
{
  "vaultRoots": ["../project-a", "../project-b"],
  "vaultIgnore": ["private/**", "generated/**"]
}
```

Vault remains file-authoritative. Its in-memory search index is derived,
bounded, briefly cached, and rebuildable.

### AKASHA

Not re-verified at a6ab453 (the `substrate` crate decides): AKASHA adds:

- durable tool-backed memories and typed lessons;
- PostgreSQL authority and pgvector retrieval;
- local embeddings and hybrid candidate fusion;
- entities, dates, threads, taxonomy, relationships, and clusters;
- correction, supersession, archival, paper boats, and Cabinet counsel.

`room_state` does not report Vault or AKASHA health. `house_lane_status` probes
substrate health, and it needs the Host (tools.ts:1020-1031).

## First week

Start with six habits:

1. Begin each session from the room directory.
2. Ask for recall when older evidence matters.
3. Save one or two meaningful memories instead of everything.
4. Cast a paper boat before ending an important session.
5. Catch it when you return.
6. Correct the record when either of you notices drift.

That is enough. House becomes deeper as the room accumulates deliberate continuity.

## Reference map

- Install, migrate, and update: [`INSTALL.md`](./INSTALL.md)
- Architecture as built: [`docs/ARCHITECTURE.md`](./docs/ARCHITECTURE.md)
- Retrieval and corrections: [`docs/RETRIEVAL.md`](./docs/RETRIEVAL.md)
- Typed lessons and imports: [`docs/LESSONS.md`](./docs/LESSONS.md)
- Identity and room design: [`IDENTITY_GUIDE.md`](./IDENTITY_GUIDE.md)
- Privacy and destructive operations: [`docs/SECURITY.md`](./docs/SECURITY.md)
- Platform and product boundaries: [`docs/LIMITATIONS.md`](./docs/LIMITATIONS.md)
- Project history and design reasons: [`HOUSE.md`](./HOUSE.md)
