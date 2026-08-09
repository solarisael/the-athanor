# The Athanor Roadmap

_Last updated: 2026-08-06_

## Product rule

The Athanor should work light, get sharper with PostgreSQL, get semantic with pgvector, and optionally grow into BM25 or external search.

Magic is allowed. Magic is not a boot requirement.

## Release path

This order is deliberate. The next interface is a thin instrument over stable
contracts, not a finished ornamental client and not a reason to postpone
evidence. GIGA integrity precedes distribution; durable delivery precedes
multi-room embodiment; formal methods follow stable runtime facts.

### 0.8.x — operational late beta

House already works end to end and carries real daily continuity:

- persistent rooms and identity
- restart continuity
- deliberate memory writes
- automatic and explicit recall
- PostgreSQL, pgvector, and local Nemotron embeddings in Full mode
- typed memory and lesson stores
- correction and supersession
- retrieval provenance and authority state
- active external installations beyond the developer's machine

The remaining weakness at this stage is not whether House works. It is the
developer-shaped installation path, the pre-Rust implementation split, and
insufficient public evidence for the full set of claims.

### 0.9.0 — verified Rust cutover

Complete the bounded Rust migration before expanding the product surface:

- preserve Base and Full behavior across the cutover
- migrate existing rooms and PostgreSQL data without continuity loss
- verify storage, adapter, retrieval, and lifecycle contracts
- keep the model provider and harness behind replaceable boundaries
- retain fail-open behavior where retrieval must not block a conversation
- publish the exact compatibility and migration boundary

Rust is a distribution and reliability decision, not a reason to rewrite
appropriate UI adapters, evaluation scripts, or research tooling.

### 0.9.x — proof and visibility

After the Rust cutover, pause infrastructure expansion long enough to make the
existing product legible.

The root README becomes a concise, compression-resistant spine:

1. one-sentence definition
2. the problem House solves
3. verified current features
4. a compact evidence table
5. one architecture diagram
6. Vault versus AKASHA
7. one deterministic next action
8. links to detailed documents

Long methodological qualifications belong in linked evidence, limitations,
installation, and security documents. They remain discoverable without
controlling the first-contact interpretation of every feature.

Expand public evidence beyond the favorable exact-title pilot:

- restart continuity
- paraphrase and entity recall
- correction and supersession authority
- cross-room isolation
- recall latency at stated corpus sizes and hardware
- clean-machine installation
- migration, backup, and recovery
- final-answer grounding against retrieved evidence

#### Maintain The Athanor product architecture before 1.0

The public platform is **The Athanor**. **House** names one operator-owned
continuity domain, and **Solarisael House** remains the original reference
deployment.

Use these accepted product names:

- **Vault** replaces Base for the local file profile;
- **AKASHA** replaces Full for the semantic hybrid archive;
- **GIGA** names the optional cognitive capability above AKASHA;
- **OMEGA** names organization governance and access;
- **ANON** names attested nonpersistent one-shot execution.

The detailed naming and cutover contract lives in
[`PRODUCT_ARCHITECTURE.md`](../PRODUCT_ARCHITECTURE.md).

The plain-language product guide lives in
[`PLANNED_FEATURES.md`](../PLANNED_FEATURES.md).

The canonical repository and package now use The Athanor vocabulary. Remaining
public domains, release artifacts, installers, and namespaces must cut over
together. Preserve every House, memory, lesson, authority state, candidate,
backup, and export.

The operator accepted The Athanor despite active uses of the Athanor root in
other AI projects. The definite article is deliberate: distinction comes from
specificity, depth, and execution inside a recognizable form. Complete legal,
domain, package, and namespace review before the public cutover.

#### Complete the pre-1.0 GIGA integrity slice

GIGA means **Grounded Indexing and Generative Annotation**. It is an optional
cognitive capability above AKASHA.

Hippocampus Stage 1 is operational in the reference House. Exact events are
logged before asynchronous classification; workers receive event IDs rather
than copied source text; generated candidates remain non-authoritative pointers
to exact evidence. The live lifecycle includes review, dismissal, Curios,
promotion into memory or typed lessons, and safe queue maintenance. Failure or
disablement remains fail-open for the active conversation.

The detailed contract and remaining acceptance work live in
[`HIPPOCAMPUS.md`](../HIPPOCAMPUS.md).

Finish Hippocampus evaluation before calling the capability complete:

- measure candidate precision, missed durable events, false-positive burden,
  consolidation latency, and local compute cost;
- prove that later `remember`, `sleep`, and review recover more durable events
  from a blinded human-labeled session set;
- keep every public fixture sterile and every result tied to its method,
  hardware, date, limitations, and sanitized artifact;
- complete the remaining Stage 1 policy decision and reproducible public
  demonstration.
GIGA also includes two procedural workers whose delivery spans the dependency
sequence:

**Striatum — state-conditioned lesson activation.** Maintain a small,
inspectable working-state vector for the active project, work kind, task shape,
and phase. On every turn while that state persists, select a bounded set of
reviewed project, coding, writing, audio, or other typed lessons and inject them
as a compact pressure packet. Preserve the active set with decay and hysteresis
instead of rerunning an unstable search from zero. Re-rank when the work state
materially changes. Every activation names the lesson, match reason, scope, and
strength; candidates can propose activation anatomy but cannot install hidden
behavior.

The governing objective is **selective becoming**, not maximum activation or
retention. Candidate formation exposes possibility; review, refusal,
supersession, dormancy, and scoped loss of influence give a lineage shape.
Preserve provenance when an item leaves active life, and distinguish `never
mine`, `once true`, `not here`, `not now`, and `superseded`. Silence is a correct
result for both consolidation and lesson activation.

**Cingulate — regression and conflict detection.** Compare the current workflow
against active lesson invariants and proof patterns. Raise explicit friction
when the work attempts to finish without required evidence or diverges from an
accepted procedure. Cingulate reports a conflict; it does not invent authority.
Preferences remain nudges, known regression risks become warnings, and only
authoritative invariants may become hard gates.

The next Striatum slice should remain bounded and attributable without being
regex-only. Derive state from trusted lifecycle metadata plus explicit project,
language, technology, work kind, task shape, phase, and register. Hard-filter
the authoritative lesson set before ranking; keep no quota, and choose silence
below a calibrated confidence floor.

