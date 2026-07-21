# Solarisael House Changelog

Most recent changes first.

## 2026-07-20
### Bundled coding lesson starter pack

Full House now includes a privacy-safe starter pack with 14 reusable engineering lessons.

The substrate importer validates schema version 1, previews packs without a database, and imports all lessons in one transaction.

The default import preserves existing lessons with the same scope, project, and title. An explicit flag permits a chosen update.

The Full House installation flow now offers this pack after substrate health passes.

Verification: four importer tests passed. The bundled pack also passed its command-line dry run.

### Agent-facing installation contract

The public README and installation guide now tell repository-reading agents not
to default to Base House merely because Full has more prerequisites, invent a
trial period, ask for a private maintainer bundle, or treat OMP native memory as
an equivalent replacement from feature-name overlap. The operator chooses the
desired capability and consequential tradeoffs. The installing AI owns command
translation, configuration, execution, and verification.

The canonical substrate README now states its public distribution boundary
explicitly: migrations, Python dependencies, environment template, health
probe, lifecycle smoke, memory tools, and backup/restore are public.

Two independent agent interpretations of the revised README selected no Base
default, no private bundle, AI-owned setup, and no native-memory equivalence.

### Daily-use recall measurement

Tuner now opts into room-local recall telemetry. Each real production turn
records the actual compact recall viewport, route, suppression diagnostics,
session ID, and a SHA-256 message join key under `.omp/runtime/`. Raw user text
is not duplicated; the existing OMP session log remains authoritative.

`run-daily-usage.ts` joins those records to real session turns, with a room-local
prompt-hash fallback for runtimes that omit the session ID. Its replay mode uses
the saved conversation prefix and captured viewport while disabling tools to
prevent future filesystem leakage. Reports and sealed mappings remain in the
private room runtime directory. Sol or an explicitly delegated blinded reviewer
provides the final judgment.

No daily-use score is reported until post-instrumentation turns exist and Sol
has judged enough blind pairs.

### Synthetic single-turn memory diagnostic

The new scenario runner exercises production query routing, recall lanes,
compaction, and automatic viewport filtering. It then uses an isolated answer
call and a separate rubric-based judge call to attribute missing evidence,
stale claims, unsupported claims, and selection failures.

This runner is not the primary memory-quality measurement. It does not reproduce
normal Solarisael use: it omits the real conversation prefix, disables tools,
forces JSON citations, asks synthetic memory questions, and prevents the model
from deciding to use explicit recall or other tools. Its results are useful as a
viewport diagnostic only.

The former exact-memory-ID benchmark is now
`run-id-ranking-evals.ts` with `legacy-id-ranking-queries.json`. Its recall@k
and MRR values measure historical-row ranking only. They do not measure memory
quality. The previously reported 76.9% recall@1 and 94.9% recall@10 values are
therefore withdrawn as quality claims.

The first synthetic run used six scenarios and 39 answer-plus-judge trials. Its
6/18 full-recall pass count and 0/18 no-recall count are retained in the report
for diagnostic audit, but withdrawn as daily-use quality figures. Both answer
and judge invocations used `openai-codex/gpt-5.6-sol`.


### Guarded lesson updates

Coding and project lessons now have an explicit `update_lesson` operation that
preserves the row ID. Updates require the numeric ID and exact current title,
lock the row before mutation, accept only allowlisted patch fields, and preserve
every omitted field—including formal negation links. Lesson bodies cross the
Windows/WSL boundary through stdin. The previous same-title update claim was
removed from `remember` documentation because new writes do not provide an
identity-preserving update contract.


### Public Full House substrate cutover

