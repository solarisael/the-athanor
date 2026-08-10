# The Athanor Security and Privacy

The Athanor keeps continuity under the operator's control. That requires explicit boundaries around credentials, room data, model providers, retrieval scope, publication, and destructive operations.

## Trust model

A House installation includes several trust domains:

| Domain | Trust responsibility |
|---|---|
| Operator | Chooses rooms, model providers, durable memories, and consequential changes |
| Room | Holds private identity and continuity scoped to one room |
| Harness adapter | Loads room context, registers tools, logs sessions, and sends selected context to the model |
| Core | Resolves rooms, shapes retrieval, ranks evidence, and validates routing contracts |
| Substrate | Stores and retrieves durable memory and lessons |
| Model provider | Processes every prompt and retrieved excerpt sent to the active model |
| Embedding endpoint | Processes text sent for vectorization |
| Athanor Host and client | Authenticates commands, projects authorized state, and prevents UI/backend bypass |
| Delivery transport | Delivers opaque record IDs and wake-up signals without owning record truth or private prose |
| Model runtime | Executes one explicitly scoped invocation; loaded weights do not authorize hidden session reuse |
| Formal/synthesis backend | Checks only its approved encoding under explicit resources; never grants authority or promotion |
| Training pipeline | Uses consented, licensed, lineage-tracked data and cannot activate its own output |
| Marketplace | Verifies artifact provenance, signatures, permissions, compatibility, revocation, and rollback |

Local storage does not make a hosted model private. Review the provider's data handling before sending private room material.

## Secrets

Never store these in a room, memory, lesson, paper boat, repository, or public evaluation artifact:

- API keys;
- access tokens;
- database passwords;
- private keys;
- session cookies;
- recovery codes;
- raw credential exports.

Use environment variables, platform credential stores, or a dedicated secret manager. Installation receipts name configuration paths and enabled features, never secret values.

## Installation changes

The installing agent must explain and receive the operator's choice before it:

- requests elevation;
- enables WSL or virtualization;
- installs global software;
- changes a system service;
- deletes or overwrites data;
- moves an existing room;
- changes model-provider authentication;
- imports a private archive;
- enables a shared or remote substrate.

Configuration edits are additive. Existing rooms, extensions, and unrelated settings remain intact.

## Room isolation

Rooms are private scopes by default.

The adapter and core resolve the explicit active room before loading identity or memory. A missing or invalid room does not silently fall back to another room's private context.

Cross-room retrieval is deliberate. An operator or authorized runtime must name the other room or exact memory address. Shared lessons and explicitly shared House scopes remain separate from private room memory.

## Planned delivery and embodiment boundaries

The accepted runtime evolution adds PostgreSQL outbox delivery, NATS JetStream,
dynamic model selection, and headless room execution. These features are not
current release claims.

The publishing transaction writes the authoritative event and outbox row
together. NATS carries opaque PostgreSQL record IDs plus bounded routing and
integrity metadata. It must not carry raw turns, memory or lesson bodies,
identity prose, or credentials. Consumers authenticate scope, reload the record
from PostgreSQL, commit an idempotent result, and only then acknowledge delivery.

NATS accounts, subjects, and consumer permissions must prevent broad
cross-House or cross-room subscription. Filtering after receipt is too late.

JetStream duplicate tracking is a bounded performance aid. Streams declare the
window explicitly, while PostgreSQL keeps consumer-operation idempotency through
the maximum retention and replay horizon. A replay outside the broker window
must still be harmless. Event, operation, and delivery identities remain
distinct.

Dynamic execution binds model selector, execution target, session lifecycle,
room, spirit, operator, and allowed side effects independently. A model,
process, working directory, or target label cannot grant room authority.
Headless room work disables interactive transports and sidecars unless policy
explicitly enables them. `room_reflection` must not silently consume a live
dialogue tail; `room_dialogue` must visibly address an intentional live session.

The Host sends Godot versioned typed deltas with bounded mutation counts and
payload sizes. Clients reject gaps, out-of-order versions, unknown operations,
and unauthorized projection fields, then request replay or a fresh snapshot.
Do not expose arbitrary database or object-property paths through a generic
patch operation.

Code indexing treats Git as authority. NATS receives only a PostgreSQL
`code_change` ID, never source or diff text. Precomputed facts and cache entries
remain partitioned by House, project, repository/ref, fact epoch, worktree
overlay, ruleset/schema, and authorization digest. Authorization filtering
happens before fact projection and cache lookup.

Lean proof checking runs as an unprivileged, networkless child with allowlisted
binary/module digests, read-only input, isolated temporary output, no inherited
credentials, and hard CPU, wall-time, memory, process, thread, file, input,
output, and artifact limits. Timeout or exhaustion kills the process tree and
cannot satisfy a proof gate.

Read [`RUNTIME_ARCHITECTURE.md`](./RUNTIME_ARCHITECTURE.md) for the full planned
contract.

## Planned synthesis and self-improvement boundaries

E-graph rewrites, SyGuS grammars/specifications, Z3 translators/formulas, Lean
modules/evidence adapters, and their cost/resource profiles are separate trusted
inputs. A generated candidate cannot change the artifact that judges it.