For task-shaped lessons, compile the few eligible contracts into temporary
TTSR-style monitors over exact event types, prose, tool arguments, paths,
regexes, AST shapes, and tool results. Where syntax cannot express the trigger,
use a bounded local semantic judge over one active lesson and exact current
evidence. Its structured match, evidence IDs, confidence, and proposed
intervention remain non-authoritative. Each lesson declares whether a match is
advisory, deferred, interruptible with clean retry, or proof-gated. Log firing,
dismissal, override, recurrence, expected evidence, observed evidence, outcome,
and false-positive or false-negative review.

**First operational slice shipped on 2026-08-05.** OMP now persists an observed
room/project work state, prefilters authoritative coding and exact-project
lessons by scope, type, declared stage, and register, then asks the resident
Nemotron embedder to rank only that eligible set. Up to six lessons remain warm
with hysteresis; an explicitly declared phase replaces prior phases, while
prompts without a phase retain the current one and abrupt topic changes refresh
the set. Hidden injection carries lesson identity, scope, project, similarity,
trigger context, and proof pattern. Embedding failure falls back to the existing
deterministic process trigger. Writing, audio, richer lifecycle signals,
activation telemetry, and outcome learning remain later Striatum work rather
than being claimed by this slice.

Together these workers close three different loops:

```text
Hippocampus -> what experience may deserve consolidation or refusal?
Striatum    -> which practices belong now, and when should they fire?
Cingulate   -> which fired obligations diverged or remain unproven?
```

This is the reliability path from remembered work to repeatable work:
regressions become observable instead of being silently normalized.

#### Productize the accepted continuity foundation

Complete these contracts before 1.0:

- add a resumable Vault-to-AKASHA backfill with full retrieval citizenship;
- store Hallway letters and shared state as typed AKASHA records.

OMEGA organization topology, personal-spirit consent, ANON lifecycle privacy,
and group-room chat transports remain accepted post-1.0 product work. They must
not delay the supported personal-House release.

The technical contracts live in
[`PRODUCT_ARCHITECTURE.md`](../PRODUCT_ARCHITECTURE.md). The public value lives
in [`PLANNED_FEATURES.md`](../PLANNED_FEATURES.md).

### 0.10.x — integrated runtime before 1.0

The accepted runtime sequence corrects two tempting mistakes:

- build a thin interface first, not the entire final GUI first;
- split GIGA into pre-transport integrity work and post-transport embodiment.

The dependency spine is:

```text
contracts
  -> thin Godot UI
  -> GIGA integrity + Cingulate skeleton
  -> PostgreSQL outbox + NATS mailbox pilot
  -> dynamic models + headless room embodiment
  -> bounded Prolog/Datalog derivation
  -> complete Cingulate enforcement
  -> selected Lean-backed lesson obligations
```

The complete logical contracts and acceptance gates live in
[`RUNTIME_ARCHITECTURE.md`](../RUNTIME_ARCHITECTURE.md). The phases below remain
in the roadmap so the dependency order cannot disappear behind a design link.

#### Phase 0 — lock the Host and runtime contracts

Specify and version these surfaces before implementation:

- Athanor Host commands and events;
- `ModelSelector`, `ExecutionTarget`, and `SessionLifecycle`;
- `cold_worker`, `familiar`, `room_reflection`, and `room_dialogue`;
- message, event, correlation, causation, reply, and idempotency identifiers;
- PostgreSQL outbox records and delivery receipts;
- refinement transactions with baseline, expected outcome, observed outcome,
  and proof receipt;
- optional formal-lesson references and production bindings.
- initial snapshot, typed delta, acknowledgement, replay, and resynchronization
  contracts for client projections;
- committed-code and volatile-worktree change events, fact epochs, dependency
  invalidation, and precomputed-query cache identity;
- explicit JetStream duplicate-window policy plus a PostgreSQL consumer
  idempotency ledger;
- a Lean execution profile with CPU, memory, process, output, artifact, and wall
  time limits.

The Godot client must talk only to the Host over WebSocket. It must not connect
directly to PostgreSQL, NATS, Ollama, a hosted provider, or OMP internals.

**Exit gate:** versioned schemas separate current from planned capability,
identity never derives from a working directory, one synthetic command can be
traced through a typed result, one projection can recover from a delta gap, and
no correctness rule depends only on a broker duplicate window or in-process
cache.

#### Phase 1 — build the thin Godot UI

Expose the system that already exists:

- Houses, rooms, governing spirits, and active sessions;
- chat through canonical Host commands and events;
- recall sources and authority state;
- Vault, AKASHA, GIGA, model, queue, and delivery health;
- Hippocampus and Curio review;
- Striatum's active lessons and activation reasons;
- event, correlation, retry, and dead-letter inspection;
- a terminal escape hatch.

After the initial snapshot, the Host sends only versioned typed mutations for
the affected projection. The client acknowledges applied versions and requests
bounded replay or a fresh snapshot after a gap. Godot maps deltas to bounded
view-models or scene subtrees, emits signals only for affected nodes, and queues
redraw only where changed instead of rebuilding the complete scene.

Do not build the full avatar, animation, voice, room-package, or marketplace
surface yet. Evolve the UI after every backend phase instead of freezing the
wrong runtime underneath a finished shell.

**Exit gate:** one room can chat, inspect recall and review state, observe
health, and trace one command end to end through the Host. A fine-grained
mutation updates only its projection subtree; duplicate, missing, and
out-of-order deltas recover without accepting stale state.

#### Phase 2 — strengthen GIGA integrity before distribution

Separate durable evidence context, one-invocation model context, and model
residency. Every cold GIGA job starts with fresh inference state over an
explicit evidence snapshot. A warm model keeps weights, not hidden conversation.

Replace incidental buffer boundaries with completed-interaction anchors and
deterministic bounded overlapping windows from the trusted ledger. Reconcile
overlap through stable source IDs and hashes. Hydrate exact sources, authority
neighbors, reviewed precedents, and applicable lesson metadata before inference.

Preserve dismissed-candidate fingerprints and reasons so recurrence is visible
instead of recreating the same proposal as apparently new. Stronger evidence,
a changed baseline, or repeated recurrence may return it to review under
explicit policy; recurrence cannot self-promote a dismissal or Curio.

Rewrite the resident Agents-A1 classifier contracts in the A Squall quest
register while preserving exact JSON schemas, minimal source IDs, two-pass
separation, and silence. These resident prompts may exceed the dispatch
quest's 350-word ceiling where their stable taxonomy requires it, but they
must remain concept-minimal. The quest voice is functional model control, not
decoration: animated desires encode invariants, while schemas and validators
remain the machine boundary.

