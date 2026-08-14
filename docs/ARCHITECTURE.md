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

The Athanor Host is the authenticated versioned boundary between interactive
clients and the Rust runtime. The thin Godot client uses only Host WebSocket
commands and events. It does not connect directly to PostgreSQL, NATS, model
endpoints, or harness internals.

```text
Godot client ───────┐
                    ├── Athanor Host ── Rust contracts ── Vault / AKASHA
OMP adapter ─────────┘                         │
                                              └── PostgreSQL outbox
                                                          │
                                                          └── NATS JetStream
```

PostgreSQL remains authoritative. The outbox records durable publication intent
in the same transaction as the domain event. NATS JetStream may deliver opaque
record IDs and wake consumers, but it never becomes memory, review state, or a
second conversation ledger.

Paper Boat sleep commits its continuity row and `boat.ready` outbox event in one
PostgreSQL transaction. NATS carries only bounded pointer and sanitized receipt
projections; wake reloads the complete Boat from PostgreSQL authority. Cranes,
general Origami folds, and Pawprints beyond this implemented lane remain
specified extensions of the same envelope rather than current release claims.

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
| `crates/house-core`, `crates/house-protocol` | Provider-neutral domain, authority, and wire contracts |
| `crates/house-vault` | Strict database-free, file-authoritative Vault retrieval |
| `crates/house-substrate`, `substrate/` | PostgreSQL-authoritative AKASHA operations, migrations, retrieval, typed stores, GIGA, health, backup, and restore |
| `crates/house-host` | Authenticated snapshots, typed deltas, resync, Recall Policy, and receipt projection |
| `crates/house-delivery` | Transactional outbox publication and bounded JetStream consumption |
| `crates/athanor-install`, `installer/` | Native service lifecycle, immutable staging, rollback, doctor, and Windows installer |
| `gui/` | Thin Godot operator client |
| `adapters/omp/` | OMP entrypoint, room integration, named tools, and Rust transport |
| `.github/workflows/` | Continuous integration and native release assembly |
| `docs/`, root Markdown | Canonical documentation |

One repository does not merge the internal authority boundaries. The core does
not import the OMP adapter. The core does not require PostgreSQL. The Vault
profile runs without any substrate component. Each boundary stays enforced by
contract, not by repository distance.

The public API boundaries are `hostApi=1`, `substrateApi=1`, `deliveryApi=1`,
and `godotApi=4.7`.

### Installed layout

Immutable product versions and mutable operator data are separate:

| Installed path | Content |
|---|---|
| `%ProgramFiles%\Solarisael\Athanor\bin` | Native lifecycle manager and stable OMP loader |
| `%ProgramFiles%\Solarisael\Athanor\versions\<version>` | Verified immutable runtime, OMP adapter, Godot client, PostgreSQL, and NATS |
| `%ProgramData%\Solarisael\Athanor\config` | Non-secret database mode and exact House room/port topology |
| `%ProgramData%\Solarisael\Athanor\secrets` | ACL-restricted service secrets |
| `%ProgramData%\Solarisael\Athanor\data` | Managed PostgreSQL and NATS durable data |
| `%ProgramData%\Solarisael\Athanor\state\host\<room>` | Isolated Host projection state per room |
| `%ProgramData%\Solarisael\Athanor\backups` | Upgrade, rollback, external-authority, and legacy pre-install backups |
| `%USERPROFILE%\.omp\agent\athanor\client.json` | ACL-restricted per-user Host token and room endpoint projection |

`current.json` atomically selects the active immutable version. The
`SolarisaelAthanor` Windows service starts only the exact children under that
version: optional managed PostgreSQL, one NATS broker, one delivery worker, and
one identity-bound Host per configured room. It reports `RUNNING` only after
every child passes readiness and drains the runtime plan in reverse order.

OMP registers one stable loader under Program Files. The loader follows
`current.json`, reads the invoking user's restricted client projection, sets
Host/substrate paths and credentials only in process memory, then loads the
active adapter and hygiene extension exactly once. Room-bound commands choose
their endpoint by exact room key; missing entries fail closed rather than route
to another room. Development checkouts may still set explicit topology
overrides, but install removes duplicate source/version registration owners.

### Release and support target

`0.9.6` is the current native Windows x64 late-beta source version. OMP is the
supported harness. The release artifact shape is one checksum-published installer:

```text
The-Athanor-<version>-windows-x64.exe
The-Athanor-<version>-windows-x64.exe.sha256
```

The payload pins PostgreSQL 18.4-2, pgvector 0.8.6, NATS 2.14.4, and Godot
4.7.1. The ordinary managed install needs no WSL, Python, Bun, Cargo, Godot
editor, or separately installed database/broker. An explicit advanced mode may
bind an operator-provided compatible PostgreSQL database.

The OpenCode adapter line and the two portable Vault/AKASHA archives are
historical. Vault and AKASHA are runtime authority profiles inside one release,
not separate packages.

## Room model

A room is a writable directory with one stable lowercase key. Display names may change without changing the room key.

The Vault room contract includes:

- `.solarisael-room.json` for machine-readable room identity;
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
defaults to the room directory; `.solarisael-room.json` may name one or more
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
Striatum's first operational slice keeps up to six coding or exact-project
lessons active across an observed project work state. Scope, project, type,
declared stage, and register eligibility precede Nemotron similarity; hysteresis
prevents small prose changes from churning the set, while an explicitly declared
phase replaces prior phases and abrupt topic changes refresh it. Cingulate remains
planned for divergence detection.

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
        ├── Striatum lessons or deterministic process lessons
        ├── automatic Vault / AKASHA recall evidence
        └── context-growth nudge
        │
        ▼
hidden attributed context for the active model turn
```

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

The spellbook keeps generic code vocabulary (`collective: "familiars"`) while exposing room language through `collectiveAliases`, such as `kittens`. Each familiar has a stable id, display name, aliases, description, and one lane. The core resolves that identity and delegates packet shaping to the same unified dispatch contract. Harness adapters still spawn explicitly.

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
[`GODOT_CLIENT.md`](./GODOT_CLIENT.md) for presentation,
[`COMPANION_ECOSYSTEM.md`](./COMPANION_ECOSYSTEM.md) for sovereignty and
marketplace, and [`roadmap.md`](./roadmap.md) for release gates.
