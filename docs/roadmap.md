# The Athanor Roadmap

_Last updated: 2026-08-10_

This page is the current release path. It does not duplicate completed history or
turn accepted architecture documents into one giant checklist.

- Current product truth: [`../README.md`](../README.md)
- Current contracts and ownership: [`ARCHITECTURE.md`](./ARCHITECTURE.md)
- Long-range runtime contracts: [`RUNTIME_ARCHITECTURE.md`](./RUNTIME_ARCHITECTURE.md)
- Complete feature/status map: [`PLANNED_FEATURES.md`](./PLANNED_FEATURES.md)
- Dated implementation history: [`history/`](./history/)

The previous long-form roadmap is preserved as
[`history/2026-08-06-roadmap-snapshot.md`](./history/2026-08-06-roadmap-snapshot.md).

## Current release state

The repository now carries the installed `1.0.0-rc.2` native Windows x64 candidate.
OMP is the supported harness. One Rust workspace owns the behavioral core,
Vault, AKASHA, Host, delivery, native lifecycle, and Godot client. Read
[the canonical component table](./ARCHITECTURE.md#repository-layout-and-component-ownership).

Implemented in the candidate:

- file-authoritative Rust Vault and PostgreSQL-authoritative Rust AKASHA;
- typed canon, memory, lessons, GIGA, Recall Policy, and Paper Boat sleep/wake;
- authenticated Host snapshots, typed deltas, resync, persistence, and restart
  recovery;
- transaction-coupled `boat.ready` outbox delivery through NATS JetStream;
- retained sanitized receipt replay after Host restart;
- live Godot Recall Policy and Paper Boat receipt screens;
- native Windows service lifecycle, immutable versions, backup, rollback,
  doctor, uninstall, and explicit purge;
- one checksum-pinned payload carrying Godot 4.7.1, PostgreSQL 18.4-2,
  pgvector 0.8.6, and NATS 2.14.4.

RC2 passed ordinary suites, isolated PostgreSQL/NATS integrations, live Godot
rendering, 20,645-artifact manifest verification, packaged-client smoke, Inno
Setup compilation, and an elevated external-authority installation on the real
Solarisael workstation. The installed service runs NATS, delivery, and separate
Kintsu/Kodo Hosts while reusing the existing PostgreSQL authority. Final
`1.0.0` remains gated on a clean generic managed install, real legacy upgrade,
signing, and the broader public evaluations in [`EVIDENCE.md`](./EVIDENCE.md).

The dependency path below records the completed implementation order and the
still-open final release gates. Where the phase table in
`RUNTIME_ARCHITECTURE.md` differs, this roadmap is authoritative.


## 1.0 dependency path

### 1. Freeze the accepted boundary

The `1.0.0` program includes Rust convergence, the narrow NATS lane, existing
fixes, hardening, the usable GUI, installation, migration, and release evidence.

Do not add Prolog/Datalog, Lean, Z3, SyGuS, marketplace behavior, new cognitive
organs, distributed-worker expansion beyond the proved NATS lane, companion
bodies, the GPU-particle constellation, or broader in-world surfaces.

Before implementation, keep [`../LESSON_MAP.md`](../LESSON_MAP.md), this
roadmap, [`ARCHITECTURE.md`](./ARCHITECTURE.md),
[`RUNTIME_ARCHITECTURE.md`](./RUNTIME_ARCHITECTURE.md),
[`GODOT_CLIENT.md`](./GODOT_CLIENT.md), and
[`EVIDENCE.md`](./EVIDENCE.md) aligned with the same owners and gates.

### 2. Capture the 0.11 parity baseline and close known fixes

The current `0.11.0` runtime is the observed parity reference, not the target
topology. Inventory each TypeScript, Python, and Rust capability with its owner,
callers, tests, persistence effects, failure behavior, and migration surface.

Record:

- exact, paraphrase, entity, date, and thread retrieval results;
- correction, supersession, archival recovery, and room-isolation behavior;
- p50 and p95 latency at named corpus sizes;
- clean-install prerequisites and operator steps;
- restart, backup, restore, update, and rollback behavior;
- paired task-efficiency results where a stable rubric exists.

Close the already-planned correctness, lifecycle, authority, and visible GUI
defects before using the baseline as migration proof. A green test suite does
not replace running the affected production-shaped path.

### 3. Lock one Rust domain, Host, and profile contract

`house-core` becomes the single behavioral authority. Define one common envelope
for identity, House and room scope, authority, lifecycle, provenance, chronology,
and relationships, with typed payloads for memories, canon, lessons, counsel,
candidates, and other distinct records. Do not flatten typed constraints into an
unvalidated generic document.

Vault and AKASHA execute the same domain commands and return the same observable
receipts. One conformance corpus proves:

- scope and authorization before ranking;
- candidate, accepted, superseded, archived, and historical state transitions;
- provenance and source identity;
- continuation and supersession relationships;
- failed-command atomicity;
- bounded attributed retrieval.

The Host is the only client control boundary. Rust owns validation, policy,
idempotency, reconciliation, ranking, storage behavior, and versioned protocol
schemas. The OMP adapter may remain TypeScript where the harness requires it,
but only as generated registration, lifecycle translation, transport, and
bounded presentation skin.

### 4. Converge Vault and AKASHA on the Rust core

Vault remains portable, file-authoritative, single-writer, and
database-service-free. Its structured records carry the same domain semantics as
AKASHA. Its lexical and optional local semantic indexes are rebuildable and
never authoritative.

AKASHA remains the primary installed profile: PostgreSQL-authoritative,
transactional, concurrent, and continuously indexed. Preserve the current live
schema and data while establishing parity; do not combine the language cutover
with an unnecessary database redesign.

Move behavior from Python and TypeScript into Rust one complete vertical path at
a time: migrations, health, backup and restore, imports, memory and lesson
operations, retrieval, ranking, embeddings, GIGA work, and maintenance. For each
path:

1. recover the current contract, owner, callers, and tests;
2. implement and exercise the Rust path through the real boundary;
3. prove Vault/AKASHA parity where shared;
4. migrate every caller and sweep for orphans;
5. delete the displaced behavioral owner.

Vault-to-AKASHA migration is an explicit one-way authority handoff. The profiles
never accept independent authoritative writes and reconcile later.

### 5. Prove the PostgreSQL-outbox/NATS spine

PostgreSQL owns truth. NATS owns delivery and wake-up only. A transactional
outbox publishes authoritative record IDs with bounded routing and integrity
metadata; consumers reload exact PostgreSQL records.

The first production lane must prove:

- commit and publish ordering;
- at-least-once delivery with durable idempotency;
- an explicit JetStream duplicate window;
- duplicate, stale, expired, malformed, and unauthorized pointer rejection;
- consumer restart, redelivery, backoff, dead-letter, and recovery;
- no private memory or conversation body in broker payloads;
- observable Host and GUI delivery receipts.

Vault does not require NATS. The lane advances only if it replaces more bespoke
queue, polling, supervision, and failure machinery than it adds. Do not widen
the broker until this one lane passes its complete gate.

### 6. Finish the usable thin Godot GUI

The first GUI is an instrument built from ordinary Godot `Control` nodes. It
consumes authenticated Host commands, snapshots, deltas, replay, and
resynchronization. It never connects directly to PostgreSQL, NATS, model
providers, or harness internals.

The 1.0 client gate is deliberately narrow:

- authenticate to the loopback Host without learning database or broker
  credentials;
- apply a real Recall Policy snapshot and typed updates;
- expose the requested/resolved modes and explicit operator override;
- render pending, delivered, refused, and degraded Paper Boat receipt states;
- replay a retained sanitized receipt after Host restart;
- never load or invent a Boat body/title.

Both implemented screens were exercised in the real scene and inspected at the
rendered surface. Conversation composition, source inspection, GIGA review,
companion bodies, spatial Hallway presentation, and the memory constellation
remain outside `1.0.0`.

### 7. Make installation and lifecycle boring

Remove Athanor-owned Python, WSL, external embedding-service, and Bun
prerequisites. OMP may retain its own harness runtime; The Athanor has one
behavioral Rust runtime.

The ordinary AKASHA installer provisions:

- the signed native Rust service and CLI;
- managed private PostgreSQL, its database and role, and required extensions;
- deterministic state, log, migration, and backup locations;
- room bootstrap, startup registration, health checks, and rollback metadata.

Advanced operators may select an external compatible PostgreSQL instance.

Prove clean Vault and AKASHA installation, Vault-to-AKASHA migration, ordinary
restart, graceful generation replacement, failed replacement, update, backup,
restore, and rollback. A candidate generation becomes active only after protocol,
schema, migration, and health readiness; the previous healthy binary remains
available until the new generation drains real work successfully.

### 8. Publish evidence and cut 1.0

The release evidence compares both profiles and the pre-cutover runtime:

- the same semantic conformance corpus against Vault and AKASHA;
- Vault exact and paraphrased local-file retrieval;
- AKASHA paraphrase, entity, date, and thread recall;
- correction, supersession, archival authority, and room isolation;
- Vault-to-AKASHA migration with record, relationship, source, and authority
  checks;
- clean native installation, restart, live replacement, failed replacement,
  backup, restore, and rollback;
- p50 and p95 latency with corpus, index, embedding, and hardware details;
- bounded context, attributed selection reasons, and Recall Policy behavior;
- final-answer grounding and paired end-to-end task efficiency;
- the NATS lane's delivery, privacy, idempotency, and recovery receipts;
- rendered GUI operation and degraded-state visibility.

AKASHA's additional machinery must measurably outperform Vault where the product
claims that it does. Private memory payloads never become public fixtures merely
to improve a score. See [`EVIDENCE.md`](./EVIDENCE.md).

Before the `1.0.0` marker, build every artifact from a clean checkout; install
both profiles on clean supported Windows x64 environments; and make README,
INSTALL, USAGE, architecture, evidence, lesson map, and release claims agree.

The release vocabulary remains:

- The Athanor is the product;
- House is one operator-owned continuity domain;
- Vault is the portable transparent file profile;
- AKASHA is the installed PostgreSQL and semantic hybrid profile;
- GIGA is an optional cognitive capability above the shared Rust domain core;
- NATS is AKASHA delivery infrastructure, never authority;
- OMEGA and ANON remain planned profiles, not current release claims.

## Deferred work after 1.0

These accepted threads cannot interrupt the release path:

1. strengthen GIGA beyond required 1.0 integrity with broader refinement
   transactions and additional workers;
2. expand Origami, Cranes, room-scoped Pawprints, and Paper Boat delivery beyond
   the single proved lane;
3. route model bodies into broader cold-worker, familiar, reflection, and live
   dialogue topologies;
4. add bounded Prolog/Datalog derivation and complete Cingulate;
5. branch suitable obligations into deterministic synthesis, optional Z3, or
   selected Lean proofs;
6. build the spatial Hallway, GPU memory constellation, companion ecosystem,
   OMEGA governance, and ANON execution.

## Release rule

Do not advance the version because a document sounds finished.

Advance it when the named behavior runs through the release artifact, survives a
restart, exposes its evidence, and matches the public claim.