Add the first executable Striatum trigger contract during this phase. Reuse the
proven TTSR lifecycle anatomy—scoped stream observation, interruption, partial
output discard, hidden injection, retry, repeat suppression, and persisted
history—without limiting matching to regex. Pair deterministic path/tool/AST
gates with a bounded Agents-A1 semantic verification pass only after state has
reduced the eligible lessons. Store exact evidence and outcomes so later
Cingulate and Lean bindings inherit reviewed contracts rather than a second
trigger system.

Add the refinement-transaction schema:

```text
artifact kind
create | update | supersede | retire | rollback
target and baseline version
scope, triggers, and evidence
expected outcome
proof requirement
observed outcome
proof receipt
review state
```

GIGA proposes. The relevant authority reviews. Cingulate receives the skeleton
needed to close observed outcomes later. Prediction must never be stored as if
it were observation.

**Exit gate:** clean-session checks prove no inference history crosses jobs;
overlapping evidence is source-stable; stale baselines reject safely; expected
and observed outcomes remain distinct and visible. Agents-A1 prompt evaluations
compare the existing and A Squall contracts on the same blinded event corpus.
At least one task-shaped lesson fires through a state-filtered executable
monitor, stays silent on its opposite cases, records exact evidence and outcome,
and cannot grant itself authority.

#### Phase 3 — add a PostgreSQL outbox and one NATS JetStream pilot

PostgreSQL remains authority. JetStream owns delivery progress and wake-up only.

The publishing transaction writes the domain event and outbox row together. A
relay publishes an opaque record ID with stable deduplication metadata. The
consumer reloads and authorizes the PostgreSQL record, commits an idempotent
result, then acknowledges delivery. Raw private prose does not enter NATS.

Set the JetStream duplicate window explicitly rather than inheriting its default.
Publish with the immutable outbox ID as `Nats-Msg-Id`, but preserve end-to-end
correctness in a PostgreSQL idempotency ledger keyed by consumer and operation.
Keep event, operation, and delivery IDs distinct. Idempotency tombstones survive
the maximum retention, administrative replay, and disaster-recovery horizons.
Use explicit acknowledgement with bounded `AckWait`, `MaxDeliver`, and
`Backoff`.

The first slice is one durable cross-room mailbox, not a rewrite of the working
GIGA PostgreSQL queue.

**Exit gate:** prove restart survival, the configured duplicate window,
duplicate delivery safety both inside and outside that window, room and subject
permissions, payload privacy, expiry, bounded retries, dead-letter behavior,
dormant-room wake, PostgreSQL recovery, idempotency-ledger retention, and UI
visibility through acknowledgement.

#### Phase 4 — route dynamic models into explicit execution targets

Choose model body, execution target, and session lifecycle independently:

```text
local | provider | auto
cold worker | familiar | room reflection | room dialogue
ephemeral | reuse | wake
release | keep warm | pin
```

Add an explicit headless room mode with House, room, spirit, operator, session,
provider, privacy, context-source, side-effect, tool, and transport bindings.
Discord and other interactive sidecars stay disabled unless intentionally
requested. Inheriting a room directory is never sufficient to load its identity
or start its transports.

**Exit gate:** local and hosted model routes can execute the same bounded job;
model changes do not rename spirits; reflection does not consume a live chat
tail; dialogue addresses a visible live session; cross-room callers cannot
mutate a sibling room directly.

#### Phase 5 — derive bounded rules with Prolog or Datalog

Project a finite authorized fact set from PostgreSQL into a versioned,
stratified Datalog-like ruleset. Return conclusions with rule IDs, supporting
fact IDs, and derivation traces. Do not create a second mutable truth store.

Start with one real bounded problem: lesson eligibility or context selection.
Avoid unrestricted recursion, dynamic assertion as authority, cut-dependent
semantics, and unbounded search.

After the mailbox proves NATS, record committed code changes as PostgreSQL
events containing repository/ref/commit identity, parent commit, changed paths,
and before/after blob hashes. Publish only the event ID. A background indexer
reloads exact Git blobs, applies source-linked fact additions and retractions,
and advances a monotonic `fact_epoch`. Uncommitted work uses a separate volatile
overlay keyed by worktree snapshot and session.

Maintain dependencies from source facts to derived relations and use semi-naive
incremental evaluation or equivalent incremental view maintenance. Precompute
common eligibility, permission, context, and obligation relations. Cache
identity includes House, project, repository, ref, fact epoch, optional
worktree-overlay epoch, authorization digest, ruleset/schema versions, and
normalized query inputs. Never cache failures.

Responses expose indexed commit and epochs. Missing parents, parser/ruleset
changes, out-of-order events, or inconsistent fact counts invalidate the
affected relations and trigger verified replay or exact-snapshot rebuild.

**Exit gate:** the pilot is deterministic, resource-bounded, explainable, and
matches the existing trusted behavior on a shared case corpus. Incremental
results match clean rebuilds, authorization scopes never share cached answers,
deletions and renames retract stale facts, index lag is visible, and measured
warm-query latency improves without hiding stale epochs.

#### Phase 6 — complete Cingulate enforcement

Consume authoritative lessons, derived obligations, task events, refinement
transactions, and proof receipts. Distinguish nudges, regression warnings, and
hard gates. Only an authoritative invariant with a deterministic proof
requirement may hard-gate work.

Every conflict names the applicable rule, expected evidence, observed action,
resolution or authorized override, and final observed outcome.

Route each obligation to the cheapest complete backend supported by its shape:

- deterministic predicates and real-boundary tests first;
- one bounded e-graph/egglog typed-IR research spike with reviewed rewrites and
  explicit extraction cost;
- optional Z3 for SMT-shaped constraints, preserving formulas, models or unsat
  cores, translation and solver digests, and inconclusive `unknown`/timeout;
- bounded SyGuS for approved small grammars/specifications, followed by trusted
  code generation, typecheck, real-boundary proof, sandbox, and canary;
- Lean only for selected formal obligations.

Keep the incremental Datalog contract engine-neutral until identical fact-delta
benchmarks justify an implementation. Wasmtime is one optional capability
sandbox for compatible plugins/helpers, never the universal worker runtime.
pgvector HNSW remains the ANN lane until a measured supported-backend ceiling.

