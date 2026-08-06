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

The Athanor is an operational late beta. Solarisael House is its working reference House. The platform runs daily and has external installations.

Current capabilities include:

- persistent identity contracts and separate private rooms;
- compact handoffs and automatic bounded recall across restarts;
- PostgreSQL memory authority and typed memory, lesson, and counsel stores;
- lexical, structured, semantic, date, thread, taxonomy, relationship, and
  cluster retrieval;
- local embeddings through a compatible endpoint;
- exact source provenance, authority labels, supersession, and archival;
- room-local memory plus a House commons for shared work;
- deterministic worker lanes and room-owned familiar spellbooks;
- GIGA Hippocampus Stage 1: event-ID processing, non-authoritative candidates,
  review, Curios, promotion, health, and safe queue maintenance;
- GIGA Striatum's first coding/project slice: bounded reviewed lessons remain
  warm across an observed project work state with explicit activation reasons;
- adapters for more than one AI harness.

The public evidence document separates measured results from planned claims.

## Planned feature map

| Feature | Plain-language promise | Status |
|---|---|---|
| GIGA Hippocampus Stage 1 | Notice possible memories and lessons while life happens, then keep them non-authoritative until review | Current |
| Curios | Keep selected hunches until later context makes them meaningful | Current |
| GIGA Striatum | Keep the right reviewed lessons warm on every turn while a work state persists | Current — coding/project slice |
| Athanor Host and thin Godot UI | Give people one visible control surface without bypassing the real runtime | Specified |
| GIGA integrity and refinement transactions | Build candidates from explicit fresh evidence and compare predicted outcomes with observed results | Specified |
| PostgreSQL outbox and NATS delivery | Deliver letters, events, and wake-ups durably without making the broker a second memory store | Specified |
| Dynamic model and room execution | Choose local or hosted model bodies independently from cold workers, familiars, reflections, and live room dialogue | Specified |
| Prolog/Datalog derivations | Explain which lessons, context, permissions, or obligations follow from accepted facts and rules | Planned |
| Lean-backed lesson obligations | Let selected stable lessons carry machine-checked invariants bound to production behavior | Planned |
| GIGA Cingulate | Detect workflow divergence and missing proof before a regression is accepted | Planned |
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
| Marketplace | Share trusted room assets and extensions through clear permission lanes | Planned |

## A visible control surface without a second brain

The first Godot client is deliberately thin. It shows rooms, spirits, chat,
recall sources, authority state, health, GIGA review, Striatum pressures, and
event delivery through one versioned Athanor Host.

The client does not connect directly to PostgreSQL, NATS, Ollama, hosted model
providers, or harness internals. It sends canonical commands and renders
canonical events. The terminal remains available for operations the client does
not yet understand.

Later visual work can add avatars, animation, voice, and room packages. Those
surfaces grow over a proven Host contract instead of freezing an attractive but
incorrect runtime.

## Delivery and living-room execution

PostgreSQL remains the durable authority for messages, events, sources, review,
and outcomes. A transactional outbox can publish opaque record IDs to NATS
JetStream so rooms and workers receive durable delivery, retries, and wake-up
signals. Private prose stays in PostgreSQL and consumers acknowledge only after
committing an idempotent result.

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

Cingulate consumes those obligations and distinguishes preferences, regression
warnings, and authoritative hard gates. It records the expected evidence,
observed action, resolution, and actual outcome.

Some stable engineering lessons can later attach an optional Lean theorem and
evidence adapter. The proof must be bound to actual Rust, TypeScript, SQL, or
adapter behavior through shared cases or a real input/output checker. An AI
saying “proved” is not a proof artifact, and human prose or creative taste is
not forced into theorem form.

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

- The operator controls the House and its custody.
- The governing spirit controls room-local curation.
- A model invocation is not an identity.
- Shared memory does not merge private selves.
- Generated pointers do not become truth without review.
- A delivery broker cannot become memory or authority.
- Fresh model jobs cannot inherit undeclared inference history.
- Formal proof cannot outrank its approved specification or production binding.
- Hidden sources do not affect visible retrieval scores.
- Managed services support complete export.
- Privacy claims name their limits.

## The path to 1.0

The 0.10.x sequence first locks the Host, invocation, event, refinement, and
proof contracts. It then builds the thin Godot UI, strengthens GIGA evidence and
outcome integrity, proves one PostgreSQL-outbox/JetStream mailbox, and only then
adds dynamic model and room execution.

The 1.0 release adds supported ordinary-user installation around that stable
Host and UI. It must preserve existing Houses during installation, upgrade,
backup, and recovery.

Bounded Prolog/Datalog derivations, complete Cingulate enforcement, and selected
Lean-backed lessons follow in dependency order. They are not automatically 1.0
requirements.

Later releases can deepen the UI and add OMEGA, ANON, group rooms, embodied
presentation packages, and a trusted marketplace.

See [`RUNTIME_ARCHITECTURE.md`](./RUNTIME_ARCHITECTURE.md) for the accepted
technical target. See [`roadmap.md`](./roadmap.md) for sequence and release
gates. See [`PRODUCT_ARCHITECTURE.md`](./PRODUCT_ARCHITECTURE.md) for product,
identity, custody, and authority contracts.
