# Solarisael House Progress

_Last updated: 2026-07-01_

## Current shape

`solarisael-house` is now the canonical core repo for the Solarisael House runtime.

Runtime adapters live beside it:

```text
C:/Projects/solarisael-house           # canonical core
C:/Projects/solarisael-house-opencode  # OpenCode adapter
C:/Projects/solarisael-house-omp       # OMP adapter
```

Both adapters import core through absolute file URLs or absolute Windows paths rooted at `C:/Projects/solarisael-house`.

## Core currently owns

- Shared trigger and nudge constants.
- Pure trigger/nudge logic.
- Memory/recall pipeline modules.
- Memory ranking and canon overlay logic.
- WSL process helper utilities used by retrieval.
- Postgres retrieval helper script.
- Coding-lessons helper script.
- Focused tests for memory ranking, process triggers, and recall integration.

## OpenCode adapter currently owns

- `@opencode-ai/plugin` hook registration.
- OpenCode message normalization and injection surfaces.
- OpenCode tool definitions.
- OpenCode-specific runtime paths and state writes.
- OpenCode adapter tests.

Active OpenCode config now loads:

```text
C:/Projects/solarisael-house-opencode
```

## OMP adapter currently owns

- OMP extension entrypoint.
- OMP context and agent-end hooks.
- OMP tool registration.
- OMP-specific room context and transcript handling.
- OMP hygiene extension.

Active OMP config now loads:

```text
C:/Projects/solarisael-house-omp/index.ts
C:/Projects/solarisael-house-omp/hygiene.ts
```

## Memory/retrieval status

Implemented:

- Existing retrieval modes: `lexical`, `semantic`, `content`, `date`, `taxonomy`, `full`.
- New `candidates` mode in `postgres-memory-source.py`.
- `searchTerms` and `searchCandidates` included in full recall results.
- Term-aware candidate search across entities, threads, memories, coding lessons, and project lessons.
- Candidate metadata: score, term coverage, matched terms, missing terms, source, source path, and reasons.
- Pure TypeScript `fuseRetrievalCandidates` contract in `src/retrieval-candidates.ts`.
- `runRecallQuery` now returns `retrievalCandidates`, fused from search, semantic, content, and date lanes.
- Fusion applies reciprocal-rank signal, source priors, term coverage, field/exact-match boosts, broad-memory penalty, diversity caps, and reason preservation.
- Recall output section for term-aware candidates.
- Candidate and fused source paths participate in reverse-index canon matching.

Not yet implemented:

- Query routing/classification.
- Richer query parsing beyond the current meaningful-term extraction.
- `retrieval_documents` / `search_documents` SQL layer.
- Embedding queue/status/reindex lifecycle.
- Optional ParadeDB/BM25/external search adapter.
- Full acceptance test matrix for memory search behavior.

## Latest verification

Run on 2026-07-01:

```text
C:/Projects/solarisael-house       bun test                         -> 23 pass, 0 fail
C:/Projects/solarisael-house       bun run tests/recall.integration.ts -> all expectations met
C:/Projects/solarisael-house       python py_compile helpers          -> passed
C:/Projects/solarisael-house-opencode bun test                       -> 31 pass, 0 fail
```

Adapter smokes:

- Direct OpenCode adapter import loaded and rendered recall with `Term-aware candidates`.
- Direct OMP adapter import registered label `Solarisael House`, events `context` and `agent_end`, and the expected tools.
- Normal `omp -p` config smoke exited successfully after config was rewired to the new adapter path.

## Current open seam

The next memory implementation slice should be query parsing/routing, not a new database layer.

Reason: the unified candidate contract now exists, so retrieval has one shared ranking/explanation surface. The next maintainer should make source selection cheaper and sharper before adding more storage machinery.