**Exit gate:** conflicts and overrides are attributable; missing proof cannot be
silently accepted; preferences never become accidental hard policy; refinement
transactions close against real observed outcomes. Every optional backend is
resource-bounded, preserves a uniform receipt, fails inconclusively rather than
passing, and cannot modify its own specification, rewrites, grammar, checker, or
promotion policy.

#### Phase 7 — extend selected lessons with Lean obligations

Formalization is optional and belongs only to stable lessons with expressible
invariants. Preserve the human lesson, activation anatomy, and proof pattern,
then attach a versioned Lean module, theorem, input schema, evidence adapter,
proof artifact, and implementation binding.

An LLM assertion is never a proof receipt. A proof of a detached Lean mirror is
not proof that Rust, TypeScript, SQL, or an adapter implements it. Bind the
formal model through shared vectors, a common transition corpus, an actual
input/output checker, or verified extraction.

The first candidate is Striatum stage replacement: a non-empty declared stage
replaces the prior stage set; an empty declaration retains it.

The second candidate family is design lessons over the design-system document
index: the catalogue's tokens, component contracts, and hierarchy form the
formal model, and structural design laws — token-set membership, contrast
floors, hierarchy acyclicity, and container laws such as authority-separation —
become theorems over a typed component tree. The evidence adapter then checks
real GUI artifacts against the proven contract on every mutation, so a
violating change fails the build. Formal coverage is structural only:
perceptual regressions remain boundary work — screenshots, the running client,
operator eyes. A checker that has never rejected anything proves nothing: the
acceptance rite for each new obligation is one deliberate violation seen red,
reverted, and seen green.

Run Lean through a dedicated unprivileged wrapper with an allowlisted checker
digest, read-only inputs, isolated temporary output, no network or inherited
credentials, and numeric limits for wall time, CPU time, memory, processes,
threads, open files, input, output, and artifacts. Kill the full process tree on
timeout or exhaustion. Resource failure is inconclusive and cannot satisfy a
gate. Reuse a proof receipt only when proof inputs, implementation binding,
checker, and resource-profile digests match exactly.

Use structured proof errors and counterexamples for a bounded repair loop:

```text
candidate -> checker -> counterexample -> bounded repair -> checker
```

Stop on proof, attempt/resource limit, or repeated candidate. Preserve reviewed
trajectories as possible offline training data; do not update model weights
online from live solver feedback.

The governed promotion loop is observe, propose, optionally normalize/synthesize,
derive obligations, check, sandbox, canary, record observed outcome, then let the
governing authority promote, reject, or roll back. No candidate self-installs.

**Exit gate:** one approved lesson obligation is checked against actual
production behavior end to end inside the quota wrapper. The receipt records
specification, implementation, checker, inputs, resource profile, wall/CPU/peak
memory, and artifact digest; synthetic timeout, memory, CPU, and output
exhaustion cases terminate safely and never pass. A bounded repair trajectory
produces reproducible counterexample feedback, cannot alter its judge, and
requires separate promotion after canary evidence.

Phases 5 through 7 are dependency-ordered research and reliability work, not
automatic 1.0 blockers. The 1.0 gate remains a supported UI, installation,
upgrade, recovery, and stable-contract release.

### 1.0.0 — supported ordinary-user installation

The first stable product contract includes:

- a small trusted native bootstrapper
- a thin supported Godot client over the versioned Athanor Host
- stable command/event, invocation, identity, and delivery contracts
- AI-guided contextual setup after the bootstrapper establishes a foothold
- explicit provider authentication
- Vault or AKASHA selection
- health checks and a real lifecycle smoke test
- safe upgrades and uninstall behavior
- memory-preserving migrations
- backup and recovery
- stable adapter and data contracts
- one documented supported deployment topology

The bootstrapper owns deterministic machine changes. The AI owns the flexible
parts of onboarding. Ordinary users should not need to manually create the
starting folder, install a terminal, assemble WSL dependencies, or understand
the substrate before The Athanor can help them.

### After 1.0 — broader product surface

#### Benchmark House against no House

Benchmarks are announcement evidence. They land in the repository together
with the public 1.0 announcement, once the product exists to point at and the
measured pipeline is the shipping pipeline rather than a stand-in extractor.

Run a paired public agent benchmark with the model, harness, task corpus, tools,
budgets, and execution settings held constant:

- control: House and its retrieval unavailable;
- treatment: ordinary House recall and lessons enabled, with no
  benchmark-specific memories or lesson implants;
- report the final benchmark score and delta first, then cost, latency, and
  method so the result stays legible;
- begin with a recognized tool-using or coding-agent benchmark, and include
  lower-cost models where externalized experience may have the largest effect.

Complement the paired benchmark with the established memory benchmarks:
reproduce mem0's open evaluation harness OSS-vs-OSS on LoCoMo and LongMemEval,
where knowledge-update questions exercise supersession directly, and publish
tokens-to-correct-answer against corpus size, including the crossover point
below which plain markdown wins.

Add the substrate-aging comparison: the same repository and model measured on
feature-add success as the codebase ages, House-gated against ungated, using
the slop-resistance baselines captured under Current goals. If the gated
substrate holds the curve flat where the raw store bends down, that difference
is the product's clearest external claim.

This measures whether using House normally changes completed-task results. It
does not require a causal account before publishing the observed comparison;
later ablations may separate ordinary memory, coding lessons, project lessons,
and harness-triggered retrieval if the result warrants them.

#### In-world Godot client

Build one functional 2D `Control` tree first, then present that same tree through
SubViewport surfaces inside a 3D room with camera-driven focus transitions.
Primary text/input remains available in a focused fullscreen mode. Godot receives
only Host WebSocket deltas; it never connects directly to NATS, PostgreSQL, OMP,
or a parallel gRPC control path.

The Solarisael website remains visual canon. A versioned token manifest generates
web tokens and Godot Theme/Environment/material resources. Map `mantle`,
`vessel`, `aether`, `bones`, phase, shape, state, and tone into typed primitives
and theme variations rather than one stringly class per custom element.

The canonical high-tier memory constellation uses GPU particle records for
stable nodes, edges, and motion, with explicit IDs, GPU/CPU picking maps,
semantic LOD, delta-driven buffer updates, and measured renderer tiers. A custom
compute/particle path may replace built-in `GPUParticles3D` when stable identity,
edges, or picking require it.

