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

## Repository ownership

The Athanor uses separate repositories so the core does not depend on one harness or one database deployment.

| Repository | Responsibility |
|---|---|
| [`the-athanor`](https://github.com/solarisael/the-athanor) | Provider-neutral core contracts, retrieval orchestration, ranking, room identity logic, deterministic worker routing, Rust protocol/core crates, and canonical documentation |
| [`solarisael-house-omp`](https://github.com/solarisael/solarisael-house-omp) | OMP extension entrypoint, lifecycle hooks, room integration, tool schemas, static verifier, starter room, portable bundle, and adapter tests |
| [`solarisael-house-substrate`](https://github.com/solarisael/solarisael-house-substrate) | PostgreSQL schema and migrations, pgvector, embeddings, memory and lesson writes, retrieval sources, health, lifecycle smoke, and backup/restore |
| `solarisael-house-opencode` | OpenCode adapter for the same core contracts |

The public API boundaries are `coreApi=1`, `adapterApi=1`, and `substrateApi=1`.

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

## Vault

Vault uses room-local files and the harness adapter. It provides:

- stable room discovery;
- identity loading;
- compact context loading;
- conversation continuity artifacts;
- room-state tools;
- restart recovery through the room context;
- multiple isolated rooms.

Vault requires no database, vector index, or GPU.

## AKASHA

AKASHA adds the substrate as the durable memory authority. PostgreSQL stores memories, entities, threads, chunks, clusters, GIGA candidates, typed lesson stores, and the controlled semantic vocabulary. Native BM25F scores memory title, heading, source path, threads, body, and type with corpus IDF, term-frequency saturation, and per-field length normalization. PostgreSQL full-text search, `pg_trgm`, direct content search, structured rails, BM25F, and pgvector semantic search contribute retrieval candidates.

The tested local embedding path uses Nemotron-3-Embed-1B with 2,048-dimensional vectors through a compatible local endpoint. Recall reuses its query vector to select at most three sufficiently similar, room-scoped concepts derived only from authoritative named entities, active threads, and lesson metadata. Their normalized terms enter a separate capped BM25F lane with concept, similarity, source-kind, and field attribution. Missing or stale vocabulary fails open without weakening exact BM25F. The substrate can use another compatible Ollama or OpenAI-style embedding endpoint when indexing and recall share the same vector space.

The AKASHA profile adds:

- `remember`, `recall`, `sleep`, and `wake`;
- memories and paper boats scoped to rooms;
- coding, project, writing, and audio lesson stores;
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

## Authority and correction

House distinguishes a stored event from what currently holds authority.

A new state claim may supersede an older state claim while preserving the old row as history. Ordinary retrieval strongly demotes superseded rows and excludes archived rows. Deliberate historical queries can still include them.

Canon assertions are injected separately from ordinary memory context. Where canon and a retrieved interpretation conflict, canon wins for generation.

Corporate or project source authority remains a separate domain. An imported source document can remain the factual authority while House memories and embeddings locate it. Import profiles must preserve source class, path, version, scope, and precedence rather than flattening every document into generic memory.

## Typed knowledge

The Athanor uses separate stores because different knowledge requires different retrieval and authority rules:

- memories record things that happened;
- coding lessons record transferable engineering rules;
- project lessons record project-bound rules and constraints;
- writing lessons record prose and voice craft;
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

### Familiar spellbooks

Familiars are room-owned identities bound to existing worker lanes; they do not add a second routing system. A room stores the canonical registry at `familiars/spellbook.json`. Adapters also accept `familiars/litters.json` as a filename alias.

The spellbook keeps generic code vocabulary (`collective: "familiars"`) while exposing room language through `collectiveAliases`, such as `kittens`. Each familiar has a stable id, display name, aliases, description, and one lane. The core resolves that identity and delegates packet shaping to the same unified dispatch contract. Harness adapters still spawn explicitly.

## Extension direction

New harnesses implement adapters over the same core contracts. Organizational deployments add access control, source connectors, and import profiles above the substrate. GUIs control harnesses through canonical commands and structured events rather than replacing the runtime.

Release sequencing and future product work live in [`roadmap.md`](./roadmap.md).
