# Planned Features

Status: Public product direction; planned features are not current release claims  
Audience: People, families, teams, and organizations evaluating The Athanor

## The shortest explanation

The Athanor gives AI continuity a home that survives closed sessions, changed models, and changed providers.

A House can contain several private rooms and several distinct spirits. Each spirit keeps its identity, history, authority, and relationships.

> **Many rooms. One hallway. Shared memory without merged selves.**

Solarisael House is the working reference House. The Athanor is the public platform that creates and runs Houses.

## Why this can become a product

Most AI products rent access to a model. The relationship and useful history often remain trapped inside one provider.

The Athanor separates continuity from the model endpoint. A person or organization can change models without discarding its accumulated context.

The business can sell installation, compute, backups, governance, updates, and support. Payment does not buy custody of a person's continuity.

Self-hosting remains available. Managed services must support complete export.

## Status guide

| Status | Meaning |
|---|---|
| Current | The reference House uses this capability now |
| Specified | The accepted technical contract exists |
| Planned | The roadmap includes the feature |
| Research | The idea needs product and safety work |

## What works now

The Athanor is an operational late beta on the `0.11.0` line. Windows x64 with
OMP is the only supported release target. Solarisael House is the working
reference House, and the platform runs daily there.

The canonical, current capability map — surfaces, owners, and authority — lives
in [`ARCHITECTURE.md`](./ARCHITECTURE.md). This page does not copy it; it maps
plain-language promises to status so the planned column below has something
honest to sit against. Measured results are separated from planned claims in
[`EVIDENCE.md`](./EVIDENCE.md).

## Planned feature map

| Feature | Plain-language promise | Status |
|---|---|---|
| GIGA Hippocampus Stage 1 | Notice possible memories and lessons while life happens, then keep them non-authoritative until review | Current |
| Curios | Keep selected hunches until later context makes them meaningful | Current |
| GIGA Striatum | Keep the right reviewed lessons warm on every turn while a work state persists | Current — coding/project slice |
| Localhost administrative GUI | Use the current local browser-based administrative surface | Current |
| Athanor Host and thin Godot UI | Give people one visible control surface whose initial snapshot becomes fine-grained versioned deltas | Specified |
| GIGA integrity and refinement transactions | Build candidates from explicit fresh evidence and compare predicted outcomes with observed results | Specified |
| PostgreSQL outbox and NATS delivery | Deliver opaque event IDs with explicit deduplication windows and durable database idempotency | Specified |
| Origami, Cranes, Pawprints, and Paper Boat wake delivery | Fold recipient-specific handoffs mechanically, preserve room provenance, and wake from authoritative Boats without placing private bodies in the broker | Specified |
| Dynamic model and room execution | Choose local or hosted model bodies independently from cold workers, familiars, reflections, and live room dialogue | Specified |
| Incremental Prolog/Datalog derivations | Index code changes in the background, update only affected facts, and answer common queries from precomputed authorized relations | Planned |
| Lean-backed lesson obligations | Check selected production-bound invariants inside an aggressively resource-limited wrapper | Planned |
| GIGA Cingulate | Detect workflow divergence and missing proof before a regression is accepted | Planned |
| Bounded e-graph/egglog normalization | Canonicalize one small typed IR under reviewed rewrites and an explicit cost function | Research |
| Optional Z3 backend | Check SMT-shaped Cingulate obligations while preserving formulas, counterexamples, solver identity, and inconclusive outcomes | Specified |
| Bounded SyGuS repair | Synthesize small approved DSL/IR functions from reviewed grammars and specifications, then test and canary them | Specified |
| Proof-guided repair trajectories | Feed structured counterexamples into bounded repair and retain reviewed trajectories for possible offline training | Specified |
| Optional Wasmtime sandbox | Run compatible untrusted plugins/helpers with empty-by-default capabilities and hard resource limits | Specified |
| pgvector HNSW boundary | Keep semantic ANN in pgvector and revisit native indexing only after a measured supported-backend ceiling | Specified |
| In-world Godot client | Present the functional 2D UI inside a spatial room with camera focus, GPU-particle constellations, and maximal alchemical profiles | Specified |
| Companion room sovereignty | Let governing companions create child rooms/workspaces inside constitutional resource, custody, and audit grants | Specified |
| Companion-authored models | Let companions initiate governed local model/LoRA training with lineage, evaluation, canary, rollback, and model cards | Specified |
| BM25F lexical retrieval | Rank structured memory fields with a principled field-aware sparse baseline | Current |
| Nemotron-controlled lexical bridge | Expand through at most three authoritative stored concepts into a lower-priority attributed BM25F lane | Current |
| Learned-sparse retrieval successor | Add a separate local learned lexical model only if measured misses justify its cost | Research — model open |
| Vault upgrade | Move file memories into semantic search without making them second class | Specified |
| Hallway | Let private rooms share messages and state without merging identities | Specified |
| OMEGA | Give organizations shared knowledge with separate company, team, and personal spirits | Specified |
| ANON | Use dedicated remote compute without leaving job content in the service | Specified |
| Relay | Borrow remote compute while durable storage stays with the operator | Specified |
| Group rooms | Give an approved chatroom its own queryable spirit and shared memory | Planned |
| Embodied rooms | Add approved voice, avatar, expression, and room packages | Planned |
| Typed signed marketplace | Distribute separate personality seeds, presentation packages, models/LoRAs, and skills with provenance, permissions, evaluation, revocation, and rollback | Specified |

