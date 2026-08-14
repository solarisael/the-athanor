# The Athanor Boundaries

This document records current support boundaries and non-goals. The product README states what The Athanor does. This document tells operators where adaptation or additional engineering is still required.

## Supported installation path

Windows 11 x64 with OMP is the only supported late-beta target.
`0.9.6` is the current source version. The reference workstation runs a locally
proven native `0.9.6.1` activation; historical RC artifact labels remain only as
immutable build identities and evidence.

The ordinary managed install requires:

- Administrator elevation;
- the checksum-published native installer;
- a supported OMP installation and its model/provider authentication;
- sufficient storage for the bundled PostgreSQL, NATS, Godot client, immutable
  release versions, database, and backups.

The installer carries Godot 4.7.1, PostgreSQL 18.4-2, pgvector 0.8.6, NATS
2.14.4, and every Athanor Rust binary. It does not require WSL, Python, Bun,
Cargo, a Rust toolchain, a Godot editor, or a separately installed database or
broker.

Vault remains a database-free runtime profile. AKASHA uses managed PostgreSQL by
default. Existing Houses must use explicit external-database mode when their
authoritative PostgreSQL endpoint already owns the configured port. RC2 takes a
first-install backup, starts no PostgreSQL child in that mode, and still requires
schema 16 plus `vector`, `pg_trgm`, and `pgcrypto`.

Local semantic embeddings still require a compatible configured embedding
endpoint. No GPU or embedding model is bundled in the current late beta.

One repository and one release own every installed component. Read
[the canonical component table](./ARCHITECTURE.md#repository-layout-and-component-ownership).

## Other hosts

| Host | Current state |
|---|---|
| Windows 11 x64 + OMP | Supported late-beta target |
| Windows 10 x64 | Installer target but not locally re-proved for the current source |
| Native Linux | Rust components are portable in principle; installer, service, Godot package, and OMP integration require host-specific engineering and verification |
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

The reference House has reproduced two live continuity-path defects that must
be repaired before 1.0:

- `remember` rejects a valid `continues` edge with a bogus `params.room`
  validation error;
- the live `sleep` tool path is not healthy, despite the implemented and tested
  Rust Paper Boat transaction architecture.

The operator GUI also remains incomplete. It does not yet provide the House,
agent, message, Recall, authority, work, health, and failure views required for
ordinary operation without terminal archaeology.

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
PostgreSQL-outbox/NATS Paper Boat lane, restart replay, native lifecycle, and
functional 2D Godot screens are current. NATS remains delivery-only and never
becomes memory authority.

The broader capabilities have accepted dependency and technical contracts in
[`RUNTIME_ARCHITECTURE.md`](./RUNTIME_ARCHITECTURE.md),
[`SYNTHESIS_ARCHITECTURE.md`](./SYNTHESIS_ARCHITECTURE.md),
[`GODOT_CLIENT.md`](./GODOT_CLIENT.md), and
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
