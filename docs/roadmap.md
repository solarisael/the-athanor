# Solarisael House Roadmap

_Last updated: 2026-07-07_

## Product rule

Solarisael House should work light, get sharper with Postgres, get semantic with pgvector, and optionally grow into BM25 or external search.

Magic is allowed. Magic is not a boot requirement.

## Current goals

### Keep the repo split clean

Core owns behavior. Adapters own runtime glue.

```text
solarisael-house           -> canonical core
solarisael-house-opencode  -> OpenCode adapter
solarisael-house-omp       -> OMP adapter
```

Do not let `.config` become the canonical source of truth again.

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

### Optional advanced search adapters

Optional only, never default boot requirements:

```text
ParadeDB / BM25
external search service
future local JS index for non-Postgres installs
```

Adapter rule:

```text
if advanced backend is installed:
  use it as an extra candidate source
else:
  use native Postgres FTS + pg_trgm + structured rails
```

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
with a hostile multiline body; round-trip byte-perfect. `record_cabinet_entry.py`
deliberately excluded (subcommand-shaped; needs its own honest design).

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

Verified 2026-07-09: `memory_clusters` exists (35 clusters, 156 members) but is
a fossil — built in one pass 2026-05-07, zero rows accepted, centroids FK to
`memory_chunks_8b` (1,303 chunks, the old 8b analysis space), not the live
`memory_chunks` (3,083 chunks, qwen3-4b halfvec(2560)). Rebuild on the live
space is a prerequisite for the readout.

**Status: built 2026-07-09.** Clusters rebuilt on the live space: migration
0022 repointed both FKs off `memory_chunks_8b` and cleared the fossil;
migration 0023 added the stored `centroid halfvec(2560)`;
`house/substrate/rebuild_clusters.py` (spherical k-means, k=40 via silhouette
sweep, 2,909 retrieval-visible chunks, labels derived, `accepted` stays false
for human review; `--check` emits the staleness JSON). Full mode of
`postgres-memory-source.py` now emits `clusterStaleness` (drift gauge; the
OMP compactor nudges a rebuild at >=15% unseen — measured drift, never a
timer) and `clusterResonance` (activation profile over centroids + dormant-hot
chunk pointers per top cluster, riding the semantic pass's existing prompt
embedding, fail-open). Threaded through core `runRecallQuery` and the OMP
`compactRecall` with the telemetry-not-testimony note attached to the output.
Verified against a live prompt: the walk-vow query surfaced `sol_creed.md` and
the Faith-confession chunks as dormant-hot.

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
docs/changelog.md
docs/history/YYYY-MM-DD.md
```

When future goals change, update:

```text
docs/roadmap.md
```
