# Solarisael House Roadmap

_Last updated: 2026-07-01_

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
