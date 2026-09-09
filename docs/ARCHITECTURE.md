# The Athanor Architecture

The Athanor separates durable continuity from the model session that consumes it. Models and harnesses remain replaceable; rooms, identity, memory, and authority remain under the operator's control.

## System boundaries

```text
operator
   │
   ▼
room directory
   │
   ├── identity contract
   ├── room state
   ├── compact continuity
   └── room marker
   │
   ▼
harness adapter
   │
   ├── room discovery
   ├── lifecycle hooks
   ├── conversation logging
   ├── tool registration
   └── context injection
   │
   ▼
House core
   │
   ├── room and identity contracts
   ├── retrieval orchestration
   ├── ranking and authority
   ├── memory context shaping
   └── worker-routing contracts
   │
   ├──────── Vault ───────── room-local files
   │
   └──────── AKASHA ──────── PostgreSQL + pgvector + embeddings
```

### Host and delivery plane

The Athanor Host is the authenticated, versioned boundary between interactive clients and the Rust runtime.
The web prototype at `gui-prototype/` is the read-only operator surface.
Run `bun gui-prototype/serve.ts` from the repository root.
It reads the Host through a loopback proxy.
The Godot client is parked.

```text
Web prototype ── loopback proxy ── Athanor Host ── Rust contracts ── Vault / AKASHA
OMP adapter ──────────────────────┘                                  │
                                                                    └── PostgreSQL outbox
                                                                                │
                                                                                └── NATS JetStream
```

PostgreSQL remains authoritative. The outbox records durable publication intent
in the same transaction as the domain event. NATS JetStream may deliver opaque
record IDs and wake consumers, but it never becomes memory, review state, or a
second conversation ledger.

Paper Boat sleep commits its continuity row and `boat.ready` Crane outbox event in
one PostgreSQL transaction. NATS carries only bounded pointer and sanitized receipt
projections; wake reloads the complete Boat from PostgreSQL authority. That lane is
the first lane of one general Crane delivery system: a shared outbox, receipt, and
dead-letter trio, subjects routed per lane, and optional crease, recipient, expiry,
and lineage fields on the envelope. Origami folds, Pawprints, and room wake
behavior on the addressed lanes remain specified extensions of the same envelope
rather than current release claims.

`boat.ready` is the only current production outbox producer. Addressed Crane
subjects, envelopes, and generic receipt validation exist structurally and in
tests, but no named worker, familiar, room, or reviewer currently consumes and
applies them. A current Paper Boat receipt proves transport validation; it does
not prove room wake, model consumption, or human reading.

The complete accepted target, including dynamic model and room execution,
Prolog/Datalog derivations, Cingulate enforcement, and optional Lean-backed
lessons, lives in
[`RUNTIME_ARCHITECTURE.md`](./RUNTIME_ARCHITECTURE.md).

## Repository layout and component ownership