Every backend preserves exact input/source refs, implementation and translation
digests, checker version, resource profile, and explicit status. `unknown`,
timeout, quota exhaustion, unsupported input, and translation failure are
inconclusive.

Wasmtime starts with no host capabilities. A module receives only reviewed
imports, WASI preopens, clocks/randomness, network policy, fuel/epoch budget,
memory/table/instance limits, and bounded input/output named by its manifest.
WASM isolation does not make a dangerous host import safe.

Proof/counterexample feedback may guide bounded repair. It does not update live
model weights. Reviewed trajectories enter a later offline dataset only after
secret/privacy/license review and immutable split assignment.

No proposal self-approves, self-publishes, or self-installs. Promotion follows
separate sandbox, canary, observed-outcome, authority, and rollback records.

Read [`SYNTHESIS_ARCHITECTURE.md`](./SYNTHESIS_ARCHITECTURE.md).

## Planned companion and marketplace boundaries

A companion's standing constitutional grant is scope- and resource-bounded.
Child-room creation enforces visibility, descendant depth/count, storage,
compute, provider, data-egress, backup, retention, and audit policy. Ordinary
room authority cannot create physical database schemas, indexes, or partitions.

Companion-authored training requires dataset lineage, participant consent,
redaction, license, immutable splits, contamination checks, base/tokenizer/runtime
digests, adversarial/regression evaluation, shadow/canary, and a rollback target.
Model weights cannot activate themselves.

Marketplace artifact classes remain separate. Personality seeds are not living
spirits; models are not identities; presentation packages are not tools.
Executable content remains inert until signature, hash, provenance,
compatibility, capability diff, sandbox, evaluation, revocation, and local
activation checks pass.

Signed proof files do not make a package zero-trust. Local verification rejects
unapproved axioms, `sorry`, imports, stale bindings, checker mismatches, and
resource failures, but still proves only the encoded theorem. Stochastic model
or personality behavior requires tuple-specific evaluation.

Marketplace metadata uses expiry, threshold signing where appropriate, key
rotation, rollback/freeze protection, and revocation. An update cannot widen
permissions silently.

The Godot marketplace/client is a presentation and consent surface. A cinematic
particle, companion body, or alchemical state cannot represent authority unless
it follows an authenticated Host event.

Read [`COMPANION_ECOSYSTEM.md`](./COMPANION_ECOSYSTEM.md) and
[`GODOT_CLIENT.md`](./GODOT_CLIENT.md).

## Organizational authorization

A central company House must filter by authorization before semantic, lexical, or structured ranking. Filtering retrieved results after ranking is insufficient because candidate generation and diagnostics can already expose private existence or metadata.

Enterprise deployments require:

- tenant isolation;
- team and project membership;
- private-user scopes;
- least-privilege service accounts;
- auditable administrative changes;
- source-specific access control;
- retention and deletion policy;
- encrypted transport and protected backups.

The current personal-room deployment is not a substitute for those controls.

## Model and embedding providers

The active model provider receives the context that the adapter sends in a prompt, including selected memory excerpts. Local embeddings prevent archive text from being sent to a hosted embedding service, but they do not change the active model's provider boundary.

When using a hosted embedding endpoint, treat every embedded document as data sent to that provider.

Provider portability protects continuity from one product surface. It does not override provider terms, logging, policy, rate limits, or outages.

Automatic model routing may select only operator-approved endpoints that satisfy
the invocation's capability and privacy class. A fallback may reduce capability
or fail clearly; it cannot widen data custody silently.

Fresh jobs receive explicit evidence snapshots. Provider session IDs, KV caches,
or retained model processes must not smuggle previous inference history into a
new cold job.

## Memory and lesson writes

Durable writes are deliberate by default. Before recording sensitive personal or company material, consider:

- whether the detail is necessary for continuity;
- which room or project owns it;
- whether it should be shared;
- whether a source document is a better authority;
- how it can be corrected, superseded, archived, exported, or deleted.

A memory records an event or realization. A lesson records a reusable rule. Do not convert an entire private transcript into lessons automatically.

## Destructive operations

Destructive lesson deletion requires the exact numeric ID and current title. Broad deletion is not supported by the guarded tool.

Room and memory deletion require explicit operator intent and a clear statement of the affected scope. Preserve the room by default when removing an adapter or bundle.

Back up AKASHA before migrations, bulk imports, retention changes, or destructive maintenance. The substrate component owns the canonical backup and restore procedures.

## Public evidence and demonstrations

Public artifacts contain sanitized aggregates and synthetic fixtures.

Never publish:

- raw private turns;
- memory titles;
- source paths;
- retrieved excerpts;
- people or entity names;
- room or thread identifiers;
- private retrieval diagnostics;
- screenshots containing account names, home paths, notifications, or credentials.

Public demonstrations use a sterile demo House with synthetic rooms, projects, decisions, and memories. Record from a clean account or environment. Post-production redaction is defense in depth, not the primary privacy boundary.

## Vulnerability reports

Report security defects privately to the repository owner before publishing exploit details or private-data exposure. Include the affected component, the release version, and the API version. Add a minimal synthetic reproduction, the expected boundary, the observed result, and whether private data was exposed.

Do not attach real room data to a report.
