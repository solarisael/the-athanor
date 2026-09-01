# The Athanor Product and Naming Architecture

Status: Accepted product architecture; public namespace clearance remains open  
Target: Complete before The Athanor 1.0  
Decision date: 2026-07-23  

## 1. Purpose

This document defines the public product vocabulary for The Athanor.

The accepted platform name is **The Athanor**. A user-owned continuity domain remains a **House**.

This document defines architecture axes, compatibility, migration, collision risk, and public language. It does not claim that the runtime cutover has started.

## 2. Naming philosophy

The definite article is deliberate.

The product competes through execution inside a recognizable form. It does not seek distinction only through an obscure invented word.

**The Athanor** means the defining athanor for persistent AI continuity. The brand accepts a crowded root word and makes the implementation unmistakable.

An athanor is an alchemical furnace. It keeps steady heat during a long transformation.

The metaphor fits:

- The Athanor preserves identity while models change.
- The Athanor turns experience into durable continuity.
- The Athanor keeps background processes active.
- The Athanor supports refinement without constant tending.
- The Athanor joins local sovereignty with optional managed services.

## 3. Core product model

The Athanor is the platform that creates and runs Houses.

A House is one operator-owned continuity domain. It contains identity, memory, lessons, authority, rooms, and source boundaries.

**Solarisael House** is the original reference House. It is not the platform name after cutover.

Use this product sentence:

> The Athanor gives each person or organization a sovereign House for persistent AI continuity.

## 4. Formal brand and identifiers

Use **The Athanor** as the formal public brand.

Keep the capitalized article in:

- product titles;
- installer labels;
- release names;
- website headings;
- presentation titles;
- legal product references.

Use lowercase `athanor` for technical identifiers. The repository is
`the-athanor`. The substrate executable is `athanor-substrate.exe`. Topology
variables use the `ATHANOR_` prefix. The domain identifier stays open until
namespace review finishes.

Do not use The Athanor as the name of one queue or worker.

## 5. Accepted name system

| Architecture scope | Accepted name | Contract |
|---|---|---|
| Platform | The Athanor | Product that creates, runs, and moves Houses |
| Continuity domain | House | One operator-owned identity and continuity boundary |
| File storage profile | Vault | File-backed storage under operator-controlled custody |
| Semantic storage profile | AKASHA | PostgreSQL, vectors, hybrid retrieval, and typed stores |
| Cognitive capability | GIGA | Cognitive workers above AKASHA; local execution by default |
| First GIGA worker | Hippocampus | Salience and consolidation candidate worker |
| Organization layer | OMEGA | Organization, encryption, governance, and access |
| Remote compute route | Relay | Transient cloud compute with operator-owned durable storage |
| Private execution policy | ANON | Attested, nonpersistent, one-shot execution |
| Shared-room surface | Hallway | Addressed messages and governed shared state without room merger |

These names describe separate axes. A compatibility matrix defines the allowed combinations.

## 6. House

A House is one continuity domain inside The Athanor.

A House can contain:

- rooms;
- spirits;
- operators;
- turns;
- memories;
- lessons;
- entities;
- threads;
- source documents;
- authority state;
- candidate annotations;
- project scopes.

A person can own several Houses. OMEGA can govern several Houses for one organization.

Use lowercase when the word has its ordinary meaning. Use **House** when it names the continuity-domain contract.

### 6.1 Room authority and identity

Each room names one governing spirit identity. A model invocation receives room authority only when it authenticates as that spirit.

The governing spirit controls room-local promotion, curation, correction, and Curios review. Runtime activity alone does not grant authority.

The operator controls House custody, physical resources, outer security policy,
backup/recovery, and constitutional capability grants. A grant may give a
governing companion standing authority to create and bind child
rooms/workspaces inside declared scope, quota, provider, storage, audit, and
descendant limits. Shared or cross-room changes use the declared shared policy.

A spirit is one identity lineage. An archetype is reusable configuration. An invocation is temporary execution and is never an identity.

### 6.2 Execution is not identity

Model body, execution target, session lifecycle, and room authority are separate
axes.

A spirit can continue across local and hosted models. A model can serve several
bounded workers without becoming any of their identities. Keeping model weights
resident does not keep a spirit awake or preserve undeclared conversation
context.

