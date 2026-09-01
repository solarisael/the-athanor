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

## Windows one-owner and one-door proof — 2026-08-31

### Contract

`athanor.exe` owns one in-process multi-room Host. The Host binds one loopback
listener. Every room route starts with `/room/<room-key>`.

### Setup

- Windows 11 Pro x64;
- installed native release `0.5.4`;
- external PostgreSQL authority;
- five configured rooms;
- no test suite;
- optimized native build, installed update, and live runtime probes.

### Results

| Measure | Result |
|---|---|
| Stable app activation | Stable, versioned, and staged `athanor.exe` SHA-256 values match |
| Host processes | One `athanor.exe`; zero `house-host.exe` children |
| Host listeners | One listener on `127.0.0.1:8787` |
| Room health | Five of five paths returned `200` and `status=ok` |
| WebSocket path | Five of five paths opened with the installed bearer token |
| Root route | `/health` returned `404` |
| Unknown room | `/room/__missing__/health` returned `404` |
| Invalid bearer | WebSocket upgrade refused |
| Service children | NATS and Delivery; no Host child |

The release manifest and this table are the sanitized artifacts.

### Boundary

The Godot client remains a presentation child of `athanor.exe`. OMP still owns
its current substrate transport children. The legacy `omp-keeper.exe` process
also remains. PostgreSQL, Ollama, and OMP remain external dependencies.


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

### Local 0.11 parity baseline — 2026-08-10

The pre-convergence repository passed its ordinary isolated development suites on
Windows 11 x64 with Rust/Cargo 1.95.0, Bun 1.3.14, and Python 3.12.10:

| Surface | Result |
|---|---:|
| `cargo test --workspace` | 128 passed; 6 ignored |
| `bun test tests/*.test.ts` | 83 passed |
| `python -m unittest discover -s src -p "test_*.py"` | 57 passed |
| `python -m pytest` under `substrate/` | 26 passed |
| `bun test --max-concurrency 1 --isolate` under `adapters/omp/` | 300 passed; 1 skipped |

The run explicitly removed live Athanor topology variables so development tests
could not touch the active House. The declared Python `test` extra supplied
pytest. This baseline includes the corrected bounded-warning compactor contract
and schema-version-14 release pins.

The ignored PostgreSQL integration suites were not run because no isolated
`ATHANOR_SUBSTRATE_TEST_DATABASE_URL` was configured. The counts above prove
only the ordinary repository contracts; they do not establish database parity,
clean installation, retrieval quality, latency, final-answer grounding, or
end-to-end task improvement.

### Local 1.0 release-candidate proof — 2026-08-10

The `1.0.0-rc.*` strings in this section are retained immutable build identities.
The product version was subsequently corrected to `0.9.3`; these local proofs
establish the named runtime and installer behavior, not complete 1.0 product
maturity.

The converged `1.0.0-rc.1` candidate was exercised on Windows 11 x64 with
Rust/Cargo 1.95.0, Bun 1.3.14, Python 3.12.10, Godot 4.7.1, PostgreSQL 18, and a
test-owned NATS 2.14.4 JetStream endpoint.

| Contract | Observed result |
|---|---:|
| Complete isolated development command | 43 root Bun tests, 12 Python unit tests, 12 substrate pytest tests, 192 Rust tests with 11 ignored, 195 OMP tests with 1 skipped |
| PostgreSQL canon correction/history/active recall | 1 passed |
| GIGA queue and atomic promotion | 1 passed |
| Paper Boat sleep/wake idempotency and room scope | 1 passed |
| PostgreSQL outbox + NATS restart/dedupe/receipt/poison lane | 1 passed |
| Receipt published before Host start and replayed from JetStream | 1 passed |
| Godot project import and scene smoke | passed on Godot 4.7.1 |
| Live Godot Recall Policy screen | authenticated Host snapshot applied |
| Live Godot Paper Boat screen | retained sanitized receipt rendered; no body/title |
| Native payload manifest | 20,643 artifacts; every byte size and SHA-256 matched |
| Packaged Godot executable | launched the staged project and GDExtension |
| Inno Setup installer | compiled successfully |

The release-candidate installer is
`The-Athanor-1.0.0-rc.1-windows-x64.exe`, SHA-256
`9956c29799d606adcfbf6c8e51a0603996ef7cfc4404f38ad293828225140ec0`.
Dependency archives were checksum-pinned before extraction: PostgreSQL 18.4-2,
pgvector 0.8.6, NATS 2.14.4, and Godot 4.7.1.

This establishes local build, contract, live Host/NATS/PostgreSQL integration,
Godot rendering, payload integrity, and installer compilation. It does not
establish a clean-machine elevated install, upgrade from a real 0.10.x
installation, public signing, or external publication.

### Installed RC2 existing-House proof — 2026-08-11

`1.0.0-rc.2` closed the last-mile seams exposed when RC1 was asked to install on
the real Solarisael workstation.

| Contract | Observed result |
|---|---:|
| Complete isolated repository command | passed: 43 root Bun, 12 Python unit, 12 substrate pytest, complete Rust workspace, 197 OMP with 1 skipped |
| Focused native lifecycle | 9 lifecycle, 6 supervisor, and 1 OMP registration unit passed |
| RC2 payload manifest | 20,645 artifacts; every byte size and SHA-256 matched |
| Elevated external-authority installation | passed |
| First external install backup | 119.5 MiB PostgreSQL dump plus manifest written before migration |
| Windows service | `SolarisaelAthanor` reported `RUNNING` and stoppable |
| Runtime children | NATS, delivery, `host:kintsu`, `host:kodo`; no Windows PostgreSQL child |
| Loopback readiness | NATS `4222`, Kintsu `8787`, Kodo `8788` reachable |
| Native doctor | release manifest, 20,645 artifact checksums, service, and persistent data all passed |
| OMP ownership | exactly one stable Program Files loader; development `index.ts`/`hygiene.ts` entries removed |
| OMP client secret | user-only ACL; 64-character token; no token in `config.yml` |
| Installed Host authentication | Kintsu and Kodo each returned the correct independent Recall Policy snapshot |
| Installed Godot launch | native manager launched Godot 4.7.1 for Kintsu with identity/token only in child environment |

