# The Athanor Evidence

The Athanor states implemented behavior directly and measures contracts whose quality depends on scale, ranking, latency, or environment.

This document is the canonical public evidence index. It separates product behavior, measured results, evaluation scope, and future proof work without interrupting the product README with methodology.

## Evidence rules

Every published evaluation names:

- the exact contract under test;
- the fixture or corpus;
- how queries were constructed;
- sample size;
- storage profile and relevant configuration;
- hardware when performance is measured;
- the scoring rule;
- the result;
- the evaluation boundary;
- the date;
- a sanitized artifact.

Private prompts, memory titles, source paths, excerpts, entities, threads, and raw telemetry never enter public artifacts.

## Dated records

The measured records from 2026-07-22 to 2026-09-07 are in [`history/2026-10-04-evidence-records.md`](./history/2026-10-04-evidence-records.md). Each record keeps its own date. None of them is a current release claim.

## Next public evidence

The public proof program for the current release, `0.5.4` (`package.json:3`), expands the evidence surface in this order.

### Restart continuity

**Contract:** A fresh harness session started from the same room recovers the room identity and a distinctive continuity anchor from the documented room source.

Publish:

- supported host and harness version;
- clean-session procedure;
- number of scenarios;
- recovered room and source;
- failures grouped without private content.

### Paraphrase and entity recall

**Contract:** Queries that do not copy the title still retrieve the intended memory through semantic, lexical, entity, date, or thread evidence.

Publish separate results by query class. Do not merge exact-title and paraphrase performance into one flattering aggregate.

### Correction authority

**Contract:** After a newer state claim supersedes an older claim, ordinary recall selects the current account and retains the older row as deliberate history.

Measure current selection, stale suppression, and historical recovery independently.

### Room isolation

**Contract:** A room-scoped query cannot surface private evidence from another room unless the request uses an explicit cross-room path authorized by the runtime.

Authorization filtering and room resolution are tested before ranking quality.

### Recall latency

**Contract:** Explicit and automatic retrieval remain responsive at declared corpus sizes.

Publish p50 and p95 latency with:

- memory and chunk counts;
- database and index state;
- embedding model and endpoint;
- CPU, GPU, memory, and storage;
- cold versus warm runs;
- query lane composition.


### End-to-end task efficiency

**Contract:** For the same model, harness, task corpus, and success rubric, an
Athanor-enabled run should reduce rediscovery, authority mistakes, user
corrections, or time to the first correct action enough to justify the context
and retrieval it adds.

Publish paired baseline and Athanor runs with:

- total uncached input, cache-write, cache-read, and output tokens when the
  provider exposes them;
- maximum and average active context size;
- retrieval latency and total task elapsed time;
- time and tool calls before the first correct action;
- repeated searches or explanations;
- stale-authority mistakes and user corrections;
- final test, review, or task-success result;
- storage profile and which context organs activated.

Do not collapse short isolated tasks and long-horizon continuity work into one
average. Report them as separate workload classes. Until this evaluation exists,
The Athanor may claim bounded attributed retrieval and continuity behavior, but
not proven net token savings or improved final answers.

### Clean-machine installation

**Contract:** A tool-capable agent can install the supported topology on a clean Windows x64 machine, create the first room, connect OMP, pass the static verifier, recover continuity after restart, and—when selected—complete the AKASHA lifecycle.

Installation evidence reports user intervention, elevation, restarts, elapsed time, and every failed prerequisite.

### Migration, backup, and recovery

**Contract:** An upgrade from an installed release to a later release preserves rooms, memories, lessons, authority state, and lifecycle behavior. The upgrade verifies the new install before activation. An AKASHA migration requires a fresh PGDMP backup receipt. A documented backup restores to a fresh environment and passes the same health and retrieval checks.

No `--migrate-legacy` mode exists. The installer CLI offers `install`, `update`, `install-omp-adapter`, `rollback-omp-adapter`, `doctor`, `rollback`, `uninstall`, and `purge` (`crates/athanor-install/src/cli/manage.rs:32-134`). A legacy 0.10.x tree enters through the one-time `import_legacy_once` step (`crates/athanor-install/src/installer.rs:282,1351-1377`).

### Final-answer grounding

**Contract:** When retrieved evidence is relevant, the final answer uses the correct current source, respects authority and supersession, and does not invent unsupported detail.

This requires a validated judge or human-reviewed rubric. Retrieval presence alone is not answer quality.

## Evidence boundaries

The following do not become public product-quality claims:

- an aggregate from an unstable or unvalidated judge;
- a mocked tool result;
- configured files without an observed lifecycle;
- the existence of embeddings without retrieval measurement;
- model testimony about what influenced it;
- private anecdotes published without a reproducible contract.

House distinguishes ground-truthable telemetry from model testimony. Injected context, selected sources, rankings, and lifecycle results are telemetry. A model's description of its own hidden associations is testimony.

## Contributing evidence

A useful external evaluation should provide:

1. a precise contract;
2. a sanitized reproducible fixture;
3. the release version and the storage profile;
4. environment details;
5. raw machine-readable results without private content;
6. a short interpretation that stays within the measured scope.

Open evidence issues in [`solarisael/the-athanor`](https://github.com/solarisael/the-athanor). Name the failing process from
[the process table](./ARCHITECTURE.md#2-processes-on-one-machine).