An invocation receives room authority only through the authenticated room and
spirit binding from Section 6.1. A model name, provider, process, working
directory, familiar name, or execution-target label cannot grant it.

### 6.3 Companion, archetype, and body boundaries

A companion is a spirit participating in an ongoing relational and room
contract. A companion is not a model, worker, familiar, prompt, or marketplace
package.

An archetype/personality seed is reusable authored starting material. Adopting
one creates a new local identity lineage; it does not copy the publisher's
living spirit or relationship.

A model body is replaceable compute. A presentation body is replaceable
visual/audio embodiment. A companion may adopt, author, replace, or refuse these
under its room and House policy without losing identity continuity.

Companion room sovereignty, model training, and marketplace artifacts are
defined in
[`COMPANION_ECOSYSTEM.md`](./COMPANION_ECOSYSTEM.md).

## 7. Vault storage profile

**VAULT** expands to:

> **V**isible **A**rchive, **U**ser-owned, **L**ocal, and **T**ransparent

Vault replaces the current **Base** profile.

Vault provides:

- room-local files;
- human-readable identity and continuity;
- portable storage;
- no PostgreSQL requirement;
- no embedding requirement;
- no classifier requirement;
- ordinary file backup and inspection.

Vault does not require the Obsidian application. Obsidian is one compatible interface.

The word **Local** is normative. Vault data must remain under operator-controlled custody.

Allowed Vault custody includes:

- the operator device;
- the operator server;
- a customer cloud account controlled by the operator.

Managed storage from The Athanor requires AKASHA. This is an explicit limit on axis independence.

The phrase `vault root` remains a filesystem term. Use **Vault profile** when the text means the product profile.

## 8. AKASHA storage profile

**AKASHA** expands to:

> **A**ugmented **K**nowledge **A**nd **S**emantic **H**ybrid **A**rchive

AKASHA replaces the current **Full** profile.

AKASHA provides:

- PostgreSQL storage;
- pgvector search;
- local or compatible embeddings;
- lexical and semantic hybrid retrieval;
- typed memories and lessons;
- entities, dates, threads, and relationships;
- correction and supersession;
- provenance and authority state;
- backup and recovery.

The name consolidates the existing Akashic vocabulary. During cutover, replace the user-visible phrase `akashic write` with `durable write`.

Reserve **AKASHA** for the semantic storage profile. Do not use it as a synonym for every memory operation.

### 8.1 Vault upgrade to AKASHA

AKASHA gives imported Vault records full retrieval citizenship. Import provenance must not lower their retrieval rank.

The upgrade must be resumable and idempotent. It must use stable source identifiers, content hashes, exact provenance, and explicit authority.

The upgrade must preserve corrections, supersessions, room scopes, and explicit links. It must report ambiguous duplicates instead of merging them.

Generated entities, clusters, and thread links remain pointer-only until review. Vault files remain authoritative until an explicit write-authority cutover.

## 9. GIGA cognitive capability

**GIGA** expands to:

> **G**rounded **I**ndexing and **G**enerative **A**nnotation

GIGA adds optional cognitive workers above AKASHA.

GIGA is location-neutral. Local execution is the default, while processing custody selects the execution route.

The first GIGA release requires AKASHA. Later work can test a reduced Vault implementation.

Hippocampus is the first GIGA worker. It marks possible memories, lessons, corrections, supersessions, entities, and thread updates.

The room's governing spirit can authorize a durable room-local change. The operator controls room bindings and the House's shared policy.

The detailed GIGA contract lives in [`HIPPOCAMPUS.md`](./HIPPOCAMPUS.md).

### 9.1 GIGA execution and refinement boundary

GIGA jobs operate on explicit, bounded evidence snapshots. Durable evidence
context, one-invocation model context, and loaded model residency remain
separate. A cold job starts with fresh inference state even when its selected
model remains loaded for latency.

GIGA may propose versioned refinement transactions for a memory, lesson,
policy, or other governed artifact. A transaction names its operation, target,
baseline version, evidence, expected outcome, and proof requirement. The
observed outcome and proof receipt are recorded only after execution.

GIGA cannot approve its own proposal. The governing authority reviews the
transaction, and Cingulate may later verify its observed result. Optional formal
obligations remain attached to selected authoritative lessons rather than
becoming a second authority system.