The exact installed topology reuses the authoritative WSL PostgreSQL database
and therefore starts no second database on `5432`. Host processes use the real
`kintsu` and `kodo` vault directories, distinct ports, and distinct durable
state directories. OMP room commands select endpoints by exact room key and
refuse a missing entry rather than crossing rooms.

The proven installer is `The-Athanor-1.0.0-rc.2-windows-x64.exe`, SHA-256
`1eff2d67bceed1367c623ed05e5f0c182cb4cbc19d231097c82f50d9434d5174`.
Its setup wrapper now propagates a nonzero native-manager result instead of
reporting a false successful installation.

This closes elevated installation for the existing-House external-database
topology. It does not yet prove a clean generic managed-database installation,
a real 0.10.x upgrade/rollback, or signing.

### Installed RC3 core-resolution proof — 2026-08-11

The first OMP restart against RC2 exposed one packaging omission:
`house-proof/core.ts` correctly resolved the product core through
`ATHANOR_ROOT`, but the release payload had not staged root `index.ts` or its
TypeScript dependencies under `src/`.

RC3 stages the root entry plus 13 runtime TypeScript modules. The native builder
now executes `loadHouseCore()` through the packaged adapter bridge and refuses
the release when `CORE_API_VERSION !== 1` or any transitive module is absent.

This was valid evidence for the immutable RC3 artifact. The `0.9.3` Rust
ownership cut below removes that bridge and its packaged root TypeScript core;
the historical result remains here rather than being rewritten.

The RC2 → RC3 elevated upgrade succeeded against the real external PostgreSQL
authority. The service returned to `RUNNING`; native doctor verified 20,659
artifact hashes and reported installed version `1.0.0-rc.3`. An independent
installed-loader reproduction returned:

```json
{
  "api": 1,
  "index": "file:///C:/Program%20Files/Solarisael/Athanor/versions/1.0.0-rc.3/adapters/omp/index.ts",
  "rooms": ["kintsu", "kodo"]
}
```

The proven RC3 installer is
`The-Athanor-1.0.0-rc.3-windows-x64.exe`, SHA-256
`5ad56051b05a8d27f32ecf1ded876b1f8f68f565ab1a6763911056b576da488f`.

### Installed 0.9.3 thin-adapter proof — 2026-08-12

The source version correction became an installed native update from
`1.0.0-rc.3` to `0.9.3`. The installer preserved RC3 as the rollback version,
registered OMP, returned the Windows service to `RUNNING`, and pointed
`current.json` at `0.9.3`.

The installed payload contains the OMP adapter and its presentation/transport
modules but no root `index.ts` or `src/*.ts` behavioral core. Typed Rust now
owns context classification, keyword/process triggers, context pressure, Recall
viewport policy, routing/familiar receipts, spellbook validation, and subagent
lineage normalization.

Focused proof:

- Rust core/protocol/Host: 88 passed, 2 ignored integration gates;
- OMP adapter: 168 passed, 1 skipped integration gate;
- local substrate deployment: 74 core/protocol, 85 substrate, migration 16,
  fresh PostgreSQL backup, Nemotron dimension 2048, Full-mode health;
- installed Host context command: 35 ms;
- installed loader registration: 5 ms, 30 tools, one context hook;
- installed casual context handler: 341 ms;
- installed work/Recall context handler: 1,951 ms and one bounded Recall tray.

The work-handler smoke exercised the installed loader, Rust context analysis,
Recall Policy Host, Rust retrieval transport, Rust Host viewport, and bounded
OMP presentation. It completed far below OMP's 30-second handler ceiling.

### Installed 0.9.5 operator-client proof — 2026-08-13

The managed native updater activated `0.9.5` from `0.9.4`, retained `0.9.4` as
the previous release, created a rollback backup, and returned
`SolarisaelAthanor` to `RUNNING`. Native doctor verified the release manifest,
all 20,642 packaged artifacts, the Windows service, and persistent data.

The installed Godot 4.7.1 Forward+ client rendered two authenticated live
screens at 1100×760 on the Radeon RX 9070 XT:

- S02 applied Kintsu's Recall Policy snapshot with Host binding, version 881,
  sequence 881, and the current state hash;
- S03 applied the direct routing result with event ID, sequence 881, and the
  four bounded worker lanes.

The frames also proved the English-only pre-i18n copy and sharper native text
contract: stretch scaling disabled, high-DPI enabled, normal hinting, automatic
subpixel placement, and 2× glyph oversampling.

Activation testing exposed and repaired three native lifecycle defects rather
than bypassing them: one-shot delivery health processes did not drain NATS,
delivery readiness had no process-owned stable signal, and durable pre-cutover
Recall receipts used snake_case nested state and decision fields. Final focused
proof passed 83 Rust protocol/Host/installer/delivery tests with three explicit
integration ignores, plus the Godot receipt-state test.

## Next public evidence

The post-1.0 public proof program expands the evidence surface in this order.

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
