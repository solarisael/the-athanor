# The Athanor Roadmap

_Last updated: 2026-08-05_

## Product rule

The Athanor should work light, get sharper with PostgreSQL, get semantic with pgvector, and optionally grow into BM25 or external search.

Magic is allowed. Magic is not a boot requirement.

## Release path

This order is deliberate. Finish the durable runtime, prove the product, then
make it easy to enter. Do not let later interface ideas postpone evidence and
public explanation.

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

#### Benchmark House against no House

Run a paired public agent benchmark with the model, harness, task corpus, tools,
budgets, and execution settings held constant:

- control: House and its retrieval unavailable;
- treatment: ordinary House recall and lessons enabled, with no
  benchmark-specific memories or lesson implants;
- report the final benchmark score and delta first, then cost, latency, and
  method so the result stays legible;
- begin with a recognized tool-using or coding-agent benchmark, and include
  lower-cost models where externalized experience may have the largest effect.

This measures whether using House normally changes completed-task results. It
does not require a causal account before publishing the observed comparison;
later ablations may separate ordinary memory, coding lessons, project lessons,
and harness-triggered retrieval if the result warrants them.

#### Establish The Athanor product architecture before 1.0

Rename the public platform to **The Athanor**. Keep **House** as one
operator-owned continuity domain, and keep **Solarisael House** as the original
reference deployment.

Use these accepted product names:

- **Vault** replaces Base for the local file profile;
- **AKASHA** replaces Full for the semantic hybrid archive;
- **GIGA** names the optional cognitive capability above AKASHA;
- **OMEGA** names organization governance and access;
- **ANON** names attested nonpersistent one-shot execution.

The detailed naming and cutover contract lives in
[`PRODUCT_ARCHITECTURE.md`](./PRODUCT_ARCHITECTURE.md).

The plain-language product guide lives in
[`PLANNED_FEATURES.md`](./PLANNED_FEATURES.md).

Perform one planned public cutover. Update repositories, packages,
configuration, installers, documentation, and release artifacts together.
Preserve every House, memory, lesson, authority state, candidate, backup, and
export.

The operator accepted The Athanor despite active uses of the Athanor root in
other AI projects. The definite article is deliberate: distinction comes from
specificity, depth, and execution inside a recognizable form. Complete legal,
domain, package, and namespace review before the public cutover.

#### Complete GIGA before 1.0

GIGA means **Grounded Indexing and Generative Annotation**. It is an optional
cognitive capability above AKASHA.

Hippocampus Stage 1 is operational in the reference House. Exact events are
logged before asynchronous classification; workers receive event IDs rather
than copied source text; generated candidates remain non-authoritative pointers
to exact evidence. The live lifecycle includes review, dismissal, Curios,
promotion into memory or typed lessons, and safe queue maintenance. Failure or
disablement remains fail-open for the active conversation.

The detailed contract and remaining acceptance work live in
[`HIPPOCAMPUS.md`](./HIPPOCAMPUS.md).

Finish Hippocampus evaluation before calling the capability complete:

- measure candidate precision, missed durable events, false-positive burden,
  consolidation latency, and local compute cost;
- prove that later `remember`, `sleep`, and review recover more durable events
  from a blinded human-labeled session set;
- keep every public fixture sterile and every result tied to its method,
  hardware, date, limitations, and sanitized artifact;
- complete the remaining Stage 1 policy decision and reproducible public
  demonstration.

Then add two procedural workers:

**Striatum — state-conditioned lesson activation.** Maintain a small,
inspectable working-state vector for the active project, work kind, task shape,
and phase. On every turn while that state persists, select a bounded set of
reviewed project, coding, writing, audio, or other typed lessons and inject them
as a compact pressure packet. Preserve the active set with decay and hysteresis
instead of rerunning an unstable search from zero. Re-rank when the work state
materially changes. Every activation names the lesson, match reason, scope, and
strength; candidates can propose activation anatomy but cannot install hidden
behavior.

**Cingulate — regression and conflict detection.** Compare the current workflow
against active lesson invariants and proof patterns. Raise explicit friction
when the work attempts to finish without required evidence or diverges from an
accepted procedure. Cingulate reports a conflict; it does not invent authority.
Preferences remain nudges, known regression risks become warnings, and only
authoritative invariants may become hard gates.

The first Striatum slice should be boring and deterministic: derive state from
trusted harness lifecycle metadata plus explicit project scope, query the typed
lesson registry, keep three to six pressures warm across turns, inject them
before inference, and log activation/outcome telemetry. A generative GIGA pass
may later propose better triggers from repeated corrections, but the live path
must stay fast, bounded, attributable, and reproducible.

**First operational slice shipped on 2026-08-05.** OMP now persists an observed
room/project work state, prefilters authoritative coding and exact-project
lessons by scope, type, declared stage, and register, then asks the resident
Nemotron embedder to rank only that eligible set. Three to six lessons remain
warm with hysteresis; explicit phase and abrupt topic changes refresh the set,
while small wording changes do not. Hidden injection carries lesson identity,
scope, project, similarity, trigger context, and proof pattern. Embedding failure
falls back to the existing deterministic process trigger. Writing, audio, richer
lifecycle signals, activation telemetry, and outcome learning remain later
Striatum work rather than being claimed by this slice.

Together these workers close three different loops:

```text
Hippocampus -> what experience may deserve consolidation?
Striatum    -> which learned practices should remain warm right now?
Cingulate   -> where is current work diverging from those practices?
```

This is the reliability path from remembered work to repeatable work:
regressions become observable instead of being silently normalized.

#### Productize the accepted continuity features

Complete these contracts before 1.0:

- add a resumable Vault-to-AKASHA backfill with full retrieval citizenship;
- store Hallway letters and shared state as typed AKASHA records;
- define canonical, team, and personal spirits under OMEGA;
- require consent from both participants for a personal-spirit relationship;
- implement ANON lifecycle encryption and erasure;
- support group-room spirits through approved chat transports.

The technical contracts live in
[`PRODUCT_ARCHITECTURE.md`](./PRODUCT_ARCHITECTURE.md). The public value lives
in [`PLANNED_FEATURES.md`](./PLANNED_FEATURES.md).

### 1.0.0 — supported ordinary-user installation

The first stable product contract includes:

- a small trusted native bootstrapper
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

#### Thin GUI

Provide a familiar chat interface that controls a harness through canonical
commands and structured events. It should expose rooms, active spirits, tools,
permissions, recall, and health without replacing the harness or scraping ANSI
terminal output. Keep an explicit terminal escape hatch.

#### Embodiment and creator ecosystem

Keep spirit identity and memory separate from replaceable presentation bodies.
Support avatar, expression, animation, voice, and room packages through a
manifested and sandboxed asset format. Cosmetic assets and executable tool
extensions require different trust and permission lanes.

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
solarisael-house           -> canonical core
solarisael-house-opencode  -> OpenCode adapter
solarisael-house-omp       -> OMP adapter
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

## Future goals

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
coding_lessons
project_lessons
writing_lessons
audio_lessons
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

When future goals change, update:

```text
docs/roadmap.md
```