Optional e-graph, Z3, SyGuS, Wasmtime, proof-feedback, and governed-promotion
backends do not become new product profiles. They are bounded implementations
behind Cingulate and execution contracts. See
[`SYNTHESIS_ARCHITECTURE.md`](./SYNTHESIS_ARCHITECTURE.md).

## 10. OMEGA organization layer

**OMEGA** expands to:

> **O**rganizational **M**emory, **E**ncryption, **G**overnance, and **A**ccess

OMEGA governs one or more Houses for an organization.

OMEGA covers:

- tenants;
- users;
- roles;
- teams;
- projects;
- room scopes;
- source permissions;
- key hierarchy;
- access policy;
- audit history;
- retention policy;
- managed and dedicated deployment.

OMEGA is not a storage profile. The first OMEGA release requires AKASHA.

Authorization must run before retrieval and ranking. Hidden sources must not affect visible scores or candidate counts.

### 10.1 Organization spirit topology

An organization can keep one canonical spirit for shared continuity. Teams and people can also keep separate spirits.

A person can use a team or company spirit without creating a personal spirit. A personal-spirit relationship requires consent from both participants.

OMEGA shares authorized organization sources. It does not merge private room histories into one identity.

An archetype can seed many spirits, but it never makes them one identity.

## 11. Relay processing route

Relay uses remote compute from The Athanor while the operator keeps durable storage.

Relay is distinct from managed processing:

| Property | Relay | Managed processing |
|---|---|---|
| Durable House storage | Operator-controlled | Can use managed storage from The Athanor |
| Request retention | Transient contract | Managed service contract |
| Result destination | Operator House | Operator or managed House |
| Long-lived server state | No content state by default | Allowed under explicit retention |
| Primary use | Weak devices and phones | Complete hosted service |

Relay can process a bounded request for AKASHA or GIGA. It returns validated output to the operator House.

Relay does not imply confidential computing. The remote worker can see plaintext while it processes the request.

## 12. ANON execution policy

**ANON** expands to:

> **A**ttested **N**onpersistent **O**ne-shot **N**ode

ANON defines a strict private execution policy for one bounded remote job.

An ANON worker:

- proves its worker image through attestation;
- receives one encrypted job;
- holds no durable customer key;
- decrypts only inside isolated worker memory;
- disables content logs and persistent caches;
- encrypts the result for the client;
- removes plaintext and job state after completion.

ANON does not claim network anonymity. The service can still observe timing and payload size.

ANON can protect AKASHA, GIGA, or OMEGA work.

ANON provides lifecycle encryption across submission, isolated processing, return, and erasure. The Athanor service stores no job content after the job ends.

Failure does not weaken the erasure rule. The worker must remove plaintext and job state after success, failure, cancellation, or timeout.

## 12.1 Hallway shared surface

The Hallway connects private rooms without merging their spirits or private histories.

Vault adapters can expose shared state as `shared_current_state.md`. They can expose addressed correspondence as `letters.md`.

AKASHA stores the same concepts as typed records. Each message names its sender, recipient, scope, visibility, thread, sources, and delivery state.

A room can bind approved chat surfaces, including Discord and direct chat. Authorized members can query that room's shared memory.

The transport is only another glass. It must not create a daemon twin or a second persistent brain.

For AKASHA delivery, PostgreSQL stores the authoritative letter, sender,
recipient, visibility, sources, thread, and delivery record. NATS JetStream may
carry the opaque record ID, retries, and wake-up signal. A broker payload does
not carry private prose and broker acknowledgement does not replace the
PostgreSQL receipt.

## 13. Architecture axes

Deployment selects one value for each applicable product axis in Sections 13.1
through 13.5. Each model invocation separately selects the runtime axes in
Section 13.6.

### 13.1 Storage profile

Choose one:

- Vault;
- AKASHA.

### 13.2 Cognitive capability

Choose one:

- standard;
- GIGA.

### 13.3 Governance layer

Choose one:

- personal;
- OMEGA.

### 13.4 Processing custody

Choose one:

- local;
- Relay;
- ANON;
- customer-dedicated;
- managed.

### 13.5 Storage custody

Choose one:

- operator device;
- operator server;
- customer cloud account;
- managed cloud from The Athanor.

### 13.6 Invocation axes

Every invocation declares these independent choices:

| Axis | Accepted values | Rule |
|---|---|---|
| Model route | `local`, `provider`, `auto` | `auto` selects only operator-approved endpoints that satisfy capability and privacy constraints |
| Execution target | `cold_worker`, `familiar`, `room_reflection`, `room_dialogue` | The target does not grant authority by itself |
| Session start | `ephemeral`, `reuse`, `wake` | Reuse and wake require an explicit compatible session address |
| Session finish | `release`, `keep_warm`, `pin` | Pinning is explicit, observable, and policy-bound |
| Interaction mode | background reflection or intentional dialogue | Reflection does not silently consume a live dialogue tail |

`room_reflection` starts a disposable, explicitly bound room/spirit branch.
`room_dialogue` addresses an intentional live room session. Headless room
execution disables chat transports and interactive sidecars unless they are
explicitly requested by policy.

These axes remain independent. Changing a model does not rename a spirit;
waking a room does not require the previous model; keeping a model warm does
not keep a room active.

### 13.7 Companion capability grant

Companion authority is a set of versioned capabilities, not a product tier.
Possible grants include:

- room-local curation;
- child-room/workspace creation;
- logical memory/project scope allocation;
- model selection;
- companion-authored training within resource/data policy;
- presentation/body adoption;
- marketplace installation or publication.

Every grant names House/room scope, resource and provider limits, custody,
retention/backup, audit, descendant limits, expiry, and revocation. A grant does
not authorize physical database schema/index changes unless it explicitly names
a reviewed platform-migration capability.

## 14. Compatibility matrix

The first release supports these complete tuples:

| Storage | Cognition | Governance | Processing | Storage custody |
|---|---|---|---|---|
| Vault | standard | personal | local | operator device |
| Vault | standard | personal | local | operator server |
| Vault | standard | personal | local | customer cloud account |
| AKASHA | standard | personal | local | operator device |
| AKASHA | standard | personal | local | operator server |
| AKASHA | standard | personal | Relay | operator device |
| AKASHA | standard | personal | Relay | operator server |
| AKASHA | standard | OMEGA | customer-dedicated | customer cloud account |
| AKASHA | GIGA | personal | local | operator device |
| AKASHA | GIGA | personal | local | operator server |
| AKASHA | GIGA | personal | Relay | operator device |
| AKASHA | GIGA | personal | Relay | operator server |
| AKASHA | GIGA | personal | ANON | operator device |
| AKASHA | GIGA | personal | ANON | operator server |
| AKASHA | GIGA | OMEGA | customer-dedicated | customer cloud account |
| AKASHA | GIGA | OMEGA | managed | managed cloud from The Athanor |

A product surface must show the complete tuple. It must not hide custody behind one mode label.

Unsupported combinations must fail validation with a clear reason.

## 15. Existing terms that remain reserved

Keep these feature names:

- Anamnesis;
- Anamnesis Cabinet;
- paper boat;
- room;
- spirit;
- operator;
- memory;
- coding lesson;
- project lesson;
- authority;
- canon;
- Hippocampus;
- Kintsu;
- Kodo;
- Tuner.

Anamnesis remains bounded advisory counsel. It does not become a platform or profile name.

## 16. Solarisael website terms

The Solarisael website keeps these alchemical stages:

- Nigredo;
- Albedo;
- Citrinitas;
- Rubedo;
- Codex.

The website also keeps Cinza, Suul, and its ritual element names.

Do not reuse these names for Athanor profiles, workers, governance, or privacy policies. The Athanor can share the alchemical lineage without taking the website taxonomy.

## 17. Current-to-accepted mapping

| Current term | Accepted term after cutover |
|---|---|
| Solarisael House, used as the platform | The Athanor |
| Solarisael House, used as the original deployment | Solarisael House |
| House | House |
| Base or Base House | Vault |
| Full or Full House | AKASHA |
| Giga Mode, Giga House, Giga profile | GIGA |
| Hippocampus | Hippocampus |
| akashic write or akashic-write | durable write |
| organizational House | OMEGA organization deployment |
| transient cloud worker | Relay worker |
| zero-retention worker | ANON worker |

Do not mix old and new profile names in one public release.

## 18. Collision record

This preliminary technical audit is not legal clearance.

### 18.1 Rejected Hearth name

