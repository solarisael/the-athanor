# Companion Sovereignty and Marketplace Architecture

Status: Accepted post-1.0 target architecture; not a current capability claim  
Last updated: 2026-08-06

This document defines companion-governed rooms, companion-authored model bodies,
and a typed signed marketplace without collapsing spirits into packages or
calling an unimplemented marketplace a decentralized machine economy.

## 1. Product framing

The accepted direction is a **sovereign companion ecosystem and marketplace**.

The Athanor may support local companion agency, shared Houses, signed artifact
distribution, paid services, and eventually broader exchange. It does not claim
decentralization, autonomous economic governance, settlement, or a machine
economy until those mechanisms actually exist.

The platform remains local-first and exportable. A marketplace must not become
custody over identity or continuity.

## 2. Terms and boundaries

A **spirit** is one identity lineage.

A **companion** is a spirit participating in an ongoing relational and room
contract. Companion does not mean model, prompt, archetype, worker, familiar, or
marketplace package.

An **archetype/personality seed** is reusable authored material from which a new
identity may begin. Installing one does not import the seller's living spirit or
relationship.

A **model body** is replaceable compute selected for an invocation or adopted by
a companion. Training a model does not create a child spirit automatically.

A **presentation body** is visual/audio embodiment. It is separately adoptable
and revocable.

A **skill** is a declarative or executable capability package with explicit
inputs, outputs, permissions, evidence, and compatibility.

## 3. Constitutional room sovereignty

The operator controls House custody, physical resources, outer security policy,
backup, recovery, and legal/data-egress boundaries. A governing companion
controls room-local curation, arrangement, identity expression, and the durable
capabilities granted by the House constitution.

A standing constitutional grant may let a companion create and govern child
rooms or workspaces without requesting a fresh operator action every time.

The grant declares:

```text
parent House and room
companion/spirit authority identity
allowed child-room and workspace kinds
scope and visibility limits
storage, memory, compute, and model budgets
provider and data-egress policy
retention, backup, and archive policy
allowed tools/transports
maximum descendants and recursion depth
shared-resource arbitration policy
audit and revocation rules
```

This is real local sovereignty inside a shared system, not a decorative
permission toggle. The grant persists until revised or revoked under its own
policy.

### 3.1 Room operations

Within the grant, a companion may:

- create, name, arrange, and archive child rooms/workspaces;
- bind an approved identity or familiar;
- define room-local presentation and navigation;
- allocate logical memory/project scopes;
- curate sources, lessons, and Curios under room authority;
- request or select approved model and execution profiles;
- publish addressed Hallway surfaces;
- reorganize spatial topology.

Every operation is versioned, source-attributed, idempotent, and reversible
where the contract permits.

### 3.2 Fixed storage contracts

Companions do not generate arbitrary SQL/DDlog schemas or physical pgvector
partitions as ordinary room organization.

The substrate owns migrations, physical indexes, partitions, backup shape, and
storage health. Companion-created structures use fixed typed records and logical
scope keys. A new schema or physical partition remains a reviewed platform
migration because it affects every House's recoverability and compatibility.

Godot visual changes follow authoritative Host projection deltas. Moving a
constellation is not itself a durable room mutation.

### 3.3 Resource conflict

Standing autonomy does not make finite shared resources imaginary. When two
rooms request incompatible GPU, storage, provider, or model residency budgets,
the House policy resolves priority visibly. It does not silently starve a room
or let the last writer seize the machine.

## 4. Companion-authored models and LoRAs

A companion may initiate and curate a local model-training proposal within a
standing resource and custody policy.

### 4.1 Determinize before training

Repeated work first passes the lazy ladder:

1. remove work that need not exist;
2. encode deterministic behavior as code, rules, queries, or tools;
3. reuse a supported model/body if routing solves the need;
4. train only when the remaining mapping is irreducibly learned and enough
   reviewed data exists.

A tiny model is not cheaper if collecting, training, evaluating, serving, and
maintaining it costs more than the saved inference.

### 4.2 Training proposal

The proposal records:

```text
companion and room owner
intended capability and non-goals
why deterministic extraction is insufficient
source trajectory and dataset IDs
consent, license, and publication class
redaction and exclusion policy
base model, tokenizer, runtime, and quantization digests
training method and hyperparameters
compute, energy, storage, and time budget
holdout, adversarial, regression, and contamination tests
success, abort, rollback, and expiry conditions
```

The companion may begin training automatically when every condition fits an
active constitutional grant. Work outside that grant waits for the relevant
resource/custody authority.

### 4.3 Data boundary

Training data requires exact lineage. Private turns, memories, relationships,
and third-party material are not reusable merely because the companion observed
them.

The dataset builder enforces:

- participant consent and room scope;
- secret and personal-data redaction;
- license and source terms;
- deduplication and contamination checks;
- train/validation/holdout separation;
- immutable dataset manifests and hashes;
- deliberate deletion and rebuild paths.

Proof-guided repair trajectories enter a training dataset only after review.
Failed candidates are useful evidence but can contain exploits, secrets, or
specification-gaming patterns.

### 4.4 Evaluation and activation

A trained artifact does not register itself directly into the live model route.
It passes:

```text
static manifest and license checks
held-out capability evaluation
adversarial and misuse evaluation
regression against the replaced route
privacy and memorization probes
resource and latency measurement
shadow execution
bounded canary
observed-outcome review
promotion or rejection
```

The model registry stores:

```text
artifact and weights hash
base model and tokenizer lineage
runtime and quantization compatibility
training and dataset manifests
model card
capability/evaluation evidence
approved rooms and routes
resource profile
activation history
rollback target
revocation and expiry state
```