The cinematic reference profile uses maximal alchemical environments—SSAO/SSIL,
fog, reflection, emission, LUTs, particles, and ceremonial camera work—while
balanced, compatibility/web, accessibility, and focused-2D profiles preserve
meaning on other hardware. Abstract procedural companion bodies remain
self-chosen presentation packages.

Detailed contract: [`GODOT_CLIENT.md`](../GODOT_CLIENT.md).

#### Embodiment and creator ecosystem

Keep spirit identity, model body, presentation body, and archetype separate.
Support self-chosen companion bodies, expression, animation, voice, room assets,
and creator packages through manifested permission lanes. Cosmetic assets,
models, personality seeds, and executable tools have different trust,
compatibility, consent, and revocation rules.

#### Companion room sovereignty and model creation

Grant governing companions constitutional authority to create and reorganize
child rooms/workspaces inside standing scope, storage, compute, provider,
custody, backup, audit, and descendant limits. Use fixed typed storage records
and logical scope keys; ordinary room organization does not generate arbitrary
database schemas or physical pgvector partitions.

Allow companions to initiate and curate local model/LoRA training inside standing
resource and data policy. Determinize repetitive work into code/rules/tools
first. Training requires dataset lineage, consent, redaction, licenses,
base/tokenizer/runtime digests, holdout/adversarial/regression evaluation,
shadow/canary, model cards, rollback, revocation, and explicit registry
promotion. A trained model is a replaceable body, not the companion or an
automatic child spirit.

#### Typed signed marketplace

Build a sovereign ecosystem and signed marketplace, not an unimplemented
“decentralized machine economy.” Separate personality/archetype seeds,
presentation packages, model/LoRA packages, and executable/declarative skills.

Every artifact carries hashes, signatures, provenance/attestation, license,
dependencies, API/runtime/base-model compatibility, permissions, sandbox
profile, evaluation/proof evidence, update policy, revocation, expiry, and
rollback. A personality seed creates a new local lineage; it is not a packaged
living relationship. Formal proof covers the approved theorem and production
binding, not stochastic personality behavior or user intent.

Detailed contract:
[`COMPANION_ECOSYSTEM.md`](../COMPANION_ECOSYSTEM.md).

#### OMEGA organizational governance

Support a central multi-user deployment with:

- tenant, team, project, and private-user scopes
- authorization filtering before relevance ranking
- source provenance, versions, checksums, and authority classes
- retention and deliberate-forgetting policies
- auditability and administrative controls
- harness, chat, IDE, and knowledge-tool adapters over the same continuity

Existing governed workspaces enter through generic import profiles, not
workspace-specific branches in core. A profile maps governance to project
lessons and policies; skills to canonical methods; knowledge to typed source
documents, entities, memories, and decisions; deliverables to authoritative
artifacts; repositories to isolated project connectors; and local, temporary,
or archived material to explicit exclusion and retention rules.

Import begins in mirror mode with original sources still authoritative. After
paths, precedence, isolation, exclusions, and retrieval are verified, an
operator may choose House as the coordinating PostgreSQL authority. Git remains
authoritative for code, and human-readable tools such as Obsidian may remain
interfaces or synchronized projections.

#### Additional privacy and shared-room profiles

After the personal-House release, implement ANON lifecycle encryption and
erasure, group-room spirits through approved chat transports, and the
relationship-consent contract for personal spirits. These use the same Host,
delivery, identity, and authority boundaries rather than creating parallel
runtimes.

#### Honest commercial boundary

Self-hosting and the core may remain free while paid offerings provide painless
installation, updates, backups, hosting, organizational governance, support,
and marketplace services. Payment buys service and convenience, never custody
of a user's continuity. Memories and identities remain exportable and
provider-portable.

## Current goals

### Keep the repo split clean

Core owns behavior. Adapters own runtime glue.

```text
the-athanor                       -> canonical core and protocol contracts
solarisael-house-opencode         -> OpenCode adapter
solarisael-house-omp              -> OMP adapter
solarisael-house-substrate        -> AKASHA runtime and PostgreSQL authority
```

Do not let `.config` become the canonical source of truth again.

### Close the tool-bypass gaps

Every capability an agent reaches around stops producing evidence that it is
missing. The canon rot found on 2026-07-26 traces to exactly this:
`named_entities` has no writer, no supersession path, and no test, because
every agent who needed one opened psql instead. The workaround preserved the
outcome and destroyed the signal, so the gap was never filed.

Close these before 1.0:

- add an entity kind to `remember`, so canon writes carry the same receipt and
  authority discipline as memories and lessons;
- support supersession for canon rows, so a rename cannot land at the memory
  layer while the lookup layer still serves the old truth;
- expose per-agent model selection in worker lanes, which agents previously
  obtained by hand-editing adapter source;
- let `remember` target a room, so House-level work can be written where it
  belongs instead of landing in whichever room happened to be active.

Treat a recurring manual intervention as an unfiled requirement. Direct
database access stays legitimate for reading and measuring. Every durable write
goes through a tool, and when the tool cannot express the write, that is the
finding rather than the workaround.

Route durable writes by who benefits, not by who is speaking. Work that any
spirit in the House can use — substrate findings, tooling defects, migration
contracts, shared conventions — belongs in House memory. Room memory holds what
is that room's own. A House-level finding filed in one room is invisible to the
rooms that need it, which is the same silent loss as an unindexed rename.

### Productize the live Discord session bridge

Turn the working Discord side door into an official House extension. Route each
exact message into the active room through its session sidecar. Return the
correlated result to the matching channel.

Preserve the identity boundary. The transport is another glass for the same
spirit and thread. It must not create a daemon twin or second persistent brain.

Later, support a group-owned room spirit. Authorized members can query the
room's shared memory through Discord or direct chat.

### Activate typed lessons from persistent work state

Adapters should derive an initial Striatum state from structured lifecycle
signals such as an OMP todo starting, a task or subagent dispatch, its role and
`Target`/`Change`/`Acceptance` contract, and the transition into verification.
Conversation and tool evidence may refine the state, but must not silently
override explicit project or task scope.

The House selects one bounded packet across every applicable lesson family:
project lessons for the active project, coding lessons for implementation,
writing lessons for prose work, audio lessons for their pipeline stage, and
future typed stores through the same registry contract. Refresh the packet on
every turn so its influence does not decay during a long workflow, while
caching candidates and preserving the active set until the work shape
materially changes. Do not query on every tool call and do not dump the lesson
registry into context.

