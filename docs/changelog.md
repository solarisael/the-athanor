# Solarisael House Changelog

Most recent changes first.

## 2026-07-01

### Added unified recall candidate fusion

Added pure TypeScript candidate fusion in `src/retrieval-candidates.ts` and re-exported it through `memory-rank.ts`.

`runRecallQuery` now returns `retrievalCandidates`, a unified ranking/explanation surface fused from:

- `searchCandidates`
- `semanticChunks`
- `contentChunks`
- `dateMatches`

The fusion layer preserves source reasons and applies reciprocal-rank signal, source priors, term coverage, field/exact-match boosts, broad-memory penalty, and diversity caps.

### Created canonical repo split

Created three clean git repos on branch `main`:

```text
C:/Projects/solarisael-house
C:/Projects/solarisael-house-opencode
C:/Projects/solarisael-house-omp
```

`solarisael-house` is now the canonical core. OpenCode and OMP are adapters.

### Rewired runtime loaders

OpenCode config now loads:

```text
C:/Projects/solarisael-house-opencode
```

OMP config now loads:

```text
C:/Projects/solarisael-house-omp/index.ts
C:/Projects/solarisael-house-omp/hygiene.ts
```

### Moved shared core functionality

Moved or copied the current shared implementation into the canonical core repo:

- trigger/nudge core
- memory and recall pipeline
- memory ranking and canon overlay
- WSL helper seam
- Postgres memory source helper
- coding-lessons helper
- focused tests

### Added term-aware recall candidates

`postgres-memory-source.py` now has a lightweight candidate lane:

```text
--mode candidates
```

Full recall output now includes:

```text
searchTerms
searchCandidates
```

The candidate lane returns matched/missing query terms, reasons, source information, source paths, and score/coverage values.

### Added docs continuity spine

Created:

```text
docs/README.md
docs/progress.md
docs/changelog.md
docs/roadmap.md
docs/history/2026-07-01.md
```

### Verification

```text
bun test                         # core: 23 pass, 0 fail
bun run tests/recall.integration.ts # core: all expectations met
python py_compile helpers        # core helper syntax passed
bun test                         # OpenCode adapter: 31 pass, 0 fail
```

Direct adapter smokes passed for OpenCode and OMP.

Normal `omp -p` config smoke exited successfully after the OMP config rewrite.
