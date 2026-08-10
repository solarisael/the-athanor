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

## Public retrieval pilot — 2026-07-22

### Contract

Given a distinctive exact memory title, `recall` should place the target memory inside the returned evidence viewport and rank it as highly as possible.

### Setup

- 20 exact-title queries;
- two active rooms;
- real room indexes;
- sanitized aggregate output;
- no private text, titles, paths, or retrieval payloads in the artifact.

### Results

| Measure | Count | Rate |
|---|---:|---:|
| Target present in viewport | 19/20 | **95%** |
| Target ranked first | 16/20 | **80%** |

### What this establishes

The pilot establishes strong retrieval for favorable exact-title queries across two real room indexes. It also demonstrates a privacy-preserving publication format for retrieval measurements.

### Scope

The pilot does not measure paraphrase recall, semantic-only recall, final-answer grounding, cross-room leakage, installation success, or retrieval latency. Those are separate contracts below.

### Artifact

[`adapters/omp/evals/2026-07-22-room-retrieval-pilot.json`](https://github.com/solarisael/the-athanor/blob/main/adapters/omp/evals/2026-07-22-room-retrieval-pilot.json)

## Existing executable contracts

The repository includes automated coverage for core, substrate, and adapter behavior. The public suites cover contracts including:

- generic room discovery and isolation;
- memory JSON source behavior;
- ranking and retrieval candidate fusion;
- query routing;
- core and adapter API compatibility;
- adapter registration and tool schemas;
- runtime smoke behavior;
- project-context selection;
- recall compaction and viewport shaping;
- substrate health classification;
- conversation logging;
- portable bundle layout.

These tests establish implementation contracts. They are not substitutes for public product-quality evaluations over representative corpora and user workflows.

## Next public evidence

The `0.11.0` proof phase expands the evidence surface in this order.

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

**Contract:** The `--migrate-legacy` upgrade from 0.10.x and later upgrades preserve rooms, memories, lessons, authority state, and lifecycle behavior. Migration verifies the new install before activation. It retains a `.athanor-rollback-0.10.x-<timestamp>` directory. An AKASHA migration requires a fresh PGDMP backup receipt. A documented backup restores to a fresh environment and passes the same health and retrieval checks.

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

Open evidence issues in [`solarisael/the-athanor`](https://github.com/solarisael/the-athanor). Name the failing component from
[the canonical component table](./ARCHITECTURE.md#repository-layout-and-component-ownership).