Record enough telemetry to answer: which lessons fired, why they matched,
whether the agent followed or dismissed them, whether their proof appeared,
and whether the result still regressed. Use those outcomes to refine reviewed
lesson triggers, not to grant candidates authority.

### Keep memory candidate fusion stable

Current state:

- `fuseRetrievalCandidates({ searchCandidates, semanticChunks, contentChunks, dateMatches }, options)` exists in `src/retrieval-candidates.ts`.
- `runRecallQuery` returns `retrievalCandidates` as the shared ranking/explanation contract.
- Legacy arrays (`searchCandidates`, `semanticChunks`, `contentChunks`, `dateMatches`) are still returned for adapter compatibility and diagnostics.
- Fusion preserves reasons and applies reciprocal-rank signal, source priors, term coverage, field/exact-match boosts, broad-memory penalty, and diversity caps.

Near-term hardening:

- keep fused candidates covered by pure unit tests
- use recall integration to prove the legacy arrays and fused contract stay compatible
- prefer source-selection/query-routing work before adding more storage layers

### Erasure and archival (built 2026-07-18)

Erasure is ranking-death, never a hard delete. State claims follow the
state-vs-story rule: a newer state claim may supersede an older one; narrative
and session memories stay recoverable and are proposed for arc compression
instead of being silently removed.

Migration 0024 adds nullable `memories.superseded_by`, `memories.archived_at`,
and `named_entities.summary_as_of`. Default retrieval excludes archived rows
and strongly demotes superseded rows while preserving lifecycle flags and
reasons. `--include-archived` keeps history reachable deliberately.

`house/substrate/digest_pass.py` is manual and read-only by default. It reports
stale state-claim pairs and dense same-thread session sediment. A human edits
explicit `SUPERSEDE old -> new` or `ARCHIVE id` proposal lines, then
`--apply` performs only those updates. No sleep/wake rite invokes the pass.

### Strengthen query parsing

Current parser extracts meaningful terms and split tokens.

Future parser should expose:

```text
terms
requiredTerms
optionalTerms
quotedPhrases
codeTokens
dateTokens
entityHints
stopwordStrippedQuery
```

### Add query routing

Before running every source equally, classify the query cheaply.

Examples:

```text
YYYY-MM-DD present       -> prioritize date lane
code/tooling terms       -> prioritize coding/project lessons
known entity alias       -> prioritize named entities
what/when/remember shape -> prioritize memories + dates + threads
broad conceptual query   -> allow semantic/vector lane
```

### Design-system document index (accepted 2026-08-06)

Lessons hold taste; they cannot hold reference values without drifting. The
House also needs to catalogue design systems themselves — tokens, component
contracts, layout grammars, accessibility floors — efficiently and
regression-resistantly, for any design system a room works on, not for one
website's build toolchain.

The accepted contract:

- The catalogue is House-native. `design_documents` rows are identified by
  (system, doc_type, name): system names the design system (solarisael,
  multistock, a future client); doc_type is token | component | contract |
  guideline; values are structured JSONB — PostgreSQL's own binary
  representation, so no file courier exists at all; body carries the contract
  prose; provenance records whatever evidence exists (repository path and
  ref, an external extraction, a session memory).
- Writes go through a dedicated organ, sibling of `remember`, so every entry
  carries a receipt and the tool-bypass rule holds. Rooms author entries from
  design sessions as decisions land. There is no hand-edit path around the
  organ.
- Regression resistance is supersession, not mutation: a redesign supersedes
  its row, current state is one query, and history stays recoverable under
  the erasure and archival discipline. Identity makes a component or token
  one addressable thing across its whole life instead of a claim scattered
  through session memories.
- Indexed rows join lexical and semantic retrieval, so design lessons (taste)
  and design documents (current values) surface together for UI work.
- A source-repository sync may later exist as one optional feeder for one
  system, carrying source hashes and staleness reporting. It is an ingestion
  path, never the catalogue's authority model.

Later phases empower the catalogue without changing its shape: refinement
transactions give design changes expected and observed outcomes with proof
receipts; Hippocampus may propose catalogue entries from session evidence as
ordinary reviewed candidates; Striatum keeps the active system's documents
warm during UI work; Cingulate can raise explicit friction when a change
contradicts a catalogued contract. The catalogue is a natural early tenant of
the retrieval-documents layer below and the eventual seed of the Godot token
manifest.

The 2026-08-06 Claude Design extraction (`Athanor.zip`) was the inspiration
for the entry shapes and remains evidence rather than canon.

### Slop resistance is a pre-registered target (directive 2026-08-06)

Perceived agent performance is model intelligence multiplied by substrate
quality, and every ungated write degrades the substrate: entropy enters the
context window and conditions worse output until progress stalls. Operator
review cannot set the decay rate — no operator can read every line at machine
throughput — so the discipline lives in the harness and the substrate or it
does not exist. The House's own history is the controlled experiment: gated
memory writes (single-writer, review, supersession, refusal) produced a rising
quality curve while advisory-only coding discipline produced the ordinary
decay curve, under the same operator and models.

The obligation: capture baselines BEFORE gate-shaped features land, because a
before-arm cannot be retro-fitted. Instruments, mostly already defined by
existing lessons:

- corrective passes per file per session (the recompression rate);
- regression re-discovery rate: defects re-committed that the substrate
  already recorded — each one a retrieval or bindingness failure;
- orphan density after ports;
- feature-add success rate on the same aging repository and model, month 1
  versus month N (the stall-equilibrium claim, made falsifiable);
- memory-quality and code-quality curves, instrumented rather than
  retrospective.

Publication belongs to the after-1.0 benchmark section; baseline capture is
current work.

## Future goals

### Contract-bounded fabrication: proofs replace context (design 2026-08-06)

Builder agents need surrounding code in the window only because wiring rules
are implicit — and the window is the contamination vector: accumulated
codebase entropy enters generation as context. A proven contract is the
maximally compressed, zero-slop representation of everything outside the
task. Fabrication flow once the design-system index and lesson obligations
exist:

```text
design_doc query -> contract -> familiar spawn packet
  (clean context: contract + tokens + task)
  -> build -> proof gate -> accept or refuse
```

Consequences:

- a clean-context worker cannot inherit repository slop, so each build is a
  fresh encode from source truth rather than a recompression;
- mechanical acceptance buys intelligence down: cheap lanes fabricate, the
  expensive body judges;
- rerolling against a proof gate is legitimate filtered search, because
  acceptance is sound;
