# The Athanor Roadmap

_Last updated: 2026-08-09_

This page is the current release path. It does not duplicate completed history or
turn accepted architecture documents into one giant checklist.

- Current product truth: [`../README.md`](../README.md)
- Current contracts and ownership: [`ARCHITECTURE.md`](./ARCHITECTURE.md)
- Accepted runtime direction: [`RUNTIME_ARCHITECTURE.md`](./RUNTIME_ARCHITECTURE.md)
- Complete feature/status map: [`PLANNED_FEATURES.md`](./PLANNED_FEATURES.md)
- Dated implementation history: [`history/`](./history/)

The previous long-form roadmap is preserved as
[`history/2026-08-06-roadmap-snapshot.md`](./history/2026-08-06-roadmap-snapshot.md).

## Current release state

The reference House is on the `0.11.0` late-beta line. One repository and one
release own core, substrate, the OMP adapter, the installer, the updater,
workflows, and canonical docs. Windows x64 with OMP is the only supported
release target. Read
[the canonical component table](./ARCHITECTURE.md#repository-layout-and-component-ownership).

Operational now:

- Vault and AKASHA storage profiles;
- native attributed Vault retrieval over Markdown, JSON, JSONL, and text;
- AKASHA PostgreSQL authority, pgvector, local embeddings, hybrid retrieval,
  typed memories and lessons, provenance, supersession, and lifecycle state;
- automatic and explicit recall with field-aware BM25F and bounded evidence;
- deterministic worker lanes and room-owned familiar spellbooks;
- Anamnesis reviewed counsel;
- GIGA Hippocampus Stage 1 and the first Striatum coding/project slice;
- one unified release with the Vault and AKASHA Windows x64 archives, the
  guided installer, the updater, health reporting, and release manifests;
- the localhost administrative GUI;
- accepted Host and Godot-client contracts; the native client stays planned.

The remaining `1.0.0` problem is product legibility and release proof, not a new
memory substrate.

## 1.0 dependency path

### 1. Lock the Athanor Host boundary

The Host is the single control boundary between clients and the current core,
adapters, Vault, AKASHA, and GIGA. Before the native client becomes authoritative,
its contracts must stay explicit:

- room/session state;
- chat and invocation lifecycle;
- recall evidence and authority state;
- health and profile state;
- GIGA candidate review;
- Striatum activation state;
- observable delivery and failure receipts.

The Godot client must not connect directly to PostgreSQL, NATS, model providers,
or harness internals.

### 2. Finish the thin Godot client

The first client is an instrument, not the final ornamental world.

It must make the current system operable and inspectable:

- choose and enter a room;
- see the active operator, spirit, storage profile, and health;
- hold a real conversation through the Host;
- inspect attributed recall evidence;
- review GIGA candidates without promoting them implicitly;
- see failures and degraded dependencies instead of silent fallback;
- preserve the visual-token and projection contracts in
  [`GODOT_CLIENT.md`](./GODOT_CLIENT.md).

The GPU-particle constellation, companion bodies, and broader in-world Control
UI remain accepted direction, not requirements for the first usable client.

### 3. Prove installation and continuity on the release shape

Before the `1.0.0` marker:

- build the release artifacts from a clean checkout;
- install Vault on a clean supported Windows x64 environment with OMP;
- upgrade Vault to AKASHA without losing room files or authoritative memory;
- verify restart continuity, recall, remember, sleep, and wake;
- verify a configured-but-unhealthy substrate reports degraded AKASHA rather
  than pretending to be Vault or healthy AKASHA;
- verify backup and restore against the release artifacts;
- record exact supported versions and known platform limits.

### 4. Publish evidence that matches the claims

The exact-title pilot is useful but narrow. The release evidence should cover:

- Vault exact and paraphrased local-file retrieval;
- AKASHA paraphrase, entity, date, and thread recall;
- correction, supersession, and archival authority;
- cross-room isolation;
- bounded context and attributed selection reasons;
- clean-machine installation and restart continuity;
- stated latency and corpus size on named hardware;
- final-answer grounding against retrieved evidence.

Private memory payloads never become public fixtures merely to improve a score.
See [`EVIDENCE.md`](./EVIDENCE.md).

### 5. Cut the 1.0 documentation and release surface

The release surface must agree on names and status:

- The Athanor is the product;
- House is one operator-owned continuity domain;
- Vault is the local transparent file profile;
- AKASHA is the PostgreSQL and semantic hybrid profile;
- GIGA is an optional cognitive capability above AKASHA;
- OMEGA and ANON remain planned profiles, not current release claims.

The root README stays the public current-state spine. INSTALL and USAGE own the
operator path. Architecture documents own contracts. Historical snapshots stay
in `docs/history/` and do not masquerade as current maintainer state.

## Accepted work after the first client

These threads have accepted contracts but are not all `1.0.0` blockers:

1. strengthen GIGA integrity with fresh per-job inference, deterministic
   overlapping evidence, reviewed precedents, and baseline-aware refinement;
2. define Origami crease patterns, Cranes, room-scoped Pawprints, and the Paper
   Boat wake contract over the shared Host command/event envelope;
3. prove one PostgreSQL-outbox/NATS JetStream mailbox, then the `boat.ready`
   wake path in which NATS carries a signed pointer and PostgreSQL keeps the
   complete living Boat;
4. route model bodies independently from identity into cold workers, familiars,
   reflections, or intentional live dialogue;
5. add bounded Prolog/Datalog derivation and complete Cingulate;
6. branch suitable obligations into deterministic checks, bounded synthesis,
   optional Z3, or selected Lean proofs, including production-bound Origami
   lifecycle invariants after the first lesson proof;
7. build the Hallway and resumable Vault-to-AKASHA import with full retrieval
   citizenship;
8. grow the in-world Control UI, memory constellation, companion ecosystem,
   OMEGA governance, and ANON execution from their accepted specifications.

## Release rule

Do not advance the version because a document sounds finished.

Advance it when the named behavior runs through the release artifact, survives a
restart, exposes its evidence, and matches the public claim.
