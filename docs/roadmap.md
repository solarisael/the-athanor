# The Athanor Roadmap

_Last updated: 2026-08-17_

This page is the current release path. It does not duplicate completed history or
turn accepted architecture documents into one giant checklist.

- Current product truth: [`../README.md`](../README.md)
- Current contracts and ownership: [`ARCHITECTURE.md`](./ARCHITECTURE.md)
- Long-range runtime contracts: [`RUNTIME_ARCHITECTURE.md`](./RUNTIME_ARCHITECTURE.md)
- Complete feature/status map: [`PLANNED_FEATURES.md`](./PLANNED_FEATURES.md)
- Dated implementation history: [`history/`](./history/)

The previous long-form roadmap is preserved as
[`history/2026-08-06-roadmap-snapshot.md`](./history/2026-08-06-roadmap-snapshot.md).

## Current late-beta state

The repository now carries the `0.9.6` native Windows x64 late-beta source.
OMP is the supported harness. One Rust workspace owns the behavioral core,
Vault, AKASHA, Host, delivery, native lifecycle, and parked Godot client. Read
[the canonical component table](./ARCHITECTURE.md#repository-layout-and-component-ownership).

Implemented in the candidate:

- file-authoritative Rust Vault and PostgreSQL-authoritative Rust AKASHA;
- typed canon, memory, lessons, GIGA, Recall Policy, and Paper Boat sleep/wake;
- PostgreSQL-authoritative Hallway membership, daily threads,
  recipient-sensitive posts, room-stable unread state, durable Bell rows,
  exact-thread reads, Host-owned automatic inbox projection, and explicit
  recipient-authorized bounded Knocks; ordinary Hallway contact remains manual;
- authenticated Host snapshots, typed deltas, resync, persistence, and restart
  recovery;
- transaction-coupled `boat.ready` outbox delivery through NATS JetStream;
- retained sanitized receipt replay after Host restart;
- a compiled addressed Crane subject/envelope and generic transport-receipt
  path, with no production addressed producer or recipient application handler
  yet;
- historical Godot Recall Policy and Paper Boat receipt screens;
- native Windows service lifecycle, immutable versions, backup, rollback,
  doctor, uninstall, and explicit purge;
- one checksum-pinned payload carrying parked Godot 4.7.1, PostgreSQL 18.4-2,
  pgvector 0.8.6, and NATS 2.14.4.

RC3 passed ordinary suites, isolated PostgreSQL/NATS integrations, historical Godot
rendering, 20,659-artifact manifest verification, packaged-client smoke, Inno
Setup compilation, and an elevated external-authority installation on the real
Solarisael workstation. The installed service runs NATS, delivery, and separate
Kintsu/Kodo Hosts while reusing the existing PostgreSQL authority.

Current NATS traffic is narrower than the surrounding House surfaces. It does
not carry Hallway posts, GIGA jobs, kitten lifecycle, project records, or live
conversation. The current Paper Boat receipt proves transport validation; it
does not prove room wake, model consumption, or human reading.

The installed artifact still identifies as `1.0.0-rc.3`; that immutable label is
retained as historical evidence, not current product maturity. Final `1.0.0`
remains gated on the complete operator GUI, healthy continuity organs, a clean
generic managed install, real legacy upgrade and rollback, signing, and the
broader public evaluations in [`EVIDENCE.md`](./EVIDENCE.md).

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
[`GODOT_CLIENT.md`](./GODOT_CLIENT.md) (parked historical specification), and
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

`hearth` becomes the single behavioral authority. Define one common envelope
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

For `1.0.0`, this proof remains bounded to the existing `boat.ready` production
lane. The structural addressed subject does not count as recipient delivery.
Generalized authority references, recipient application handlers, and private
cross-room subjects belong to the post-1.0 communication spine. Broker
credentials and subject ACLs are mandatory before that expansion.

### 6. Complete the web operator surface

The web prototype at `gui-prototype/` is the read-only operator surface.
Run `bun gui-prototype/serve.ts` from the repository root.
It reads the Host through a loopback proxy.
The Godot client is parked.
The parked native specification describes authenticated Host commands, snapshots, deltas, replay, and resynchronization.
It prohibits direct connections to PostgreSQL, NATS, model providers, or harness internals.
Its Recall Policy, sanitized Paper Boat receipt, and worker-lane screens remain historical evidence.

The 1.0 operator gate must make the state of a House legible:

- rooms, spirits, sessions, active model bodies, identity bindings, and health;
- messages among the operator, room agents, familiars, subagents, and other
  authorized agents, with direction, lifecycle, delivery, and failure state;
- Recall Policy, the active working set, selected sources, attribution,
  selection reasons, freshness, degradation, and useful retrieval metrics;
- memory, canon, lesson, and GIGA candidate authority and review state;
- agent, familiar, and subagent dispatch lineage, current work, completion,
  refusal, failure, and durable output;
- Host, substrate, PostgreSQL, NATS, queue, delivery, backup, migration, and
  version state at the level an operator can act on;
- clear attention signals that distinguish healthy background activity from
  pending decisions and failures.

Every view consumes authoritative Host projections and links summary metrics to
inspectable attributed records. The GUI must not infer a second truth from
renderer state, expose private message bodies across unauthorized scopes, or
make raw telemetry the only explanation.

Conversation composition, source inspection, GIGA review, dispatch and quest
lineage, House and agent observability, and operational metrics therefore
remain before `1.0.0`.

The 1.0 conversation and observability surface may use the existing
Host/harness/PostgreSQL paths. It does not require putting Hallway, project,
kitten, or live-token traffic through NATS.

Companion bodies, spatial Hallway presentation, and the memory constellation
remain later work and do not block 1.0.

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

These accepted threads cannot interrupt the release path.

### House communication spine first

Before broader Origami, independent workers, or dynamic model routing, implement
the accepted contract in
[`RUNTIME_ARCHITECTURE.md`](./RUNTIME_ARCHITECTURE.md#7-house-communication-spine-postgresql-crane-nats-and-host)
in this dependency order:

1. harden the current lane with service credentials and subject ACLs, complete
   stream/consumer readiness, and truthful transport-receipt names;
2. replace the memory-only Crane reference with a typed authority reference;
3. add crease handlers and PostgreSQL application receipts distinct from
   outbox, transport, and read state;
4. prove recipient-specific consumers, dead-letter/replay operation, and NATS
   reconstruction from PostgreSQL;
5. replace the delivered PostgreSQL/Host-polled Hallway Knock with
   recipient-scoped NATS wake hints only after steps 1–4, preserving messages,
   Bell rows, Host authorization, exact reads, and recipient wake policy;
6. create project identity, membership, subscriptions, and typed project records
   before adding project notifications;
7. add addressed kitten work only for a demonstrated independent/dormant-worker
   need, with durable capability/workspace contracts and coalesced progress;
8. announce committed conversation turns only to asynchronous subscribers while
   keeping live commands and streaming on Host/WebSocket;
9. add GIGA wake hints only if multiple dormant workers justify them; SQL remains
   the claim authority.

This is one communication spine, not one generic message table. PostgreSQL owns
records, permissions, idempotency, and application receipts. NATS carries
bounded pointers. Host authenticates, applies, and projects. Project is scope;
Hallway is a social log; kitten is an actor; Crane is a delivery intent.

### Later accepted threads

1. strengthen GIGA beyond required 1.0 integrity with broader refinement
   transactions and additional workers;
2. expand Origami, room-scoped Pawprints, Paper Boat application, and Crane
   delivery only through the proved communication spine;
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