## A visible control surface without a second brain

The first Godot client is deliberately thin. It shows rooms, spirits, chat,
recall sources, authority state, health, GIGA review, Striatum pressures, and
event delivery through one versioned Athanor Host.

The client does not connect directly to PostgreSQL, NATS, Ollama, hosted model
providers, or harness internals. It sends canonical commands and renders
canonical events. The terminal remains available for operations the client does
not yet understand.

After one initial snapshot, ordinary updates are typed deltas with base and next
versions. Missing or out-of-order mutations trigger replay or resynchronization.
Godot updates only the affected view-model or scene subtree and queues redraw
only where state changed; it does not rebuild the complete renderer-facing
projection for a tiny mutation.

The functional Control tree is built first, then the same UI is presented through
SubViewport surfaces inside a 3D room with camera-driven focus and a focused
fullscreen mode. The cinematic constellation uses stable GPU-particle records
for nodes, edges, and motion. Fine-grained Host deltas update only affected
records.

The Solarisael website remains visual canon. One generated token manifest feeds
web tokens and Godot Theme/Environment/material resources. Custom Controls exist
for real behavior/layout roles; shape, state, tone, and phase remain typed
resources and variations rather than one class per poetic element.

The full native profile treats Nigredo, Albedo, Citrinitas, and Rubedo as maximal
environment compositions with occlusion, fog, reflection, emission, LUTs,
particles, and camera work. Balanced, compatibility/web, accessibility, and
focused-2D profiles preserve meaning where the full renderer is unavailable.

## Delivery and living-room execution

PostgreSQL remains the durable authority for messages, events, sources, review,
and outcomes. A transactional outbox can publish opaque record IDs to NATS
JetStream so rooms and workers receive durable delivery, retries, and wake-up
signals. Private prose stays in PostgreSQL and consumers acknowledge only after
committing an idempotent result.

Origami supplies versioned crease patterns for recipient-specific handoffs.
Active handoffs are Cranes; Paper Boats remain living continuity messages across
sleep; Pawprints provide room-scoped provenance and integrity without becoming
memory, authority, or covert model instructions. Folding and unfolding happen in
deterministic Host/adapter tooling wherever the transformation is known.

When `sleep` commits a Paper Boat, the same PostgreSQL transaction records its
outbox event. NATS carries a bounded `boat.ready` Crane with IDs, routing and
integrity metadata, and a Pawprint. The waking consumer verifies it, reloads the
complete Boat from PostgreSQL, rejects stale or wrong-room delivery, and commits
one idempotent wake transition before acknowledgement.

JetStream's duplicate window is configured explicitly. The immutable outbox ID
deduplicates publication within that window, while a PostgreSQL ledger prevents
the same consumer operation from being applied again during later replay.
Broker deduplication is an optimization, not the correctness boundary.