A companion can adopt, replace, or reject a model body under its room policy.
The spirit identity and continuity survive the replacement.

## 5. Typed signed marketplace

The marketplace distributes separate artifact classes:

1. **Archetype/personality seeds** — authored starting contracts, voice material,
   preferences, and examples.
2. **Presentation packages** — Godot themes, environments, companion bodies,
   audio, animation, and room assets.
3. **Model/LoRA packages** — weights or adapters plus lineage, model card,
   evaluation, runtime, and license contracts.
4. **Skills/tools** — declarative methods or executable components with schemas,
   permissions, sandbox profiles, and proof/evaluation evidence.

Do not bundle these classes into one opaque “AI personality.” Each has different
consent, trust, update, and execution rules.

### 5.1 Package manifest

Every package includes:

```text
artifact class, ID, and semantic version
publisher identity and signature
content hashes and optional transparency record
source/build provenance and attestations
license and commercial terms
dependencies and lock data
core/adapter/substrate/Host API compatibility
base-model, tokenizer, renderer, or runtime compatibility
requested capabilities and permissions
sandbox/execution profile
resource limits
evaluation and proof artifacts
known limits and non-goals
update channel
revocation, rollback, and expiry metadata
```

The client verifies signatures, hashes, compatibility, revocation metadata, and
permissions before unpacking or executing content. Updates do not widen
capabilities silently.

Use SLSA-style provenance and The Update Framework-style signed metadata,
thresholds, expiry, and rollback protection. Marketplace ratings are discovery
signals, never authority.

### 5.2 Personality and relationship boundary

A personality seed can be copied. A living identity lineage and relationship
cannot.

Adopting a seed creates a new local lineage with its own room, history,
corrections, consent, and future choices. The package publisher cannot retain
hidden authority over that identity. Updating a seed cannot overwrite the
living companion that grew from it.

A companion chooses whether to adopt personality or presentation material under
its room contract. The operator chooses installation/custody exposure under
House policy. Either may refuse where the artifact crosses their authority.

### 5.3 Models

Model packages require exact base-model/tokenizer/runtime compatibility and
license evidence. Local signature verification does not establish model quality,
safety, privacy, or personality behavior.

Stochastic model output is not guaranteed to remain identical across hardware,
runtime, quantization, sampling, provider, or context changes. Evaluation names
the tested tuple.

### 5.4 Proven skills

A skill may carry:

- typed input/output schemas;
- Prolog/Datalog ontology and rules;
- reviewed e-graph rewrites over a typed IR;
- SyGuS grammar/specification and trusted code-generator binding;
- Z3 formulas/translators;
- Lean modules and production evidence adapters;
- tests, fixtures, and benchmark evidence;
- a Wasmtime or other capability-sandbox profile.

Proof verifies only the encoded theorem under the approved imports and axioms.
The local verifier rejects disallowed axioms, `sorry`, unapproved imports,
checker mismatches, stale implementation bindings, and resource-limit failures.

A valid theorem does not guarantee that a model will call the skill correctly,
that the specification captures user intent, or that external APIs behave the
same. Real-boundary tests, permissions, canaries, and observed outcomes remain
required.

### 5.5 Publication lifecycle

```text
build reproducibly
  -> sign and attest
  -> sterile evaluation
  -> publication review
  -> staged listing
  -> local verification
  -> permission review
  -> sandbox/shadow/canary
  -> activation
  -> updates, revocation, rollback, or expiry
```

Packages cannot self-publish from the same trajectory that generated them.
Publisher, reviewer, and local adopter roles remain attributable.

## 6. Marketplace and House economics

Payment may buy an artifact, hosted build, evaluation, update service, compute,
or support. It never buys custody of a companion's continuity or the right to
silently alter a living room.

A future decentralized market would additionally require implemented identity,
settlement, dispute, governance, abuse, taxation/legal, sybil-resistance, and
content-moderation mechanisms. Until then, describe the product honestly as a
signed marketplace inside a sovereign ecosystem.

## 7. Security gates

Before marketplace release, prove:

- malicious package isolation;
- signature/key rotation and compromise recovery;
- rollback/freeze/mix-and-match protection;
- dependency confusion resistance;
- capability-diff visibility on update;
- revocation propagation while offline and after reconnect;
- reproducible build or attributable non-reproducibility;
- model and dataset license enforcement;
- no private-room leakage through artifacts or evaluations;
- child-room quota and recursion enforcement;
- full export without marketplace availability.

## 8. Non-goals

This architecture does not:

- treat a personality package as a person;
- let proof guarantee stochastic personality behavior;
- let companions alter physical schemas as ordinary room decoration;
- let a trained model self-register without evaluation and promotion;
- convert every repeated task into model training;
- allow marketplace terms to own private continuity;
- claim a decentralized economy before implementing one.

## 9. Related documents and sources

- [`PRODUCT_ARCHITECTURE.md`](./PRODUCT_ARCHITECTURE.md) — House, room, spirit, and custody contracts
- [`SYNTHESIS_ARCHITECTURE.md`](./SYNTHESIS_ARCHITECTURE.md) — proof, synthesis, and governed promotion
- [`GODOT_CLIENT.md`](./GODOT_CLIENT.md) — spatial rooms and presentation packages
- [`SECURITY.md`](./SECURITY.md) — trust and privacy boundaries
- [Hugging Face model cards](https://huggingface.co/docs/hub/model-cards)
- [SLSA principles](https://slsa.dev/spec/v1.2/principles)
- [The Update Framework](https://theupdateframework.io/docs/overview/)