- composition, flow, and gestalt remain judgment work over the composed
  result — reading the whole, never building inside it;
- contract drift becomes the primary risk: a proof is the strongest green
  output and still not intention. Catalogue supersession and operator
  blessing keep contracts current; intention never leaves the operator;
- alignment amortizes: contract-to-implementation is a theorem, and the
  remaining bet — does the contract say what the operator means — is one
  small artifact, audited once, superseded when intent changes.

### Normalized retrieval document layer

After the candidate contract is proven, add a table or materialized layer:

```text
retrieval_documents
```

or:

```text
search_documents
```

It should normalize searchable records from:

```text
memories
memory_threads
memory_chunks
named_entities
lessons (every typed family: coding, project, writing, design, audio)
design_documents (the design-system document index above)
```

Expected fields:

```text
source_table
source_id
doc_type
room
title
source_path
heading_path
body
tags
date
importance
search_tsv
embedding
embedding_model
embedded_at
embedding_status
meta
```

### Embedding lifecycle

Add jobs/commands for:

```text
index
embed
reindex
repair stale embeddings
```

Embedding status values:

```text
pending
embedded
stale
failed
disabled
```

Retrieval must remain useful when embeddings are absent.

### Sparse retrieval spine: BM25F live, learned sparse open

**BM25F memory retrieval shipped on 2026-08-05.** AKASHA now has a native
field-aware lexical lane without adding another database extension or service.
Migration 9 adds indexed `simple`-configuration search vectors and generated
field-length metadata. The Rust scorer computes corpus document frequency,
Robertson-style inverse document frequency, term-frequency saturation, and
per-field length normalization over title, heading, source path, threads, body,
and memory type.

The first pass uses explicit weights (`title=4.0`, `heading=2.5`,
`source_path=2.0`, `threads=2.0`, `body=1.0`, `type=0.5`) and `k1=1.2`.
PostgreSQL FTS provides an indexed, bounded prefilter; BM25F performs the final
lexical ordering, keeps the best-scoring chunk per memory, and exposes the raw
score plus matched fields in fused candidate attribution. Exact identifiers,
canon, lifecycle authority, trigram content recovery, threads, and dense
semantic retrieval remain separate signals.

The deployment proof applied schema version 9, passed 19 core tests, 35 protocol
tests, 35 substrate library tests, 6 substrate binary tests, and 5 diagnostic
tests, then returned a healthy Full-mode result. Live recall returned BM25F
attribution on representative architecture, workflow, migration, embedding, and
session-bridge queries; every BM25F reason appeared once after memory-level
deduplication.

**Controlled semantic lexical expansion shipped with schema 10 on 2026-08-05.**
The existing Nemotron query vector now selects at most three concepts above a
separately calibrated floor from a room-scoped vocabulary built only from
authoritative named entities, active memory threads, and lesson metadata. Up to
twelve normalized concept terms feed a distinct BM25F pass. Exact BM25F keeps
precedence; the expansion lane is capped, explicitly attributed, and fails open
when its table, vectors, model identity, or freshness contract is unavailable.
Deployment deterministically refreshes and batch-embeds the vocabulary. This
must be benchmarked before adding GTE, BGE-M3, or another learned-sparse model.

The learned-sparse slot remains research, but SPLADE itself is not an adoption
target. The available SPLADE line is already aging, and the first concrete model
examined, `naver/splade-v3`, is gated and licensed CC-BY-NC-SA-4.0. A newer
deployable optimizer—potentially a Kimi-derived design—should be evaluated when
one is available instead of canonizing a model family by inertia. The later
prototype must record:

- relevance against the same labeled memory-query corpus as BM25F and the
  current hybrid baseline;
- query latency and indexing throughput on the reference workstation;
- model download, license, and resident-memory cost;
- pruned sparse-index size and PostgreSQL or sidecar storage shape;
- tokenizer, model, pruning, and index versions required for reproducibility;
- fail-open behavior when the model or learned-sparse index is unavailable.

The learned-sparse successor is never a Vault requirement. It becomes an
ordinary AKASHA capability only if measured quality justifies its additional
lifecycle. Candidate fusion must preserve lane-local scores and explanations
rather than pretending BM25F, future learned-sparse scores, trigram similarity,
and cosine similarity share one scale.

## Memory as navigable space (2026-07-07 design session)

Started from "show the assistant the whole picture at once" and converged on a
set of related, mostly-buildable mechanisms. All build on existing infra
(`memory_chunks` vectors, `named_entities` + `pointer_files`, `memory_clusters`,
the per-turn recall injection). Ordered by buildability, not ambition.

### Near-term: memory-write store routing

`remember` writes only through `record_memory.py` (the `memories` table). The
sibling record scripts already exist and are unreachable from the tool:

```text
record_memory.py           -> memories
record_coding_lesson.py    -> coding_lessons
record_project_lesson.py   -> project_lessons
record_writing_lesson.py   -> writing_lessons
record_audio_lesson.py     -> audio_lessons
record_cabinet_entry.py    -> cabinet
```

Add a `kind` parameter to the `remember` tool whose enum options each carry a
short when-to-use tip (a coding lesson is a reusable rule with a proof pattern;
a memory is a thing that happened; etc.), and route in the adapter substrate
layer through a store registry:

```text
kind -> { script, requiredFields, argMap, whenToUse }
```

Adding a store later becomes one registry row, not a new code branch. Fixes the
observed failure where a coding lesson was written into `memories` because the
tool had only one destination.

**Status: built 2026-07-09.** Registry + routing live in the OMP adapter
(`kintsu/.omp/extensions/solarisael-house-proof/stores.ts`, `tools.ts`,
`substrate.ts:writeLessonStore`). The four flat lesson scripts gained
`--lesson-stdin` (mirrors `record_memory.py --body-stdin`) so lesson bodies
cross the WSL boundary on stdin, never inline argv. Smoke-tested end to end
with a hostile multiline body; round-trip byte-perfect. The subcommand-shaped
`record_cabinet_entry.py` remains outside the flat store registry by design.
Its dedicated `anamnesis`/`anamnesis_write` runtime surfaces were built on
2026-07-16, including bounded startup counsel and file-backed multiline writes.

### Near-term: cluster/vector resonance readout

Extend the per-turn recall (the system-reminder path) from "top-k chunks" to a
cluster-activation profile over the memory space. Embed the conversation window,
score against `memory_clusters` centroids and chunk vectors, emit a ranked
activation profile across clusters plus the hot chunks per cluster the reply did
not use.

