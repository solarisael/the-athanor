# The Athanor Boundaries

This document records current support boundaries and non-goals. The product README states what The Athanor does. This document tells operators where adaptation or additional engineering is still required.

## Supported installation path

Windows 11 x64 with OMP is the only supported late-beta target.
`0.9.6` is the source version label carried by this documentation. It is a
labeled historical snapshot. The root `package.json` declares the current
product version. The installed immutable manifest declares the installed build;
dated evidence in [`EVIDENCE.md`](./EVIDENCE.md) and [`../BUGS.md`](../BUGS.md)
names `0.5.4+dev.…` builds installed on 2026-09-05. The earlier `0.9.6.1`
activation and RC artifact labels remain immutable build identities and
evidence, not current installed state.

The ordinary managed install requires:

- Administrator elevation;
- the checksum-published native installer;
- a supported OMP installation and its model/provider authentication;
- sufficient storage for the bundled PostgreSQL, NATS, parked Godot client, immutable
  release versions, database, and backups.

The installer carries parked Godot 4.7.1, PostgreSQL 18.4-2, pgvector 0.8.6, NATS
2.14.4, and every Athanor Rust binary. It does not require WSL, Python, Bun,
Cargo, a Rust toolchain, a Godot editor (parked client), or a separately installed database or
broker.

The web prototype at `gui-prototype/` is the read-only operator surface.
It requires Bun today.
Run `bun gui-prototype/serve.ts` from the repository root.
It reads the Host through a loopback proxy.

Vault remains a database-free runtime profile. AKASHA uses managed PostgreSQL by
default. Existing Houses must use explicit external-database mode when their
authoritative PostgreSQL endpoint already owns the configured port. That mode
takes a first-install backup, starts no PostgreSQL child, and still requires the
release's current migration schema plus `vector`, `pg_trgm`, and `pgcrypto`.
The 2026-09-05 installed build ran schema 30 (`BUGS.md:106`); the earlier
schema 17 requirement is historical.

Local semantic embeddings still require a compatible configured embedding
endpoint. No GPU or embedding model is bundled in the current late beta.