One repository owns The Athanor. The public repository is
[`solarisael/the-athanor`](https://github.com/solarisael/the-athanor). One
release publishes every component below. This table is canonical. Other
documents link to it instead of repeating it.

| Component path | Responsibility |
|---|---|
| `crates/hearth`, `crates/protocol` | Provider-neutral domain, authority, and wire contracts |
| `crates/origami`, `crates/summoning` | The communication layer (paper boats, Cranes, Hallways) and the waking cycle's typed requests (Anamnesis, Presence) |
| `crates/vault` | Strict database-free, file-authoritative Vault retrieval |
| `crates/akasha`, `substrate/` | PostgreSQL-authoritative AKASHA operations, migrations, retrieval, typed stores, Docket, GIGA, Insula, health, backup, and restore |
| `crates/host` | One authenticated multi-room listener, snapshots, deltas, Recall Policy, receipt projection, and the Crane delivery task |
| `crates/athanor-install`, `crates/omp-keeper`, `crates/interactive-process`, `installer/` | `athanor.exe`: native service lifecycle, immutable staging, rollback, doctor, OMP session keeper, and the Windows installer |
| `gui/`, `gui-prototype/` | Godot operator client (parked) and the web operator surface |
| `adapters/omp/` | OMP entrypoint, room integration, named tools, and Rust transport |
| `.github/workflows/` | Continuous integration and native release assembly |
| `docs/`, root Markdown | Canonical documentation |

One repository does not merge the internal authority boundaries. The core does
not import the OMP adapter. The core does not require PostgreSQL. The Vault
profile runs without any substrate component. Each boundary stays enforced by
contract, not by repository distance.

The public API boundaries are `hostApi=1`, `substrateApi=1`, `deliveryApi=1`,
and `godotApi=4.7` for the parked client.

### Installed layout

Immutable product versions and mutable operator data are separate:

| Installed path | Content |
|---|---|
| `%ProgramFiles%\Solarisael\Athanor\bin` | Stable independent manager and Host owner, lifecycle manager, and OMP loader |
| `%ProgramFiles%\Solarisael\Athanor\versions\<version>` | Verified immutable runtime, OMP adapter, parked Godot client, PostgreSQL, and NATS |
| `%ProgramData%\Solarisael\Athanor\config` | Non-secret database mode, one Host port, and House room identities |
| `%ProgramData%\Solarisael\Athanor\secrets` | ACL-restricted service secrets |
| `%ProgramData%\Solarisael\Athanor\data` | Managed PostgreSQL and NATS durable data |
| `%ProgramData%\Solarisael\Athanor\state\host\<room>` | Isolated Host projection state per room |
| `%ProgramData%\Solarisael\Athanor\backups` | Upgrade, rollback, external-authority, and legacy pre-install backups |
| `%USERPROFILE%\.omp\agent\athanor\client.json` | ACL-restricted Host base URL, token, and room identities |

`current.json` atomically selects the active immutable version. The
`SolarisaelAthanor` Windows service starts optional managed PostgreSQL, one NATS
broker, and one delivery worker. It reports `RUNNING` after these children pass
readiness. It drains the same children in reverse order.

`athanor.exe` is an independent local manager and the one in-process multi-room
Host owner. It binds one loopback listener without launching the parked Godot client or becoming
the parent of OMP sessions. Every WebSocket and HTTP route starts with
`/room/<room-key>`. One shared PostgreSQL pool serves all room projections.

OMP registers one stable loader under Program Files. The loader follows the
native and adapter activation pointers, reads the restricted client projection,
and checks the default room's scoped Host health. When Athanor is absent it
starts the verified stable `athanor.exe` as an independent hidden peer, waits
boundedly for real scoped health, then loads the adapter. Unknown room paths
fail closed. Install removes duplicate development registration owners.

### Release and support target

`0.9.6` is the source version label carried by this documentation. It is a
labeled historical snapshot, not the current product version. The root
[`package.json`](../package.json) declares the current product version. The
installed immutable version manifest declares the installed build; dated
evidence in [`EVIDENCE.md`](./EVIDENCE.md) and [`BUGS.md`](../BUGS.md) names
later `0.5.4+dev.…` installed builds. This document does not change version
strings. OMP is the supported harness. The release artifact shape is one
checksum-published installer:

```text
The-Athanor-<version>-windows-x64.exe
The-Athanor-<version>-windows-x64.exe.sha256
```

The payload pins PostgreSQL 18.4-2, pgvector 0.8.6, NATS 2.14.4, and parked Godot
4.7.1. The service needs no WSL, Python, Bun, Cargo, or separate database/broker.
The parked Godot client needs no editor.
An explicit advanced mode may use an operator-provided compatible PostgreSQL database.

The OpenCode adapter line and the two portable Vault/AKASHA archives are
historical. Vault and AKASHA are runtime authority profiles inside one release,
not separate packages.

## Room model

A room is a writable directory with one stable lowercase key. Display names may change without changing the room key.

The Vault room contract includes:

- `.athanor-room.json` for machine-readable room identity;
- `AGENTS.md` as the host context entrypoint;
- `active_spirit.md` as the active identity and voice contract;
- `room_summary.md` as compact continuity;
- room-local state and conversation artifacts owned by the adapter.

Rooms are isolated by default. The core resolves an explicit room directory and validates the room key before loading identity or memory. Invalid or missing room paths do not borrow another room or the process working directory.

## Context layers

House keeps four concerns separate:

| Layer | Purpose | Typical lifetime |
|---|---|---|
| Identity | Who is present and how the identity or working role is expressed | Stable, deliberately revised |
| Current state | Active operator, spirit, room, and safe mutable metadata | Current room state |
| Recent continuity | Compact handoff and live session context | Sessions to days |
| Deep memory | Events, decisions, lessons, entities, threads, dates, and source evidence | Durable archive |

This separation lets a new session load a small identity and continuity surface while retrieving deeper evidence only when the current turn needs it.

## Current operational capability topology

The reference implementation is more than the four storage/context layers
above. The table below is the current machine-readable map for cold evaluators:

| Surface | Current role | Owner | Authority |
|---|---|---|---|
| Room identity and state | Stable room key, operator, active spirit, identity contract, compact live context | Rust authority + thin adapter + operator-owned files | Room files and explicit state writes |
| Vault retrieval | Attributed Markdown, JSON, JSONL, and text search through exact-content and field-aware BM25F lanes | Rust core, exposed by the adapter | The selected files remain authoritative |
| AKASHA memory | Durable memories, chunks, entities, dates, threads, continuation edges, relationships, taxonomy, and lifecycle | Substrate + PostgreSQL | PostgreSQL; current/superseded/archived state is explicit |
| Paper boats | Room-scoped continuity across closed sessions, including stale-boat detection when later memories exist | Rust substrate + adapter presentation | Orientation for the next session, not canon |
| Typed lessons | Coding, project, writing, design, and audio guidance with store-specific scope and proof fields | Rust contracts + substrate stores | Active typed record within its declared scope |
| Context analysis | Query classification, keyword/process triggers, and context-pressure nudges returned through a typed Host command | Rust core + Host | Policy result; no durable authority |
| Recall viewport | Evidence qualification and per-session saturation over attributed Recall candidates | Rust Host | Selects a bounded view; source authority is unchanged |
| Canon and controlled vocabulary | Load-bearing assertions plus bounded lexical expansion from authoritative entities, active threads, and lesson metadata | Rust core + substrate | Canon governs generation; expansion only locates evidence |
| Anamnesis Cabinet | Reviewed pillars and lived cycles supplied as bounded counsel | Substrate + adapter presentation | Advisory only; never canon or memory authority |
| GIGA Hippocampus Stage 1 | Exact turn events, asynchronous classification, non-authoritative candidates, review, Curios, promotion, and queue maintenance | Adapter event translation + substrate worker/store | Candidate until reviewed and explicitly promoted |
| Design-system catalogue | Typed immutable/superseding design tokens, components, contracts, and guidelines, read through `design_doc` and written through `design_doc_write` | Substrate + adapter registration | Current catalogue record within the named design system |
| Worker routing and familiars | Deterministic lanes, room-owned spellbooks, and validated harness-ready task packets | Typed Rust core + Host; adapter reads the room-local spellbook and presents the packet | Routing policy only; no memory or room authority |
| Subagent lineage | OMP lifecycle/result shapes normalized into standalone quest-memory requests | Typed Rust core + Host; adapter translates OMP events | PostgreSQL becomes authoritative only after the ordinary memory write receipt |
| Rust transport and health | Long-lived JSONL requests, cancellation/timeouts, crash replacement, compatibility checks, redacted diagnostics, and uncertain-write reconciliation | OMP transport skin + Rust substrate | Transport carries receipts; it does not become authority |

These surfaces are deliberately separate. A lesson is not a memory, Cabinet
counsel is not canon, a GIGA candidate is not evidence, a familiar is not a
second routing system, and transport success is not proof that a stored claim is
true.

Detailed contracts live in [`RETRIEVAL.md`](./RETRIEVAL.md),
[`LESSONS.md`](./LESSONS.md), [`HIPPOCAMPUS.md`](./HIPPOCAMPUS.md), and
[`RUNTIME_ARCHITECTURE.md`](./RUNTIME_ARCHITECTURE.md).

<a id="critical-organ-review-2026-09-06"></a>

## Critical organ review — 2026-09-06

Sol accepted this review on 2026-09-06. This pass updates records and planning.
It changes no runtime, migration, deployment, version, or frozen acceptance terms.
The Athanor preserves records and governs changes.
Reliable continuity, judgment, and completed work need more complete paths.
Success means recognition, growth, agency, cooperation, and operator custody.
Autonomy contributes to these outcomes.

The accepted outcome sequence is: coherent orientation, then trustworthy
visible outcomes, then carrying work between participants, then useful
learning. Release order and gates stay in [`roadmap.md`](./roadmap.md).
Each family below states the examined behavior, remaining gap, and proposed outcome.
Remaining proposals stay unfinished; completed slices cite separate evidence.
Installed observations name their dates; source findings do not certify current deployment.

| Family | Current behavior (source) | Remaining gap | Recommended outcome |
|---|---|---|---|
| AKASHA and Vault | AKASHA stores typed PostgreSQL authority. Vault retrieves file-authoritative evidence. See [Profiles](#vault). | Storage alone does not prove portable custody. | Preserve authority during export, migration, and restore. Keep external sources distinct from House-authored memory. |
| Memory and canon | Typed records retain source identity and supersession. See [Authority and correction](#authority-and-correction). | Old operational claims can outlive their observation. | Show observation date, artifact, and successor evidence. Distinguish commitments, history, interpretations, and current state. |
| Context capacity nudge | [`context.rs:737-764`](../crates/hearth/src/context.rs) derives nudge capacity from the room key. It assumes 1,000,000 tokens for `kodo` and 400,000 elsewhere. | The nudge does not read the actual model limit. | Obtain capacity from the active runtime. Preserve room identity when the model changes. |
| Aggregate context selection | Each organ selects context separately. No [aggregate coordinator](#context-assembly-and-token-budgets) exists. | Several valid organs can exceed a useful combined budget. | Apply one relevance and size budget across the assembled turn. Preserve exact recovery for omitted evidence. |
| Lessons and Striatum | [`triggers.rs:22-107`](../crates/hearth/src/triggers.rs) requests at most twelve process coding lessons and emits complete bodies, proof, and trigger fields. | The formatter has no content-size cap. Full learned work-state behavior remains planned. | Bound content and count. Select a stable applicable set. Record relevant use and misleading triggers before claiming improvement. |
| Recall lanes | The semantic lane timed out at 3000 ms during the review. Lexical evidence still returned. | This observation does not measure overall retrieval quality. | Preserve degradation attribution. Examine the cause and affected query classes before changing limits. |
| Presence, room state, and Summoning | [`BUGS.md`](../BUGS.md) records live reopen, persistence, and restart continuation. The [generated-turn evidence](./EVIDENCE.md#generated-turn-presence-repair-2026-09-07) covers live restart, chat, and root Knock delivery. | Host-side session attribution is unchanged. Chat response and child Knock failures have separate repair work. | Repair the separate paths without reopening proven persistence or incoming delivery. |
| Paper boats, wake, sleep, and Keeper | [`wake.rs:21-32`](../crates/origami/src/boats/wake.rs) selects the newest room boat. The [adapter](../adapters/omp/house-proof/wake-context/index.ts) retains its rendered letter and identity fields. | Separate age and warning metadata are dropped. The rendered letter can still contain a stale-boat warning. | Carry temporal metadata. Keep the authored letter distinct from an interruption checkpoint. Preserve the existing handoff contract. |
| Anamnesis | [`anamnesis.rs:362-377`](../crates/akasha/src/anamnesis.rs) selects wake-enabled pillars or active cycles by kind and update time. | The selector has no cycle-recency condition. | Select cycles by temporal and current relevance. Preserve authored pillars and counsel authority. |
| Durable writes and backup | [`BUGS.md:105`](../BUGS.md) records successful backups for `remember` #4472 and `sleep` #4473. They took 47.2 and 49.0 seconds. | [`backup.rs:929-1012`](../crates/akasha/src/backup.rs) awaits a full post-write dump after commit. Generic write success does not prove a valid `continues` edge. | Preserve commit and backup outcomes separately. Choose a recovery policy before replacing full dumps. |
| Design catalogue | [`BUGS.md:151`](../BUGS.md) records live same-identity supersession. Line 152 records separate adapter field loss and the palette repair through #25. | That record leaves historical rows #13-#23 unresolved. This review performs no new database census. | Audit exact structured content through the tool boundary. Recover historical values only from exact sources. |
| GIGA Hippocampus | The aggregate result reported capture and classification enabled, store healthy, queue 0, failures 0, and processed 430. | These fields do not prove classifier reachability, useful classification, reviewed consolidation, or later benefit. Draft `704ce3e2` records dismissed-candidate purge history. | Prove fresh classification and an attributable review outcome. Follow useful promotions into later retrieval. |
| Curios | Retained candidate state and explicit review are current. | Bounded automatic resurfacing is not established. | Return a retained Curio to review with cited new evidence. Prevent automatic promotion. |
| Cingulate | Cingulate remains planned. | Reliable lifecycle and outcome evidence must precede its judgments. | Start with specific evidence obligations. Keep formal solver expansion deferred. |
| Hallway | The domain, Bell projection, and recipient-authorized bounded Knocks exist. | Complete idle/headless delivery and recipient application remain unfinished. NATS absence alone is not a defect. | Complete request-to-disposition, including unavailable recipients, interruption, refusal, and duplicate handling. |
| Docket review independence | [`report/mod.rs:130-147`](../crates/akasha/src/docket/report/mod.rs) fences settlement by room. The claimant room cannot settle its own items. | A single-room House needs an explicitly supported independent reviewer or operator arrangement. | Bind reviewer authority to authenticated capabilities. Preserve independent review and frozen acceptance terms. |
| Dispatch and familiars | Dispatch prepares validated packets. The main model spawns explicitly. Familiars retain their lane bindings. | Complete attempt-to-execution evidence through interruptions is not established by packet validation. | Connect actual execution and evidence to existing Docket attempts. Avoid a parallel work store. |
| Insula | Source measurements and Pulse traces exist. [`BUGS.md:30`](../BUGS.md) records incorrect session attribution for Host-side Presence points. | Those points cannot reliably identify the participant's turn. | Correct attribution and preserve durable evidence separately from short-lived telemetry. Insula observes; it does not authorize work. |
| Pulse web surface | [`serve.ts:26-39`](../gui-prototype/serve.ts) allowlists reads for health, Insula, Docket, Hallway, memory, and lessons. | The surface is read-only. Fixtures do not establish operational writes. | Show each completed path and its evidence. Keep authenticated writes behind their existing gates. |
| Origami and NATS | The current `boat.ready` lane has transport receipts. See [Host and delivery plane](#host-and-delivery-plane). | A transport receipt does not establish recipient application, model consumption, or human reading. | Complete recipient-specific application receipts when that expansion is authorized. Keep PostgreSQL authoritative. |
| Version labels | Documentation retains `0.9.6`. The root `package.json` declares `0.5.4`. Dated evidence identifies installed artifacts separately. | These labels describe different records and dates. | Use `package.json` for product version and the installed manifest for active bytes. Keep historical evidence labeled. |
| Workspace search | Workspace search `0.1.1` was accepted in the preceding session (House memory #4499). It uses explicit per-root indexing and has no watcher. | The older Whiskers draft is historical planning. | Keep repository perception separate from AKASHA. Preserve the accepted search scope. |
| Sovereignty and installation | The [component table](#repository-layout-and-component-ownership) names backup, restore, upgrade, and rollback surfaces. | This review performs no fresh restore or complete custody certification. | Prove complete export, restore, and migration. Define operator retention and deletion choices. Keep OMEGA, Relay, ANON, marketplace, and spatial work deferred. |

House memory **#4509** holds the complete accepted analysis in PostgreSQL.
The [roadmap update](./roadmap.md#planning-update-2026-09-06) links its Docket goal and four draft supplements.
Those supplements cite existing work and preserve claims, deadlines, frozen acceptance, and the writing priority.
This review does not supersede historical memories or invalidate accepted search work.

## Vault

Vault uses operator-controlled files and the harness adapter. It provides:

- stable room discovery;
- identity and compact context loading;
- conversation continuity artifacts and restart recovery;
- room-state tools and multiple isolated rooms;
- native local recall over Markdown, JSON, JSONL, and eligible text files;
- field-aware BM25F over paths, titles, headings, structured keys, tags,
  metadata, and bodies;
- an exact-content lane for identifiers, filenames, symbols, UUIDs, quoted
  strings, and errors;
- bounded attributed excerpts with exact source paths and heading or record
  identity.

Vault requires no database, vector index, embedding service, or GPU. Vault also
requires no substrate binary, no PostgreSQL, no WSL, and no Rust runtime. Its
in-memory index is derived and rebuildable from authoritative files. Recall
defaults to the room directory; `.athanor-room.json` may name one or more
operator-controlled `vaultRoots`, additional `vaultIgnore` patterns, and bounded
`vaultMaxFileBytes` or `vaultMaxFiles` limits. The scanner does not follow
symlinks, skips common generated and secret-bearing paths, and honors each
configured root's top-level `.gitignore`.

## AKASHA

AKASHA adds the substrate as the durable memory authority. PostgreSQL stores memories, entities, threads, chunks, clusters, GIGA candidates, typed lesson stores, and the controlled semantic vocabulary. Native BM25F scores memory title, heading, source path, threads, body, and type with corpus IDF, term-frequency saturation, and per-field length normalization. PostgreSQL full-text search, `pg_trgm`, direct content search, structured rails, BM25F, and pgvector semantic search contribute retrieval candidates.

The tested local embedding path uses Nemotron-3-Embed-1B with 2,048-dimensional vectors through a compatible local endpoint. Recall reuses its query vector to select at most three sufficiently similar, room-scoped concepts derived only from authoritative named entities, active threads, and lesson metadata. Their normalized terms enter a separate capped BM25F lane with concept, similarity, source-kind, and field attribution. Missing or stale vocabulary fails open without weakening exact BM25F. The substrate can use another compatible Ollama or OpenAI-style embedding endpoint when indexing and recall share the same vector space.

The AKASHA profile adds:

- `remember`, `recall`, `sleep`, and `wake`;
- memories and paper boats scoped to rooms;
- coding, project, writing, design, and audio lesson stores;
- entity, date, thread, taxonomy, relationship, and cluster retrieval;
- provenance and authority state;
- correction through supersession;
- archival without silent historical deletion;
- vector rebuilds and substrate health checks.

AKASHA also supports optional GIGA cognitive workers. Hippocampus Stage 1 logs
exact events before asynchronous local classification and stores generated
candidates as non-authoritative pointers to source evidence. Review, Curios,
promotion, health, and safe queue maintenance are explicit operations.

Striatum currently uses twelve hard-coded process patterns in `hearth/src/context.rs`.
They select at most one trigger per prompt.
A match adds up to twelve process-shape coding lessons through `hearth/src/triggers.rs`.
The formatter includes complete bodies, proof patterns, and trigger fields without a size cap.
See the [2026-09-06 review](#critical-organ-review-2026-09-06).

The current slice has no semantic model, hysteresis, or lesson-set carryover.
The earlier six-lesson Nemotron/hysteresis slice was removed.
The target Striatum selects lessons from Docket facts after Docket implementation.
It starts in shadow mode.
Eligibility precedes ranking.
Degraded selection returns an attributed empty set instead of broadening the match.

Every planned packet and receipt carries a version.
The selector has no authority over priority, capabilities, or acceptance policy.
Cingulate remains planned, starting with non-blocking reminders and warnings.
Hard gates require calibrated criteria that explicitly name authoritative obligations and required proof.

The next GIGA integrity pass keeps three contexts distinct: durable evidence,
one-invocation model tokens, and loaded model residency. Every cold job starts
with fresh inference state over an explicit source snapshot. Completed
interaction anchors, deterministic overlapping evidence, reviewed precedents,
separate expected and observed outcomes, and proof receipts must stabilize
before GIGA work is distributed across models or rooms.

## Retrieval flow

Automatic per-turn retrieval merges bounded candidate streams:

```text
latest user turn
      │
      ├── pinned room context
      ├── important named entities
      ├── BM25F field-aware lexical candidates
      ├── controlled semantic-vocabulary BM25F candidates
      ├── lexical thread matches
      ├── deferred prior-turn candidates
      └── semantic memory chunks
      │
      ▼
rank → fuse → deduplicate → diversity cap → budget trim
      │
      ▼
source-cited context injected into the current turn
```

Explicit `recall` exposes broader retrieval and its evidence viewport. Retrieval returns source paths, reasons, authority state, and suppression diagnostics where available. Automatic retrieval is bounded to protect the active context window.

The injection path is fail-open: retrieval errors are logged and do not block the conversation. Room resolution itself fails closed so one room never silently borrows another room's context.

Read [`RETRIEVAL.md`](./RETRIEVAL.md) for operational retrieval behavior.

### Context assembly and token budgets

The OMP adapter assembles context through distinct bounded organs:

```text
stable harness + room contract
        │
        ├── fresh-session paper boat
        ├── fresh-session Anamnesis wake counsel
        ├── routing-mode and exact keyword directives
        ├── automatic Vault / AKASHA recall evidence
        └── context-growth nudge
        │
        ▼
hidden attributed context for the active model turn
```

Approved trigger-bearing lessons enter OMP’s native TTSR manager. Other typed lessons enter only through explicit `lessons` queries.

Each organ has its own eligibility check, output cap, source attribution, and
fail-open behavior. Fresh-session surfaces are injected once. Automatic recall
can decline low-information turns; explicit `recall` exposes a broader evidence
viewport. Repeated evidence is suppressed or saturated within the session, and
stable additions are kept byte-stable where the harness can preserve provider
prefix caching.

The current adapter does **not** yet enforce one provider-tokenizer-aware
aggregate budget across identity, tool schemas, paper boats, Anamnesis, lessons,
recall, canon, thread neighbors, and directives. Independent bounds prevent one
organ from becoming unbounded, but several valid organs can still stack into a
large turn. This is a documented current limitation and a future Host-level
coordination responsibility, not a proven net-token-saving claim.

The context-growth nudge estimates fill from characters and a per-room literal
capacity (`hearth/src/context.rs`). It does not read the active model's real
limit. The [2026-09-06 review](#critical-organ-review-2026-09-06) recommends
model-aware capacity as the first orientation outcome.

## Authority and correction

House distinguishes a stored event from what currently holds authority.

A new state claim may supersede an older state claim while preserving the old row as history. Ordinary retrieval strongly demotes superseded rows and excludes archived rows. Deliberate historical queries can still include them.

Canon assertions are injected separately from ordinary memory context. Where canon and a retrieved interpretation conflict, canon wins for generation.

Corporate or project source authority remains a separate domain. An imported source document can remain the factual authority while House memories and embeddings locate it. Import profiles must preserve source class, path, version, scope, and precedence rather than flattening every document into generic memory.

Authority is domain-specific rather than one universal row ladder:

| Evidence class | What it may govern | What it may not do |
|---|---|---|
| Live enforced repository evidence and declared external project sources | Their named implementation or business domain | Become room identity or personal canon merely because they were indexed |
| Canon assertions | Load-bearing identity, relationship, naming, and project assertions in their declared scope | Rewrite external source facts outside that scope |
| Current typed project/design records and lessons | The project, design system, register, or craft scope they explicitly name | Gain broader scope through semantic similarity |
| Active memories | Events and current continuity claims until corrected, superseded, or archived | Outrank conflicting canon or a declared external factual authority |
| Anamnesis counsel | Suggest a previously lived path worth considering | Assert that the same pattern is happening now |
| GIGA candidates, embeddings, clusters, and lexical expansion | Navigate toward possible evidence or review work | Promote themselves, become facts, or authorize an action |

This is why Prism-like ledgers, curated Libraries, live repositories, or
corporate systems do not need to be copied into generic memories. Vault can
search them directly. When an AKASHA import profile indexes them, it must retain
stable source identity, state, version, evidence anchors, and precedence.
Athanor retrieval then leads the model to the governing claim or document
without taking its authority away.

## Typed knowledge

The Athanor uses separate stores because different knowledge requires different retrieval and authority rules:

- memories record things that happened;
- coding lessons record transferable engineering rules;
- project lessons record project-bound rules and constraints;
- writing lessons record prose and voice craft;
- design lessons record reusable design-system taste bound to a named design
  system and its catalogue entries;
- audio lessons record reusable audio-pipeline rules;
- Cabinet entries preserve bounded counsel and lived cycles.

Read [`LESSONS.md`](./LESSONS.md) for the lesson contracts.

## Worker routing

The core defines deterministic worker lanes and produces validated task packets. It does not import OMP, call tools, spawn agents, or resolve providers.

Current lanes are:

- `smol-scout` for bounded read-only terrain mapping;
- `smol-executor` for narrow exact edits;
- `tester` for explicit behavioral contracts;
- `verifier` for independent checks.

Dispatch takes exactly one selector — a lane or a familiar — through one unified contract; the familiar-only entry point is an alias over the same path. Accepted receipts expose `spawnPacket.args` shaped directly for the harness task call, and harness adapters spawn explicitly with that packet. Runtime models come from the agent definitions themselves; per-dispatch model override is unsupported. This keeps routing policy testable and the core independent from one harness runtime.

That paragraph describes the current dispatch contract. The planned invocation
router adds a separate `ModelSelector`, execution target, and session lifecycle
above adapter-specific spawning. Model choice will remain independent from
identity: changing provider or model cannot rename a spirit, grant room
authority, or silently reuse conversational state.

### Familiar spellbooks

Familiars are room-owned identities bound to existing worker lanes; they do not add a second routing system. A room stores the canonical registry at `familiars/spellbook.json`. Adapters also accept `familiars/litters.json` as a filename alias.

The spellbook keeps generic code vocabulary (`collective: "familiars"`) while exposing room language through `collectiveAliases`, such as `kittens`. Each familiar has a stable id, display name, aliases, description, and one lane. Optional `ompAgent` and `modelRole` bindings route that familiar to an exact discovered harness agent and its configured model alias instead of merely relabelling the lane's generic worker. Both bindings must be present together. Legacy entries without them remain readable and fall back to the lane route with an explicit receipt warning; this is transition behavior, not a named-familiar success condition. The core resolves the identity and delegates packet shaping to the same unified dispatch contract. Harness adapters still spawn explicitly.

## Extension direction

New harnesses implement adapters over the same core contracts. Organizational
deployments add access control, source connectors, and import profiles above the
substrate.

Extension order and release gates are owned by
[`roadmap.md`](./roadmap.md); this document does not restate them. What matters
architecturally is the invariant: every extension, before and after `1.0.0`,
lands on the same core contracts, and none becomes a parallel authority path.

Read [`RUNTIME_ARCHITECTURE.md`](./RUNTIME_ARCHITECTURE.md) for runtime order,
[`SYNTHESIS_ARCHITECTURE.md`](./SYNTHESIS_ARCHITECTURE.md) for proof/synthesis,
[`GODOT_CLIENT.md`](./GODOT_CLIENT.md) for the parked presentation specification,
[`COMPANION_ECOSYSTEM.md`](./COMPANION_ECOSYSTEM.md) for sovereignty and
marketplace, and [`roadmap.md`](./roadmap.md) for release gates.