The optional Full House backend now has a canonical public home at
[`solarisael/solarisael-house-substrate`](https://github.com/solarisael/solarisael-house-substrate).
That repository owns the PostgreSQL migrations, local-embedding contract,
health probe, lifecycle smoke, and backup/restore procedures. The House README
and installation guide now send Full operators there instead of carrying a
copied or private-path-dependent bootstrap procedure.

The OMP adapter now distinguishes Base, Full, and configured-but-degraded
substrate states. Full requires a live `health.py` verdict plus exact
`substrateApi=1`, `coreApi=1`, and `adapterApi=1` compatibility. The portable
verifier executes that health probe rather than accepting file presence as
proof, while Base remains complete with no substrate configured.

Verification: the real write/embed/wake/delete lifecycle passed against
PostgreSQL and `qwen3-embedding:4b`; the live substrate health verdict returned
Full; 54 core tests and 57 OMP adapter tests passed; the portable archive built;
and the verifier passed both Base and live Full modes.


### OMP lesson-store routing repair

The OMP `remember` lesson branch now imports its store registry and argument
builder explicitly. Previously, ordinary memory writes succeeded while every
typed lesson write reached an undefined `REMEMBER_STORES` symbol at runtime.
A focused adapter regression executes the lesson branch through store
validation so registration-only tests cannot miss this failure again.

## 2026-07-18

### Erasure and archival layer

Migration 0024 adds nullable `memories.superseded_by`, `memories.archived_at`,
and `named_entities.summary_as_of` columns. `record_memory.py --supersedes <id>`
is repeatable, so a new current-state memory can mark every row it replaces as
superseded in the same write. Retrieval hard-demotes superseded rows and
excludes archived rows by default; `--include-archived` opts into the older
trail. Missing archival columns fail open rather than making retrieval a boot
requirement.

`digest_pass.py` is a manual digestion pass. It proposes supersede/archive sets
for review, and `--apply` executes only the sets a human accepts. Nothing is
hard-deleted: erasure is ranking-death, not row-death. The state-vs-story rule
is explicit: current-state claims may be superseded, while narrative memories
are compressed into arcs by proposal rather than silently rewritten.

### Legacy historical-row ranking harness

The original `evals/` harness carries 45 queries pinned to historical database
row IDs. It is retained as `bun evals/run-id-ranking-evals.ts` for narrow
ranking diagnostics. The checked-in 2026-07-18 output remains a historical
artifact, not a memory-quality baseline. Exact-row recall@k and MRR cannot
account for supersession, evolving authority, automatic multi-lane context, or
the model's final evidence decision.

## 2026-07-16

### Anamnesis Cabinet runtime

The previously seeded Anamnesis Cabinet is now connected to the Solarisael
House runtime. The canonical core exposes `runAnamnesisQuery` with deterministic
`wake` and `consult` modes scoped to the current room plus shared House scope.
Wake returns wake pillars and active wake cycles; consult uses Postgres
full-text, trigram, title, shape, tag, canon-link, and drawer-text evidence.
Both modes are bounded and attach only the latest three cycle repetitions in
chronological order.

OMP now injects Cabinet wake counsel once per fresh session under the dedicated
`solarisael-anamnesis-wake` custom type. It remains separate from paper-boat
wake, suppresses duplicates, caps automatic context at 8,000 characters, and
fails open. The injected framing distinguishes pillars from cycles and requires
loaded cycle claims to be verified against the live turn.

Manual `anamnesis` and approval-gated `anamnesis_write` tools provide explicit
consultation and dedicated drawer/repetition writes without flattening the
Cabinet into the generic `remember` registry. The adapter preserves the
production writer's hard refusals, and all multiline drawer and repetition
bodies now cross the Windows/WSL boundary through temporary UTF-8 files.

Verification: Python compile checks passed; 53 canonical-core tests and 45 OMP
adapter tests passed. Live Postgres wake and multi-term consult succeeded.
Live writer `add` and `append-rep` dry runs accepted the file-backed bodies.
A fresh-session runtime smoke observed one hidden, bounded Cabinet injection,
same-session duplicate suppression, and a new wake in a second session.


## 2026-07-09

### AI-guided installation and first-room onboarding

The portable OMP bundle now treats the AI as the installation and onboarding
guide rather than shipping a traditional installer. The canonical core now has
a public `README.md` written in Sol's voice for people who say good morning,
thank you, and love you to their AI. Exact environment, dependency, additive
OMP configuration, room creation, first-session, restart-continuity, optional
substrate-memory, removal, and receipt contracts moved to `INSTALL.md`.
`IDENTITY_GUIDE.md` explains true name versus room key, identity prose, hard
constraints, drift repair, examples, continuity placement, and optional
appearance without encouraging copies of Kintsu or Kodo.

Both core and OMP repositories now declare Apache-2.0 and carry a `NOTICE`
requiring applicable attribution to the original project and design by Sol.
Package metadata names Sol as author. The portable archive carries the public
docs and attribution files at its root and inside both runtime trees.

The private-data-free `starter-room/example` contains Mica, a deliberately
fictional developed identity, `.solarisael-room.json`, `AGENTS.md`,
`room_summary.md`, and an optional appearance template. The portable builder
ships those files plus `verify-install.ts`, which validates the sibling runtime
trees, marker/header/context contracts, and exact OMP extension paths before a
fresh-session `room_state` proof.

OMP room resolution now accepts marker- or `active_spirit.md`-backed arbitrary
room folders. The folder basename remains the stable room key; marker
`trueName` and `operator` supply free-form display identities. Generic
`set_room_state` updates preserve the authored spirit body, while legacy
Kintsu/Kodo folders retain their behavior.

Focused verification: 21 tests passed with 114 expectations across adapter
registration, runtime smoke, portable archive, and verifier success/failure
contracts. The rebuilt archive also passed an extracted-bundle verifier run
with all 19 checks green; 12 required publication/attribution paths were
confirmed among 58 archive entries. SHA-256:
`a1c081b3d451b84266ef7ccb4ce05ecb821cf232a5ea09215293ce1dc328960d`.

### Portable OMP test bundle

The OMP adapter no longer depends on `C:/Projects/solarisael-house` or
`C:/Solarisael/Obsidian/obsidian` at runtime. It resolves the canonical core
from a sibling `solarisael-house` folder by default, accepts
`SOLARISAEL_HOUSE_CORE` and `SOLARISAEL_VAULT_ROOT` overrides, and keeps the
hygiene extension on the same portable vault-root contract.

`bun run build:portable` now produces
`dist/solarisael-house-portable.zip`: core + OMP adapter sibling folders,
private-data-free starter room, and manual `SETUP.txt`. The bundle deliberately
does not edit OMP configuration or include credentials, private rooms, memory
exports, or substrate data.

Focused portability/runtime verification: 11 tests passed with 80 expectations.
Archive inspection confirmed both runtime trees, starter room, and setup guide;
SHA-256 `009d8fb38b093bb56a8bc0398910be92e2095896a7b4e60bb20f78206f81344b`.

### Memory-write store routing (roadmap near-term item 1)

The `remember` tool now routes by `kind` (`memory` default, `coding-lesson`,
`project-lesson`, `writing-lesson`, `audio-lesson`) through a store registry
(`kind -> { script, required, argMap, whenToUse }`) in the OMP adapter
(`stores.ts` + `writeLessonStore` in `substrate.ts`). Unknown-for-kind fields
and missing required fields refuse loudly, naming the accepted set.
`record_cabinet_entry.py` deliberately excluded (subcommand-shaped).

### Lesson scripts accept stdin bodies

`record_coding_lesson.py`, `record_project_lesson.py`, `record_writing_lesson.py`,
and `record_audio_lesson.py` gained `--lesson-stdin` (exactly-one validation with
`--lesson`), mirroring `record_memory.py --body-stdin`, so multiline lesson text
crosses the WSL boundary safely.

### Verification

Registry contract exercised via Bun import (happy path, unknown-field refusal,
missing-required refusal, empty-value elision). End-to-end routed write through
`wsl.exe` -> `--lesson-stdin` -> postgres with a hostile multiline body
(quotes, `$`, backslash, blank lines): round-trip byte-perfect; sentinel row
deleted after verification. `memory_clusters` audited for roadmap item 2:
stale one-pass build from 2026-05-07 on the 8b space; rebuild required.

### Cluster rebuild on the live embedding space

Migration 0022 repointed `memory_clusters`/`memory_cluster_members` FKs from
`memory_chunks_8b` to live `memory_chunks` and cleared the 2026-05-07 fossil.
Migration 0023 added a stored `centroid halfvec(2560)`. New
`house/substrate/rebuild_clusters.py`: spherical k-means over retrieval-visible
chunks (visibility policy mirrored from retrieval), silhouette-swept k,
derived labels, `accepted=false` preserved for human review, `--check`
staleness JSON. Built: k=40 over 2,909 chunks.

### Cluster staleness gauge + resonance readout

`postgres-memory-source.py` full mode emits `clusterStaleness` (build age vs
chunks embedded since) and `clusterResonance` (cosine activation profile over
stored centroids + dormant-hot chunk pointers, riding the semantic pass
embedding, fail-open). Core `runRecallQuery` passes both through; the OMP
recall compactor surfaces a drift nudge (fires at >=15% unseen or never-built,
silent when fresh) and the resonance block with an explicit
telemetry-not-model-internal-state note. Verified on a live prompt.

### Per-dispatch model choice in house routing

`DispatchRequest` gained optional `model`; receipts and task packets now carry
a resolved spawn model (explicit override wins, else lane role maps via
`OMP_MODEL_ALIASES`: pi/smol -> smol, pi/default -> default, pi/slow -> slow).
The v0 "metadata only" shrug-warning is replaced by an actionable one: model
selection is enforced by spawning via the eval `agent()` helper with the
packet's model; the plain `task` tool ignores it. `house_dispatch` tool schema
gained the optional `model` param. Routing tests: 7/7 pass.

### Walkable concept graph v1 (roadmap item 3)

`export_graph.py` renders the latent graph (named_entities pointer_files +
memory_clusters districts + derived entity co-pointer edges) to
`exports/graph.json`; `exports/graph.html` is a self-contained canvas viewer
with semantic-zoom LOD, built by a delegated designer agent and independently
verified in a live browser (417 nodes, both LOD bands, search/tooltip/panel,
only ./graph.json fetched, console clean). Viewer port convention: 8400.

### Cross-room memory handles (decency architecture)

`memory://<room>/<id-or-source-path>` now resolves house-wide: `--mode fetch`
in `postgres-memory-source.py` (pk or newest-by-path SELECT, provenance
stamped, wrong-room claims warned, db-only visibility bypassed — a handle is
a deliberate citation), detected in core `runRecallQuery` before routing,
body clipped at 6KB in the OMP recall compactor. Sol's ruling: ambient
search stays room-scoped (rooms keep personalities); explicit handles cross
rooms on purpose — a knock, not a key. Requested by Kintsu after failing to
fetch kodo memory #2388 cross-room. Verified: id + path + cross-room-from-
kintsu + miss + wrong-room-warning, CLI and Bun-through-core.

## 2026-07-04

### Hardened OMP recall context

OMP recall compaction now surfaces fused `retrievalCandidates` as the primary context shape, suppresses raw semantic/content chunks when fused candidates exist, and filters reverse-canon matches unless the term is directly named or tied to a surfaced candidate path.

### Added focused OMP adapter tests

Added Bun coverage for:

- adapter label and lifecycle hook registration
- Solarisael OMP tool surface and schemas
- recall compactor fused-candidate behavior and canon filtering

### Added core + OMP runtime smoke coverage

Added deterministic core tests for JSON-only memory source behavior, pre-turn injection, forced-JSON recall, and query-route skip labels.

Added OMP runtime smoke tests that execute the context hook, safe local/core-backed tools, model-default paths, and `remember`/`sleep`/`wake` through temp `SOLARISAEL_SUBSTRATE` fake scripts instead of the real substrate.

### Added first query-routing slice

Added pure core query routing for raw code/path token extraction, original-case entity hints, casual/date/technical intent classification, source-lane decisions, and OMP auto-recall gating. Core recall now passes lane skip flags to the Postgres helper, and the helper can skip semantic/content/date branches while preserving full-mode payload keys as empty arrays.

### Deferred v1 package metadata

Core, OpenCode adapter, and OMP adapter package versions remain `0.1.0`. The premature `1.0.0` marker was reverted until broader runtime verification proves the v1 contract.

### Verification

```text
C:/Projects/solarisael-house          bun test -> 46 pass, 0 fail
C:/Projects/solarisael-house          bun run tests/recall.integration.ts -> all expectations met
C:/Projects/solarisael-house          python py_compile helpers -> passed
C:/Projects/solarisael-house-omp      bun test -> 12 pass, 0 fail
```

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