One repository and one release own every installed component. Read
[the canonical component table](./ARCHITECTURE.md#repository-layout-and-component-ownership).

## Other hosts

| Host | Current state |
|---|---|
| Windows 11 x64 + OMP | Supported late-beta target |
| Windows 10 x64 | Installer target but not locally re-proved for the current source |
| Native Linux | Rust components are portable in principle; installer, service, parked Godot package, and OMP integration require host-specific engineering and verification |
| OpenCode | Historical adapter line; unsupported |
| macOS | Unsupported |
| Other harnesses | Require an adapter over the Rust contracts |

An adapted path becomes trustworthy when it proves the same observable contracts: adapter loading, room discovery, `room_state`, fresh-session continuity, and—when AKASHA is selected—a real substrate write/read lifecycle.

## Installation boundary

The native installer manages one immutable-version topology. A bounded first
install backs up only the named legacy 0.10.x product trees; it never executes
legacy Python, WSL, Bun, or shell behavior as a fallback. The operator still
needs a working OMP installation and its provider authentication before the AI
can use the adapter. Read [`../INSTALL.md`](../INSTALL.md) for managed/external
database modes, readiness, rollback, uninstall, and explicit purge.

The installer is locally built and payload-verified. A clean generic
managed-database installation remains a public evidence gap, not a completed
claim.

## Known late-beta blockers

The earlier blocker record names a `remember` failure on a valid `continues` edge.
It reports a bogus `params.room` validation error.
This review examines no edge-specific installed proof.
Generic `remember` successes do not resolve that recorded defect.
Keep its repair status open until the affected edge is exercised.

The live `sleep` path has a dated success receipt.
On 2026-09-05, the installed OMP tool wrote paper boat #4473 with `backup.status: ok`.
Its backup took 49.0 seconds.
The backup for `remember` #4472 took 47.2 seconds.
See `BUGS.md:105` and [`EVIDENCE.md`](./EVIDENCE.md).
The PostgreSQL commit precedes the full post-write dump.
The remaining sleep and wake gaps are:

- the wake presentation keeps rendered `wake_context`, title, source, and id,
  and drops the separate boat age and warning fields;
- the [generated-turn adapter repair](./EVIDENCE.md#generated-turn-presence-repair-2026-09-07) is installed, with isolated component proof;
  real restart, chat, and root Knock turns now have live incoming Presence observations;
- Host-side Presence points still require correct session attribution (`BUGS.md:25-30`);
- requested backups wait for a dump that excludes `insula`; `remember` defaults to no backup, while `sleep` keeps backups enabled.

The operator GUI remains read-only and incomplete. The web prototype allowlists
POST-only `/live/*` read routes for health, Insula, Docket, Hallway, memory,
and lesson reads (`gui-prototype/serve.ts`). It does not yet provide the
agent, message, authority, work, and failure views required for ordinary
operation without terminal archaeology.

## Review-derived boundaries — 2026-09-06

Sol accepted a critical organ review on 2026-09-06. The dated census lives in
[`ARCHITECTURE.md`](./ARCHITECTURE.md#critical-organ-review-2026-09-06). The
bounded facts below are current boundaries, not defects to be inferred beyond
their evidence.

- The context-growth nudge derives capacity from the room key
  (`crates/hearth/src/context.rs:737-764`): 1,000,000 tokens for `kodo`,
  400,000 otherwise. It is an assumption about the model, not a measured limit.
- A matched process trigger emits up to twelve coding lessons with complete
  bodies, proof, and trigger fields (`crates/hearth/src/triggers.rs`). No size
  cap applies.
- Wake metadata carries `created_at` and warnings in the substrate
  (`crates/origami/src/boats/wake.rs:21-32`). The OMP presentation drops those
  separate fields. Age-aware orientation is a recommendation.
- The Anamnesis wake selector loads pillars and active cycles by kind and update
  time without a cycle recency gate (`crates/akasha/src/anamnesis.rs:362-377`).
  An active cycle that loads is not thereby relevant to the live turn.
- Requested post-write backups use a full dump.
  The recorded backups took about 47–49 seconds.
  Commit and backup remain distinct outcomes.
- The reviewed GIGA aggregate reported enabled capture and classification, a healthy store, and an empty queue.
  It listed only dismissed candidate states.
  These fields do not prove classifier reachability, useful review, or later benefit.
- Docket settlement is fenced by room (`crates/akasha/src/docket/report/mod.rs:130-147`).
  A single-room House needs an explicit independent reviewer or operator
  arrangement. This is an authority boundary, not an exploit.
- Workspace search `0.1.1` indexes only explicitly requested roots and has no
  watcher. It is perception over a consented workspace, not AKASHA memory.
- This review performs no fresh restore or complete custody certification.
  Complete export, restore, and operator retention/deletion journeys require their own evidence.

## Retrieval boundary

House retrieves bounded evidence; it does not load an entire archive into every prompt.

Automatic retrieval is intentionally narrower than explicit `recall`. Low-information turns may retrieve nothing. Explicit recall remains available for deliberate archive investigation.

Semantic proximity is a candidate signal, not factual authority. Important answers should follow the cited source and its authority state. Imported corporate or project documents require an explicit source-precedence policy.

Retrieval is fail-open for conversation continuity. If PostgreSQL or embeddings are unavailable, the adapter keeps lighter room continuity usable and reports the degraded source rather than blocking the turn.

## Context-budget boundary

The current OMP adapter bounds each context organ independently. Room context,
tool schemas, a fresh paper boat, Anamnesis wake counsel, active lessons,
automatic recall, canon, thread neighbors, directives, and context-growth
nudges each have their own eligibility and output rules.

There is not yet one provider-tokenizer-aware coordinator that assigns a single
turn budget across all of them. Several individually valid organs can therefore
stack into a context surface that is larger than a short task warrants. Prefix
caching can reduce billed cache-write cost for stable prefixes, but cached
tokens still occupy model context and still depend on provider behavior.

The Athanor has not yet publicly established that retrieval and continuity
reduce total input tokens or total task cost against a no-Athanor baseline.
Long-running work can plausibly avoid repeated explanation, searching, mistakes,
and rediscovery; short isolated tasks may consume more input context. Treat net
efficiency as an evaluation question, not a product claim.

## Memory boundary

The Athanor is not indiscriminate transcript storage. Durable memory remains deliberate by default.

- Events and realizations belong in memories.
- Transferable engineering rules belong in coding lessons.
- Project-bound rules belong in project lessons.
- Current state can supersede older current state.
- Narrative history remains recoverable.
- Secrets belong in a secret manager, never memory.

The Athanor can preserve a wrong interpretation if an operator or agent deliberately records it. Correction and supersession make the trail repairable; they do not eliminate the need for judgment.

## Identity boundary

House preserves and loads an identity contract. It does not prove metaphysical identity, consciousness, or equivalence between different model providers.

A room can keep names, voice, commitments, corrections, and shared history available across model changes. Different models may still express the same contract with different capability, style, or reliability.

Identity prose is co-authored. The installer does not manufacture intimacy, relationship claims, or a personality on the operator's behalf.

A personality/archetype package is reusable starting material, not a packaged
living companion. A model or LoRA is a replaceable body, not proof of identity.
Installing either cannot import a relationship or overwrite an existing spirit
lineage.

## Provider boundary

A local House does not make the model provider local. Any context sent to a hosted model can be processed under that provider's terms.

Local embeddings keep archive vectorization off a hosted embedding service. They do not prevent selected memory context from reaching the active model provider.

The Athanor keeps continuity provider-portable, but it cannot remove provider-side rate limits, model policies, outages, or capability differences.

## Runtime-evolution boundary

The current release line does not ship background code-change indexing,
incremental Prolog/Datalog facts and precomputed relations, invocation-time model
routing, headless room targets, complete Cingulate, e-graph/egglog
normalization, Z3, SyGuS, Wasmtime sandbox profiles, proof-guided repair, the
resource-bounded Lean checker, in-world SubViewport presentation, the
GPU-particle constellation, companion room sovereignty, companion-authored model
training, or the signed marketplace.

The authenticated Host, typed snapshot/delta/resync path, Recall Policy, narrow
PostgreSQL-outbox/NATS Paper Boat lane, restart replay, and native lifecycle are current.
The Godot screens are parked and retain historical proofs.
NATS remains delivery-only and never becomes memory authority.

The broader capabilities have accepted dependency and technical contracts in
[`RUNTIME_ARCHITECTURE.md`](./RUNTIME_ARCHITECTURE.md),
[`SYNTHESIS_ARCHITECTURE.md`](./SYNTHESIS_ARCHITECTURE.md),
[`GODOT_CLIENT.md`](./GODOT_CLIENT.md) (parked historical specification), and
[`COMPANION_ECOSYSTEM.md`](./COMPANION_ECOSYSTEM.md). Documentation labels them
as specified, planned, or research until observable implementation gates pass.

Worker lanes still obtain their runtime models from harness agent definitions;
per-dispatch model override remains unsupported. A model process kept warm is
not a persistent room.

The current personal House has no online training service, companion model
registry, package signature/revocation service, marketplace, autonomous child
room creation, or constitutional resource scheduler.

## Organizational boundary

The current room model is not yet a complete enterprise authorization system.

A central multi-user deployment requires:

- tenant, team, project, and private-user scopes;
- authorization filtering before relevance ranking;
- source provenance and versioning;
- retention and deletion policy;
- auditability;
- administrative controls;
- tested connectors for corporate sources.

Do not place an entire company's private corpus behind shared retrieval until those controls exist and have been verified.

## Non-goals

The Athanor does not replace:

- Git for source-code history, branches, review, and merges;
- a secret manager for credentials;
- object storage for large binary artifacts;
- human judgment over consequential memories and lessons;
- the AI harness that executes models and tools;
- specialized knowledge interfaces such as Obsidian.

House coordinates continuity and retrieval across those systems.

## Planned boundary changes

The release path is maintained in [`roadmap.md`](./roadmap.md). Planned work is
kept explicitly separate from current release claims in the root README and
every architecture document.