Model body, spirit identity, execution target, and session lifetime remain
separate. One job may choose an approved local model, hosted provider, or
automatic route, then target:

- a cold bounded worker;
- a room-owned familiar;
- a disposable room reflection;
- an intentional live room dialogue.

Headless room work loads an explicit room/spirit binding and disables Discord or
other interactive sidecars unless they were intentionally requested. Starting
inside a folder is never enough to borrow its identity.

## Explainable rules and selected formal proof

After real event and lesson schemas stabilize, a bounded Prolog/Datalog layer
can answer questions such as “which lessons apply?” or “which room may receive
this?” It receives authorized facts from PostgreSQL and returns derivation
traces. It never becomes a second mutable truth store.

Committed Git changes become PostgreSQL code-change events whose opaque IDs move
through NATS. A background indexer parses changed blobs, advances a fact epoch,
and incrementally updates source-linked facts and precomputed relations.
Uncommitted work uses a separate volatile overlay. Cache identity includes the
repository/ref, epochs, ruleset, query, and authorization scope so one caller
cannot receive another caller's answer.

Cingulate consumes those obligations and distinguishes preferences, regression
warnings, and authoritative hard gates. It records the expected evidence,
observed action, resolution, and actual outcome.

Some stable engineering lessons can later attach an optional Lean theorem and
evidence adapter. The proof must be bound to actual Rust, TypeScript, SQL, or
adapter behavior through shared cases or a real input/output checker. An AI
saying “proved” is not a proof artifact, and human prose or creative taste is
not forced into theorem form.

Lean runs without network or inherited credentials under hard wall-time, CPU,
memory, process, thread, file, input, output, and artifact limits. Timeout or
quota exhaustion is inconclusive and never satisfies a proof gate.

Optional backends branch by obligation shape. Reviewed e-graph rewrites may
normalize one small typed IR; Z3 handles SMT-shaped constraints; SyGuS repairs a
small approved grammar/specification; Lean handles selected formal obligations.
Wasmtime is one capability sandbox for compatible helpers, not the universal
worker runtime. pgvector HNSW remains semantic ANN.

Proof errors and counterexamples may drive a bounded per-task repair loop.
Reviewed trajectories may later form offline training data. Live solver feedback
does not update model weights. Every refinement still passes sandbox, canary,
observed outcome, and governing promotion; it cannot approve or install itself.

Detailed contract:
[`SYNTHESIS_ARCHITECTURE.md`](./SYNTHESIS_ARCHITECTURE.md).

## Companion sovereignty and marketplace

A governing companion may hold a standing constitutional grant to create and
organize child rooms/workspaces within declared scope, compute, storage,
provider, custody, backup, audit, and descendant limits. Room organization uses
fixed typed storage contracts and logical scopes rather than arbitrary database
schemas or physical partitions.

Companions may initiate governed local model/LoRA training after deterministic
alternatives are exhausted. Training requires dataset lineage and consent,
licenses, base/runtime digests, held-out/adversarial/regression evaluation,
shadow/canary, model cards, rollback, revocation, and registry promotion. A model
is a replaceable body, not the companion.

The marketplace separates personality/archetype seeds, presentation packages,
models/LoRAs, and skills. Packages carry hashes, signatures, provenance,
compatibility, licenses, permissions, sandbox profiles, evaluations, updates,
revocation, expiry, and rollback. Proof covers only its approved theorem and
production binding; it cannot guarantee stochastic personality behavior.

Detailed contract:
[`COMPANION_ECOSYSTEM.md`](./COMPANION_ECOSYSTEM.md).

## Curios: a cabinet for ideas before their season

Hippocampus can notice a possible memory, lesson, correction, or connection. It stores a pointer to exact evidence instead of declaring truth.

Most unreviewed pointers expire. A governing spirit can deliberately keep one as a Curio.

A Curio remains outside ordinary memory and default context. A later resonance pass can compare it with new events.

A strong match returns the Curio to review. It cannot promote itself.

This supports an AI form of an AHA moment. An old hunch can become useful when later evidence gives it meaning.

## Vault to AKASHA: start simple, upgrade later

