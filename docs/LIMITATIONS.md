# The Athanor Boundaries

This document records current support boundaries and non-goals. The product README states what The Athanor does. This document tells operators where adaptation or additional engineering is still required.

## Supported installation path

The guided public installation currently supports:

- Windows 10 or 11;
- OMP;
- Bun;
- the stable Rust MSVC toolchain;
- Vault room files and the TypeScript OMP lifecycle adapter over Rust core contracts.

AKASHA on the guided Windows path additionally uses:

- the release-built Windows `solarisael-house-substrate.exe`;
- WSL 2 with Ubuntu;
- PostgreSQL 16 with pgvector and `pg_trgm`;
- Python 3.11+ for migrations, health, imports, and maintenance;
- a compatible local embedding endpoint;
- approximately 10 GB of free storage for the tested setup.

The tested embedding default is Nemotron-3-Embed-1B through WSL ROCm Ollama on compatible AMD hardware. The mounted OMP memory path uses the long-lived Windows Rust process; Python support scripts are not a substitute for that runtime proof.

## Other hosts

| Host | Current state |
|---|---|
| Windows + OMP + Vault | Supported guided path |
| Windows + OMP + WSL AKASHA | Supported guided Rust-first path split between the core, OMP, and substrate repositories |
| Native Linux | Database and support tools are adaptable; the current Windows executable and OMP integration require host-specific engineering and verification |
| OpenCode | Adapter work predates the Rust cutover; it is not covered by the current guided installer |
| macOS | Unsupported by the guided installation |
| Other harnesses | Require an adapter over the core contracts |

An adapted path becomes trustworthy when it proves the same observable contracts: adapter loading, room discovery, `room_state`, fresh-session continuity, and—when AKASHA is selected—a real substrate write/read lifecycle.

## Installation boundary

Version 0.10 uses an AI-guided developer-shaped setup with a verified Rust-first runtime. The operator still needs a working harness and its authentication before the AI can take over. The 1.0 milestone adds a trusted native bootstrapper, ordinary-user onboarding, upgrades, uninstall behavior, backup, and recovery.

The current installer does not promise one-click setup on an otherwise empty machine.

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

Version 0.10.1 does not yet ship the Godot client, Athanor Host WebSocket or
delta synchronization, PostgreSQL-outbox/NATS delivery or idempotency ledger,
background code-change indexing, incremental Prolog/Datalog facts and
precomputed relations, invocation-time model routing, headless room targets,
complete Cingulate, e-graph/egglog normalization, Z3, SyGuS, Wasmtime sandbox
profiles, proof-guided repair, the resource-bounded Lean checker, in-world
SubViewport presentation, the GPU-particle constellation, companion room
sovereignty, companion-authored model training, or the signed marketplace.

These capabilities have accepted dependency and technical contracts in
[`RUNTIME_ARCHITECTURE.md`](./RUNTIME_ARCHITECTURE.md),
[`SYNTHESIS_ARCHITECTURE.md`](./SYNTHESIS_ARCHITECTURE.md),
[`GODOT_CLIENT.md`](./GODOT_CLIENT.md), and
[`COMPANION_ECOSYSTEM.md`](./COMPANION_ECOSYSTEM.md). Documentation labels them
as specified, planned, or research until observable implementation gates pass.

The current worker lanes still obtain their runtime models from harness agent
definitions; per-dispatch model override remains unsupported. The current GIGA
queue remains PostgreSQL-owned. A model process kept warm is not a persistent
room, and no current broker should be treated as memory authority.

The current personal House also has no online training service, companion model
registry, package signature/revocation service, marketplace, autonomous child
room creation, or constitutional resource scheduler. The current Godot plan is
architecture, not a rendered client or performance result.

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