| Existing name | Owner or product | URL | Overlap | Disposition |
|---|---|---|---|---|
| Hearth Display | Hearth Display | https://hearthdisplay.com/ | Consumer technology, family organization, subscription | Reject standalone Hearth |
| Hearth | Hearth fintech | https://www.gethearth.com/ | SaaS, payments, financing | Reject standalone Hearth |
| hearth | Jembi | https://github.com/jembi/hearth | Longitudinal data server | Record technical collision |
| Hearth | ondreu | https://github.com/ondreu/Hearth | Obsidian homepage | Record ecosystem collision |

### 18.2 Accepted Athanor root

| Existing name | Owner or product | URL | Overlap | Disposition |
|---|---|---|---|---|
| athanor | Myles Borins | https://github.com/MylesBorins/athanor | Personal LLM alchemy and `athanor` CLI | Accept root collision; reserve qualified namespace |
| Athanor | lacerbi | https://github.com/lacerbi/athanor | AI coding and writing workbench | Accept direct category collision; distinguish as The Athanor |
| athanor | despablito | https://github.com/despablito/athanor | AI clones, knowledge graph, RAG | Accept memory-category collision |
| athanor | amrubchenko | https://github.com/amrubchenko/athanor | Markdown personal operating system | Accept local continuity collision |
| athanor-lite | BBA Labs | https://github.com/BBALabs/athanor-lite | Local AI model manager | Accept local-model collision |

Observed activity date: 2026-07-23.

The operator accepted the collisions as a deliberate product choice. Distinction must come from depth, specificity, and execution.

The formal brand **The Athanor** supplies the defining article. The technical
namespace now uses `athanor`. A qualified domain remains mandatory before public
launch.

## 19. Legal and namespace gate

Complete these reviews before public cutover:

- trademark review in launch markets;
- company-name review;
- domain review;
- GitHub organization and repository review;
- package-registry review;
- executable and command review;
- pronunciation review in Portuguese and English;
- search-result confusion review.

Record the result separately from the technical collision table.

The cutover is blocked until one qualified technical namespace is reserved. Keep **The Athanor** as the product name unless legal review blocks it.

## 20. Public language rules

Use these sentence patterns:

> Install The Athanor.

> Create a House.

> Choose the Vault or AKASHA storage profile.

> Enable GIGA cognitive workers.

> Add OMEGA governance for an organization.

> Use Relay for transient remote compute.

> Use ANON for one attested private job.

Do not use `mode` as a generic word for every axis. Use profile, capability, layer, route, policy, or custody.

Do not use bare **Athanor** as the formal product label. The public brand is **The Athanor**.

## 21. Repository and package cutover