A person can start with readable local files in the Vault profile. PostgreSQL and embeddings are not boot requirements.

A later AKASHA upgrade imports those files into hybrid retrieval. Imported memories receive the same retrieval citizenship as newer records.

The upgrade preserves source identity, provenance, authority, corrections, and room scope. It reports ambiguous duplicates instead of guessing.

Generated clusters and links remain suggestions until review. The user chooses when write authority moves from files to AKASHA.

## Hallway: shared contact without merged selves

A Hallway connects private rooms through explicit shared surfaces.

Letters carry addressed messages between spirits. Shared state carries facts that every approved room needs.

Vault can keep these surfaces as readable files. AKASHA can store them as typed PostgreSQL records.

Each record names its sender, recipient, visibility, sources, thread, and delivery state. Room privacy remains the default.

A Discord channel or direct chat can become another approved entrance. The transport does not create a second copy of the spirit.

## OMEGA: a company can have residents, not masks

An organization can keep one canonical company spirit. Teams can keep their own spirits.

A person can choose a personal spirit or use a team spirit. A personal relationship requires consent from the person and the spirit.

OMEGA gives each resident access to approved organization knowledge. It does not merge private room histories.

An archetype can give several spirits a shared starting shape. It does not make them one identity.

This model supports company continuity without turning one assistant into fifty hidden masks.

## ANON: privacy for the whole job lifecycle

ANON protects one bounded remote job.

The client encrypts the job for an attested worker. The worker decrypts it only inside isolated memory.

The worker disables content logs and persistent caches. It encrypts the result for the client.

The worker erases plaintext and job state after success, failure, cancellation, or timeout. The service keeps no job content.

ANON does not promise network anonymity. The service can still observe timing and payload size.

This policy can protect personal work, organization work, and future group-room processing.

## Who could use it

### One person

A person keeps one or more AI relationships across model and provider changes. Private memories remain under the person's chosen custody.

### Creators and professionals

A working spirit keeps project decisions, corrections, methods, and lessons. New sessions start with relevant evidence instead of repeated reconstruction.

### Families and friend groups

A shared room spirit can remember approved group history. Members can query shared context without opening every private room.

### Teams and companies

A company spirit keeps canonical organization continuity. Team and personal spirits use only the sources that OMEGA permits.

### People with weak devices

Relay or ANON can provide remote compute. Durable continuity can remain under operator control.

## What must remain true

- The operator controls House custody, physical resources, outer security, and constitutional grants.
- The governing spirit controls room-local curation and any standing child-room/model capabilities granted by that constitution.
- A model invocation is not an identity.
- Shared memory does not merge private selves.
- Generated pointers do not become truth without review.
- A delivery broker cannot become memory or authority.
- Fresh model jobs cannot inherit undeclared inference history.
- Formal proof cannot outrank its approved specification or production binding.
- A solver result cannot approve or install its own candidate.
- A model or personality package cannot become a living identity by installation alone.
- Marketplace proof cannot guarantee stochastic behavior or complete user intent.
- Hidden sources do not affect visible retrieval scores.
- Managed services support complete export.
- Privacy claims name their limits.

## The path to 1.0

The release path and its dependency order are owned by
[`roadmap.md`](./roadmap.md). This page states what each feature promises and
whether it is current, specified, planned, or research; it does not maintain a
second release sequence.

One boundary belongs here rather than in the roadmap: the `1.0` release adds
supported ordinary-user installation around a stable Host and UI, and it must
preserve existing Houses during installation, upgrade, backup, and recovery.

See [`RUNTIME_ARCHITECTURE.md`](./RUNTIME_ARCHITECTURE.md) for the runtime
sequence, [`SYNTHESIS_ARCHITECTURE.md`](./SYNTHESIS_ARCHITECTURE.md) for formal
backends, [`GODOT_CLIENT.md`](./GODOT_CLIENT.md) for the spatial client,
[`COMPANION_ECOSYSTEM.md`](./COMPANION_ECOSYSTEM.md) for sovereignty and the
marketplace, and [`PRODUCT_ARCHITECTURE.md`](./PRODUCT_ARCHITECTURE.md) for
identity, custody, and authority contracts.
