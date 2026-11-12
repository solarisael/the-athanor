# Using The Athanor

Installation provides persistent homes for spirits. A conversation can use a home from a separate project directory.
Managed homes and mutable state live under `%ProgramData%\Solarisael\Athanor`.
Immutable releases live under `%ProgramFiles%\Solarisael\Athanor`.
Read [`INSTALL.md`](./INSTALL.md) before the first installation.

The Host supplies identity and continuity. Durable memory writes remain deliberate.

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
House tools and worker enrollment need the Host. See [Tools and the Host](#tools-and-the-host).

## Use a spirit inside a project

Portable sessions require schema 36 and the matching OMP component.

1. Start OMP in the project directory.
2. Enter `/spirit kintsu` to select the configured `kintsu` home.
3. Use `/spirit` to inspect the attachment and configured home keys.

The command does not change the working directory or the home's shared identity.
Project instructions remain in effect. Home instructions enter the model context through the adapter.
Other supported clients use the same authenticated Host session endpoint.

The selection belongs to the conversation. Separate conversations can share a home and a workspace.
OMP normally saves a new conversation after its first assistant message.
A resume restart refuses until the conversation file exists.

Use `/spirit off` to detach. This choice also overrides a launcher-provided home.
A saved selection that cannot attach does not silently become an ordinary session.
An unbound project remains an ordinary OMP session.

Use `home_resources` to list or read personal text documents.
Use the returned revision with `home_resource_write`.
Set `expectedRevision` to `null` only when creating a document.
Conflicting writes refuse instead of replacing newer content.
Documents remain in the home. They are not copied into the project.
Runtime files, path escapes, and links are excluded from this document interface.

Use `house_sessions` to inspect live instances and supported controls.
Use `session_control` with both the instance ID and conversation ID.
Controls include delivery, interruption, compaction, close, and supported restart.
Capability availability does not replace authorization.

A delivered compaction without a receipt after 120 seconds reports `outcomeUnknown`.
The original instance and conversation can report completion later.
The Host does not deliver this command again.
An undelivered command still expires. Restart and close deadlines remain unchanged.

Use `/spirit chat on` to claim room chat delivery.
Use `/spirit chat off` to release it.
Another live recipient is never displaced implicitly.

Tern restart requires a user-run Host, a reachable local Tern daemon, and provisioned restart capabilities.
`request_restart` supports `resume` and `fresh`.
A fresh restart requires an existing paper boat.
Restart opens a new tab after the predecessor exits.
The successor receives the home and harness profile through a private, one-use native handoff.
No restart command is typed into a shell.
The Host verifies the exact successor pane and instance.

One restart can be active per workspace. The native storm guard also limits repeated exits.
Interrupted or uncertain operations remain visible. They are not automatically replayed after a Host restart.


## Everyday loop

1. **Select a home.** Start inside its configured directory, or use `/spirit <home-key>` from a project.
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
Select a configured room from Direct.
Open Status to inspect its registered sessions.
Select an exact instance before you review a session command.
Check the target and effect before confirmation.
The app keeps room chat separate from session controls.
Memories, lessons, Hallways, and Docket records remain read-only.
Interface preferences persist in this browser.
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

## Work with task notebooks

OMP `task` children receive individual wakes and temporary notebooks.
They do not receive the room's identity, paper boat, Anamnesis, Hallway Bell, or Presence.
Each notebook belongs to one task lineage within a swarm.
Sibling workers cannot read each other's books.

Named kittens and dragonlings also receive their role lessons with every wake, resume included.
Tag those coding lessons as [House Lessons](./docs/LESSONS.md#familiar-role-lessons) describes.
A missing role lesson stops the familiar, also in the middle of a task.
The parent keeps the book and continues it with `worker_notebook` repair and successor.

Use `worker_notebook` with these actions:

| Action | Use |
|---|---|
| `inspect` | Read the owned book and instance summaries. |
| `read` | Read up to 50 entries after a sequence number. |
| `append` | Add a worker note or parent evidence. |
| `accept` | Preserve a self-contained report and evidence before disposal. |
| `retained` | Read accepted evidence by swarm, including after disposal. |
| `repair` | Append a parent-attributed repair to an incomplete handoff. |
| `successor` | Prepare a task packet from an exact retired predecessor. |
| `dispose` | Remove a whole inactive swarm's transient records. |

Workers use their own bound book.
Parents control acceptance, repair, succession, and disposal.
An ordinary completed worker can resume with its own checkpoint.
At 40% context usage, the worker receives a warning.
At 60%, it enters handoff-only mode with at most two final provider requests.
After retirement, the parent must start a successor with the returned task packet.
A missing or incomplete handoff never selects the room's boat.

Accepted evidence and disposal receipts remain available after disposal.
For `retained`, use `swarmId` and optional `cursor` and `limit`.
Only the owning parent session can read this evidence through the notebook tool.
Existing backups remain unchanged.
Room books and optional context folding are separate from this worker lifecycle.

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

The Host and OMP adapter provide these parts:

- active-room discovery;
- identity and compact continuity loading;
- live session context;
- conversation logging;
- bounded relevant-context injection;
- paper-boat recovery near session start;
- native attributed Vault retrieval when AKASHA is not configured.

Use explicit recall for load-bearing old decisions, names, promises, corrections, or important memories.

## Orihon: the room book

Orihon records visible user messages, replies, tool calls, tool results, and attributed reports.
Each spirit has a saved recording default within its room. New spirits start with `log`.
Each session loads that default when it attaches. Existing sessions keep their effective settings.
A mode command changes the current session immediately and saves the default for new sessions of that spirit.
Use `/orihon status` to compare `thisSession` with `defaultForNewSessions`.
The status also shows the model and the current session's latest folder outcome.
Use `/orihon off` to stop this session's new records and cancel its folder job.
Use `/orihon log` to record in this session without automatic folding.
Use `/orihon fold MODEL:TAG` to enable recording and local folding.
Use `/orihon fold hosted` to select hosted folding with Sonnet 5.5.
Use `/orihon fold hosted PROVIDER/MODEL` to select another supported OMP model.
Each new session receives its own recording generation.
An off-to-log or off-to-fold transition starts a new generation for the current session.

Each session starts the new generation at its next user page and shows a notice.
Orihon does not import earlier history or records from an off interval.
A normal restart keeps the existing generation and resumes from its accepted checkpoint.
Reattaching the same spirit and session preserves its effective settings, even when the saved default has changed.
Existing room-wide settings migrate once to the current spirit's default. This migration preserves an explicit `off` setting.
Capture failures show a warning. Ordinary work continues.
The adapter retries from the retained harness log.
A missing source remains an explicit gap.

Use the read-only `orihon` tool to read a page, sequence range, or UTC source day.
Reads return at most 200 records and a continuation cursor.
The book is provenance. A row records what was said, not what is true.
Canon and memory outrank the book.
Use `orihon` with `query` to find user pages in your own words.
Read the selected `turnNo` before you rely on a local search candidate.
Search uses bounded BM25F ranking. Tool output does not enter the candidate pool.
The pool holds at most 512 user rows of at most 64 KiB each.
Results hold at most 32 candidates and 128 KiB of exact source text.
An incomplete search reports its limits. It does not clip source text.

AKASHA stores accepted rows in PostgreSQL and exports exact rows to `<room>/orihon/YYYY-MM-DD.jsonl`.
Vault uses those daily files as its durable book.
The Host never switches stores after a failure.
Credential-pattern redaction precedes storage. Arbitrary unmarked secrets are not guaranteed to match those patterns.
Echoes retain their head and tail within 30,000 Unicode characters.
Non-echo text is limited to 1 MiB. Prepared batches are limited to 4 MiB.
Worker sessions and `ATHANOR_REPLAY_MODE=1` do not create book records.

OMP capture excludes model thoughts, injected context, images, and external artifact expansion.
Tool attempts refused before OMP dispatches `tool_call` are unavailable to this observer.
OMP retains its own source log; Orihon does not promise power-loss durability for that source.
The existing Markdown transcript moves to `<room>/orihon/YYYY-MM-DD.md` on its next write.
Migration preserves bytes and turn markers. A destination conflict preserves both files and refuses the write.
Orihon mode does not disable the harness log or the existing Markdown transcript.

### Live folding

Generated summaries can omit conditions or attribution. Read the exact sources before you rely on a summary.
The folder stores derived provenance. It does not create memory or canon.
Keep recording on `log` until the selected model meets your needs.

In `fold` mode, each new idle user turn sends three parts to the model.
The first part is the stable system prompt and the tool definitions.
The second part is the persisted chat view, a summary-only user message.
The third part is the current user message.
Previous raw turns and earlier compaction summaries do not cross a user-turn boundary.
The current turn keeps its tool calls, tool results, and steering messages.
A request waits for the view inside the turn budget.
When the view is not ready, the adapter refuses the request and shows the reason. It never sends the old conversation instead.
Send the message again when the compactor is ready, or run `/orihon status`.

The Host builds the view from a message tree.
Short texts stay verbatim. Longer texts use the model. Each summary targets 512 UTF-8 bytes.
The chat view appends new summaries between batches.
Above 128,000 UTF-8 bytes it merges older lines down to 64,000 bytes.
The compactor reads its own persisted view of 16,000 to 32,000 bytes.
The Host saves the exact view state. A restart restores the same bytes.
At most 8 model requests run at the same time. A failed request retries at a later message boundary.
Each job records its attempts, usage, and price. An unknown price stays unknown.

Use the `zoom` tool with a line id and width to open the two children of a summary, or the exact source record.
Use the `date` tool with a line id to read the source timestamp.
Task children receive the bounded view and their task. Their private logs stay private.
A combined task report keeps the exact identity of each child transcript.

OMP compaction in a `fold` session writes a fixed note and summarizes nothing. The view carries the conversation.
Handoff, advisor, and title requests outside an agent loop are not changed.

`/orihon status` reports the live state: units, built leaves, running and failed jobs, view bytes, and the last error.
The Host cancels running jobs on shutdown or when the last live session leaves.
Cancellation does not guarantee that Ollama stops its internal computation.
Hosted cancellation terminates the owned gateway process before it releases generation ownership.
Cancellation does not attest that an upstream provider stops its internal computation.

### Hosted folding

Warning: hosted folding sends conversation text through the selected OMP model route.
Changing this session's setting does not stop hosted folding in another active session.

Run `/orihon fold hosted` to select `anthropic/claude-sonnet-5-5`.
Keep the current session open. The Host summarizes this room's live records in the background.
Run `/orihon status` to inspect the provider, model, and outcome.
Run `/orihon log` to stop this session's folding while its recording continues.

Supported model selectors use the `anthropic` or `openai-codex` provider.
The selected model must appear in the bound OMP chat catalog.
Sonnet, Haiku, and Luna use their exact catalog selectors.
The hosted context allowance is 131,072. The summary limit remains 512 UTF-8 bytes.

The command binds the current executable, optional Bun entrypoint, operator home, agent directory, working directory, and configuration files.
The Host must run under the same operating-system account.
OMP owns credential discovery. Provider credentials do not enter Orihon settings or receipts.
Use absolute paths for configuration overlays when the original launch directory is unavailable.
Run the command again after moving the runtime or changing its profile binding.

The Host runs one multiplexed gateway process for the room's live engine.
Hosted requests use the same system prompt and tool definitions as the turn, so the provider can reuse its cache. The Host never claims that cache reuse happened.
The scoped configuration preserves the operator's overlay order and disables OMP model fallback and Anthropic server-side fallback.
The gateway uses the operator's configured logical model route, including configured proxies and wire-model mappings.
Its returned selector does not attest the model that an upstream service actually executes.
OMP 18.12.6 passed the hosted runtime proof. Other versions must provide the same gateway and configuration capabilities.


### Remote navigation judgment

Remote judgment sends private conversation text to TypeSafe. Enable it only with explicit approval for the room.

Without approval, navigation returns local candidates for the speaking model to inspect.
The separate `jevOrihon` grant lives in the room's `.athanor-room.json`.
It requires `mode: "active"`, `provider: "typesafe"`, and the marker's matching `room`.
Its `grant` requires `purpose: "orihon-navigation"`, `allowPrivateOrihonPackets: true`, and a nonempty `policyRevision`.
The adapter obtains the TypeSafe credential through OMP's existing credential resolver.
Recall reranking consent alone does not authorize this packet.

Jev first selects user pages, then exact rows within those pages.
Close page choices receive independent row judgments.
The response preserves confidence, the choice distribution, and abstention.
Privacy, size, credential, and provider failures remain visible.
No remote packet exceeds the existing 32 KiB judgment limit.
An oversized row-selection packet returns an exact-page read instruction instead of clipped source.
Folding consent and the Jev navigation grant are separate settings.



## Tools and the Host

The adapter registers the public tool families below.
House operations use authenticated Host commands.
The adapter does not start substrate children or provide a fallback when the Host is absent.

| Family | Tools | Runtime |
|---|---|---|
| Continuity | `wake`, `sleep`, `remember`, `thread_memories`, `recall`, `canon_read`, `canon_write` | Host and native storage |
| Lessons and design | `lessons`, `update_lesson`, `delete_lesson`, `design_doc`, `design_doc_write` | Host and native storage |
| Anamnesis | `anamnesis`, `anamnesis_write` | Host and native storage |
| Room | `room_state`, `set_room_state`, `house_routing_mode`, `house_model_default` | Native state; OMP performs model selection |
| Hallway | `hallway_create`, `hallway_join`, `hallway_post`, `hallway_knock_policy`, `hallway_knock`, `hallway_read`, `hallway_inbox` | Host and native storage |
| Docket | `quest_post`, `quest_board`, `quest_claim`, `quest_report`, `quest_evidence` | Host with native capability checks |
| GIGA | `giga_candidate_list`, `giga_health`, `giga_queue_maintenance`, `giga_review`, `giga_promote_memory`, `giga_promote_coding_lesson`, `giga_promote_project_lesson` | Host-owned native services |
| Dispatch | `house_lane_status`, `house_dispatch`, `familiar_status`, `familiar_dispatch` | Host |
| Recall policy | `recall_policy` | Host |
| Restart | `request_restart` | Native authorization; OMP performs the exit |
| Local observation | `kitten_lineage_status` | Adapter process |
| Workers | `worker_notebook` | Host; PostgreSQL or a Vault runtime journal |
| Room book | `orihon` | Host; PostgreSQL or a Vault book |

GIGA capture and classification remain opt-in.
The Host receives explicit enablement from the top-level harness session.
Read-only GIGA queries do not start workers.
Replay cannot change another producer.

See [Architecture](./docs/ARCHITECTURE.md#4-the-omp-adapter) for ownership.
See [Limitations](./docs/LIMITATIONS.md#7-doors-that-need-the-host) for unavailable dependencies.

## Vault and AKASHA workflows

### Vault

The Host chooses retrieval from its bound storage configuration.
A Host without AKASHA configuration uses native Vault retrieval without PostgreSQL.
Vault keeps file authority.
The supported installer still provisions PostgreSQL; a database-free packaged installation is not implemented.

The Host-to-Vault path has a live isolated proof.
The broader corpus and ranking behavior below retains its earlier verification scope in `crates/vault`.

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