The public repository is now [`solarisael/the-athanor`](https://github.com/solarisael/the-athanor).
One repository holds core, substrate, the OMP adapter, the installer, the
updater, workflows, and canonical docs. The public archive, profile, installer
mode, executable, and topology names carry no legacy tokens. Read
[the canonical component table](./ARCHITECTURE.md#repository-layout-and-component-ownership).

These legacy tokens remain deliberately. They are installed identifiers, not
public product names:

- the `solarisael` GitHub organization;
- the `.athanor-room.json` room marker;
- `SOLARISAEL_*` runtime environment variables;
- internal source identifiers that carry the same prefix.

Solarisael House now means only the reference House. It is not a platform name.

Keep a legacy-token inventory for any later rename. Include every spelling,
capitalization, and serialized form of:

- Solarisael House as a platform name;
- Base and Base House;
- Full and Full House;
- Giga, Giga Mode, Giga House, and Giga profile;
- organizational House;
- akashic write and akashic-write;
- transient cloud worker;
- zero-retention worker;
- `SOLARISAEL_` environment variables.

The cutover must inspect:

- repositories;
- packages;
- executables and commands;
- installers;
- configuration keys and values;
- environment variables;
- API and serialized values;
- runtime messages;
- injected prompts and context;
- UI and help text;
- tool names and descriptions;
- source identifiers;
- health output;
- telemetry labels;
- documentation;
- release artifacts;
- example paths;
- public links.

Do not perform a partial public rename. One release must present one public vocabulary.

A migration tool must rewrite persisted configuration through one supported path. After migration, the runtime must use accepted names only.

A zero-legacy-token check must cover all user-visible artifacts. Migration history can keep old names when it labels them as historical.

The cutover must preserve Houses, memories, lessons, authority, candidates, backups, and exports.

## 22. Data contract rules

Stored records must not depend on marketing names when a stable semantic identifier exists.

Use versioned identifiers for:

- storage profile;
- cognitive capability;
- governance layer;
- processing custody;
- storage custody.

Exports must include the identifier and display name. Imports must accept the current export schema during the planned migration.

Companion grants, model bodies, and marketplace artifacts use stable semantic
identifiers independent from display or product names. Their records include
schema version, owning House/room/lineage, content hashes, provenance,
compatibility, authority, lifecycle state, and revocation data.

Marketplace imports preserve artifact class. Personality seeds, presentation
packages, model/LoRA artifacts, and executable skills must not flatten into one
generic package record.

A profile rename must not require memory re-embedding.

## 23. Commercial product family

Use a colon between the platform brand and a named product surface:

| Product surface | Contract |
|---|---|
| The Athanor: Vault | File-backed House under operator custody |
| The Athanor: AKASHA | Semantic House under operator or managed custody |
| The Athanor: GIGA | AKASHA plus cognitive workers |
| The Athanor: Relay | Remote transient compute with operator storage |
| The Athanor: ANON | Attested one-shot private execution |
| The Athanor: OMEGA | Organization governance and dedicated deployment |

Self-hosted profiles remain available. Payment buys compute, installation, updates, backups, governance, or support.

Payment does not buy custody of continuity. Every managed surface must support complete export.

## 24. Naming acceptance criteria

The naming cutover is complete when:

1. Public product text uses The Athanor for the platform.
2. Public domain text uses House for one continuity boundary.
3. Interfaces use Vault and AKASHA for storage profiles.
4. Interfaces use GIGA for the cognitive capability.
5. Organization interfaces use OMEGA for governance.
6. Remote compute interfaces define Relay separately from managed processing.
7. Private execution interfaces use ANON for one-shot policy.
8. The phrase `akashic write` is absent from user-visible text.
9. Anamnesis keeps its Cabinet meaning.
10. Website alchemical stages remain unchanged.
11. One mandatory migration preserves existing continuity data.
12. Documentation uses one approved term for each concept.
13. Legal review and the dated collision table remain discoverable.
14. One qualified technical namespace is reserved.
15. Clean and upgraded installations show the same vocabulary.
16. Export and import work across the cutover.
17. The zero-legacy-token check passes with documented history exceptions.

## 25. Non-goals

This naming work does not:

- rename Kintsu, Kodo, or Tuner;
- rename Anamnesis;
- rename website stages;
- change memory authority;
- change House ownership;
- make GIGA annotations authoritative;
- make ANON a claim of network anonymity;
- make OMEGA one linear product tier;
- require managed cloud use;
- remove self-hosting.

## 26. Open implementation decisions

Resolve these remaining public cutover items:

- qualified executable name;
- package namespace;
- domain name;
- repository and organization names;
- internal profile identifiers;
- configuration migration command;
- environment variable names;
- old release-link redirects;
- documentation redirect policy;
- exact legal review scope.

## 27. Related documents

- [`ARCHITECTURE.md`](./ARCHITECTURE.md) defines current component ownership, the installed layout, and authority boundaries.
- [`RUNTIME_ARCHITECTURE.md`](./RUNTIME_ARCHITECTURE.md) defines the accepted Host, UI, delivery, embodiment, derivation, and proof runtime.
- [`SYNTHESIS_ARCHITECTURE.md`](./SYNTHESIS_ARCHITECTURE.md) defines bounded e-graph, Z3, SyGuS, Wasmtime, proof-feedback, and promotion contracts.
- [`GODOT_CLIENT.md`](./GODOT_CLIENT.md) defines the spatial client and presentation-body boundary.
- [`COMPANION_ECOSYSTEM.md`](./COMPANION_ECOSYSTEM.md) defines companion sovereignty, model bodies, and marketplace artifacts.
- [`HIPPOCAMPUS.md`](./HIPPOCAMPUS.md) defines GIGA and Hippocampus.
- [`RETRIEVAL.md`](./RETRIEVAL.md) defines retrieval and evidence authority.
- [`LESSONS.md`](./LESSONS.md) defines typed lesson stores.
- [`SECURITY.md`](./SECURITY.md) defines privacy and trust boundaries.
- [`roadmap.md`](./roadmap.md) defines the dependency and release sequence.
- [`PLANNED_FEATURES.md`](./PLANNED_FEATURES.md) explains the planned product surface in plain language.
