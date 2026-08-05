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

Solarisael House is an operational late beta. It runs daily and has external installations.

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
- adapters for more than one AI harness.

The public evidence document separates measured results from planned claims.

## Planned feature map

| Feature | Plain-language promise | Status |
|---|---|---|
| GIGA Hippocampus Stage 1 | Notice possible memories and lessons while life happens, then keep them non-authoritative until review | Current |
| Curios | Keep selected hunches until later context makes them meaningful | Current |
| GIGA Striatum | Keep the right reviewed lessons warm on every turn while a work state persists | Current — coding/project slice |
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
- Hidden sources do not affect visible retrieval scores.
- Managed services support complete export.
- Privacy claims name their limits.

## The path to 1.0

The 0.9.x phase finishes Hippocampus evaluation, adds public evidence with
sanitized fixtures, and begins the procedural reliability path through
Striatum's state-conditioned lesson activation. Cingulate follows with explicit
regression and proof-conflict detection.

The 1.0 release adds supported ordinary-user installation. It must preserve
existing Houses during upgrades.

Later releases can add OMEGA, ANON, group rooms, embodied rooms, and a trusted
marketplace.

See [`roadmap.md`](./roadmap.md) for sequence. See [`PRODUCT_ARCHITECTURE.md`](./PRODUCT_ARCHITECTURE.md) for the accepted technical contracts.