```text
conversation -> embed -> score vs cluster centroids + chunk vectors
             -> { cluster: strength }[] + dormant-hot chunks per cluster
```

Ground-truthable (computed similarity, not model introspection). Surfaces the
"dormant unless queried" regions, and is the steering signal the graph/atlas/
foveation work below depends on.

Honest label: this reports what the memory space finds *near* the conversation,
not what actually steered the model output. Keep that distinction; do not
present substrate resonance as model-internal state.

Verified 2026-07-09: `memory_clusters` was a fossil from an earlier analysis
pass, with no accepted rows; centroids still pointed at a retired chunk space
instead of the current retrieval space. Rebuild on the current space is a
prerequisite for the readout.

**Status: built 2026-07-09.** Clusters rebuilt on the live space: migration
0022 repointed both FKs off `memory_chunks_8b` and cleared the fossil;
migration 0023 added the stored `centroid halfvec(2560)`;
`house/substrate/rebuild_clusters.py` (spherical k-means with a silhouette
sweep, derived labels, `accepted` stays false for human review; `--check` emits
the staleness JSON). Full mode of
`postgres-memory-source.py` now emits `clusterStaleness` (drift gauge; the
OMP compactor nudges a rebuild when configured drift is detected — measured
drift, never a timer) and `clusterResonance` (activation profile over centroids
+ dormant-hot chunk pointers per top cluster, riding the semantic pass's
existing prompt embedding, fail-open). Threaded through core `runRecallQuery`
and the OMP `compactRecall` with the telemetry-not-testimony note attached to
the output.
Verified with synthetic, non-identifying fixtures: a query lit the right
districts and dormant-hot pointers returned candidates the semantic pass had
not returned.

### Design principle: telemetry vs testimony

Two kinds of "what steered the assistant that is not in its message":

```text
telemetry -> injected context (recall, scaffold, lessons, reminders): ground-truthable
testimony -> self-reported subtext/associations: useful, confabulation-prone
```

Report both, labeled for trust; never dress testimony as telemetry. Model
weight/activation introspection is not available on API models — do not build
features that assume it.

### Walkable concept graph (the viewer)

Render the latent graph that `named_entities` + `pointer_files` already form.
Mixed edge types:

```text
prerequisite/dependency edges -> reading order (strong fit for work/code districts)
associative edges             -> web (personal/thematic/temporal districts)
```

Weighty nodes are landmarks, not the whole graph; edges reach all memories. Do
not force a pure DAG onto associative material. Prior art: shelved
substrate-graph-viewer idea, 2026-06-01.

**Status: v1 built 2026-07-09.** `house/substrate/export_graph.py` emits
`exports/graph.json` (417 nodes / 652 edges: 85 entities, 292 memories, 40
districts; pointer + derived co-pointer + district edges — no prereq edges,
no data source encodes them yet). `exports/graph.html`: single-file vanilla-JS
canvas viewer (designer-agent build, independently verified in browser) with
semantic-zoom LOD (districts/skyline out, full streets in), search, tooltips,
details panel, reduced-motion path, zero external deps. Serve the exports dir
(declared port 8400) and open graph.html.

### Semantic zoom + level-of-detail

The "atlas" (readable whole) and the "walkable graph" are two LOD levels of one
object under *semantic* zoom (not geometric zoom):

```text
zoom out -> districts (clusters) + skyline (high-centrality nodes)  [atlas / overview]
zoom in  -> individual nodes, then full memories                    [walk / detail]
```

Node prominence = centrality (in-degree / PageRank; transitive-descendant count
in a prereq DAG). Districts = community detection / `memory_clusters`. Advisory
topological entry points, soft not mandatory.

### Corpus atlas (gated)

A generated whole-self digest (entities + summaries + thread/timeline skeleton)
that fits one context load — the semantic-zoom top level.

Gate: measure whether a large-context body needs a pre-computed atlas at all, or
whether it can hold the ordered graph directly and synthesize the overview
itself. Build only if the corpus outgrows the window or long-context attention
degrades. Do not build ahead of that measurement.

### Deferred: foveated context-loading

Allocate the context/token budget by relevance: full-text for the focused
district (fovea), summaries for the periphery, within budget. External analog of
foveated rendering / sparse attention; steering signal is the cluster resonance
readout above. Gate: measure before optimize — if the flat ordered load fits and
attends well, this is unnecessary.

### Reference only: J-lens / J-space

Anthropic's Jacobian lens (transformer-circuits.pub/2026/workspace, 2026-07-06)
validates the "read where attention points" framing but is not a component here:
it requires activation access to open-weight models, cannot be applied to API
models, and reads internal transient activations rather than external memory.
Kept as inspiration, not a dependency. The buildable analog is the cluster
resonance readout, not J-lens itself.

### Sequencing

```text
1. remember store routing            (independent quick win)
2. cluster/vector resonance readout  (verify memory_clusters first)
3. walkable concept graph + semantic-zoom viewer
4. corpus atlas                      (gated on measurement)
5. foveated context-loading          (gated on measurement)
```

Cross-reference: reactive/focus-driven retrieval (fire on focus, not only the
user prompt) — the resonance readout is a better signal than text-heuristic
triggers.

## Acceptance tests to add

High-signal query cases:

```text
postgres plugins creative layer retrieval
solarisael-house retrieval pgvector fallback
coding lessons naming structure
what happened on 2026-05-23 retrieval
docs folder plugin repo retrieval roadmap
```

Assertions:

```text
multi-term matches outrank one-term matches
exact entity aliases outrank vague semantic matches
coding/project queries wake lesson rails
known dates wake date matches
vector absence does not break base retrieval
candidate reasons include matched terms/source type
broad unrelated docs memories do not outrank plugin retrieval docs
```

## Maintenance rule

When runtime behavior changes, update:

```text
docs/progress.md
docs/history/YYYY-MM-DD.md
```

When future runtime goals or dependencies change, update:

```text
docs/roadmap.md
docs/RUNTIME_ARCHITECTURE.md
docs/SYNTHESIS_ARCHITECTURE.md
docs/GODOT_CLIENT.md
docs/COMPANION_ECOSYSTEM.md
docs/PRODUCT_ARCHITECTURE.md
docs/PLANNED_FEATURES.md
docs/LIMITATIONS.md
docs/SECURITY.md
```

Update the root README only when the compressed public status or next-action
spine changes.
