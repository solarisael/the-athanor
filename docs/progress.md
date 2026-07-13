# Solarisael House Progress

_Last updated: 2026-07-13 (public documentation complete; Discord extension and lesson-intent routing deferred)_

## Current shape

`solarisael-house` is now the canonical core repo for the Solarisael House runtime.

Runtime adapters live beside it:

```text
C:/Projects/solarisael-house           # canonical core
C:/Projects/solarisael-house-opencode  # OpenCode adapter
C:/Projects/solarisael-house-omp       # OMP adapter
```

The OpenCode adapter still imports core through the local absolute project path. The OMP adapter now resolves the canonical core from its sibling `solarisael-house` folder by default, with `SOLARISAEL_HOUSE_CORE` and `SOLARISAEL_VAULT_ROOT` overrides for portable installations.

## Deferred next threads

- Productize the working live Discord side door as an official session-owned extension: same active OMP conversation and spirit, different glass; never a daemon twin or separate model invocation.
- Add small curated coding-lesson umbrella intents such as `design` and `refactor`, mapped explicitly onto the existing exact lesson taxonomy.

Both are recorded in `docs/roadmap.md`. They are intentionally deferred while Sol rests; neither is active implementation work.

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

Portable OMP test distribution:

```text
C:/Projects/solarisael-house-omp/dist/solarisael-house-portable.zip
```

The archive keeps core and OMP adapter as siblings and ships a public `README.md` in Sol's voice, the exact AI-facing `INSTALL.md`, an identity-writing guide, Apache-2.0 `LICENSE`/`NOTICE` attribution, a developed private-data-free example spirit, an optional appearance template, and a deterministic verifier. Marker-backed rooms may use any filesystem-safe room key with a separate free-form true name and operator; legacy Kintsu/Kodo rooms remain valid. The bundle never modifies OMP configuration automatically and omits credentials, private rooms, memory exports, and substrate data. Optional memory/substrate features retain their external WSL/Python/PostgreSQL prerequisites and fail open when absent.

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

Implemented after restart/reload testing:

- No-embedding candidate term extraction now drops weak conversational glue (`wanna`, `see`, `use`, `without`, `our`, `tool`, `size`, etc.) while preserving technical/search terms.
- Named-entity candidates now carry `kind` and `weighty` into the fused ranking layer.
- Fused ranking now treats `weighty` as conditional promotion, not an absolute throne: exact matches promote, technical/meta/project weighty can promote for technical retrieval queries, and personal/autobiographical weighty is demoted for technical retrieval queries unless explicitly named.
- Focused no-embedding regressions cover filler filtering, technical evidence ranking, and conditional weighty behavior.
- OMP recall context now prefers `retrievalCandidates` from the shared fused candidate contract.
- OMP recall compaction suppresses raw semantic/content chunk arrays when fused candidates exist, reducing noisy context injection while preserving fallback behavior.
- OMP reverse-canon compaction now keeps canon matches only when the query directly names the term/alias or the canon file touches a surfaced candidate path.
- OMP adapter tests now cover adapter label/hooks, tool registration/schema surface, and recall compactor behavior.
- Core, OpenCode adapter, and OMP adapter package metadata remain `0.1.0`; `1.0.0` is deferred until broader runtime verification proves the v1 contract.
- Core now has a pure `query-routing.ts` leaf for query parsing/classification, including raw code/path tokens, original-case entity hints, casual/date/technical intent, and lane decisions.
- Core memory injection gates semantic/content source loading through query routing and keeps date lookup on its existing no-date preflight.
- Core recall honors JSON-only source mode for deterministic tests and passes query-route skip flags to the Postgres helper so full-mode recall can omit semantic/content/date branches without extra WSL spawns.
- The Postgres helper now accepts `--skip-semantic`, `--skip-content`, and `--skip-date` flags that keep full-mode payload keys as empty arrays when skipped.
- OMP auto recall is gated by shared query routing, so low-information casual prompts skip automatic recall while manual recall remains available.
- OMP tests now execute context hooks and representative tool bodies, including room state, routing mode, lane status/dispatch, model defaults, and `remember`/`sleep`/`wake` through temp `SOLARISAEL_SUBSTRATE` scripts.

Not yet implemented / not yet v1-proof:

- Live OMP process reload smoke proving the running extension picks up the synced runtime files and `hygiene.ts` load path.
- Real substrate write/read proof against the production substrate scripts. Current `remember`/`sleep`/`wake` coverage uses isolated temp fake scripts via `SOLARISAEL_SUBSTRATE`.
- Deeper query-routing tuning against live conversation behavior; the first source-selection slice is covered, but the full acceptance matrix is not complete.
- Embedding queue/status/reindex lifecycle.
- Optional ParadeDB/BM25/external search adapter.

Run on 2026-07-09 (AI-guided onboarding slice):

```text
C:/Projects/solarisael-house-omp
bun test tests/adapter-registration.test.ts tests/runtime-smoke.test.ts tests/portable-bundle.test.ts
-> 21 pass, 0 fail, 114 expectations

bun run build:portable
-> dist/solarisael-house-portable.zip
-> 58 archive entries; 12 required publication paths present
-> extracted-bundle verify-install.ts: ok=true, all 19 checks passed
```

Run on 2026-07-04:

```text
C:/Projects/solarisael-house          bun test -> 46 pass, 0 fail
C:/Projects/solarisael-house          bun run tests/recall.integration.ts -> all expectations met
C:/Projects/solarisael-house          python py_compile helpers -> passed
C:/Projects/solarisael-house-omp      bun test -> 12 pass, 0 fail
```

Run on 2026-07-01:

```text
C:/Projects/solarisael-house       bun test tests/retrieval-candidates.test.ts -> 7 pass, 0 fail
C:/Projects/solarisael-house       bun test                         -> 26 pass, 0 fail
C:/Projects/solarisael-house       bun run tests/recall.integration.ts -> all expectations met
C:/Projects/solarisael-house       python py_compile helpers          -> passed
C:/Projects/solarisael-house-opencode bun test                       -> 31 pass, 0 fail
```

Adapter smokes:

- Direct OpenCode adapter import loaded and rendered recall with `Term-aware candidates`.
- Direct OMP adapter import registered label `Solarisael House`, events `context` and `agent_end`, and the expected tools.
- Normal `omp -p` config smoke exited successfully after config was rewired to the new adapter path.

## Current open seam

Next proof should be a live OMP reload/runtime smoke: confirm the active OMP process loads the synced adapter files, confirm `hygiene.ts` is part of the loaded extension set, and then run a production-substrate `remember`/`sleep`/`wake` proof when Sol explicitly wants a real write.

After that, keep tuning query routing against the acceptance matrix and real conversation behavior before any `1.0.0` marker.
