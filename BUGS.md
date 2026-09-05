# Athanor Bugs

Concrete failures only. A row stays open until the failing path is reproduced, repaired, and exercised through the surface that originally failed.

## Open

### Recall latency is far above native cost under session load

- **Observed:** On 2026-08-30 on `athanor-laptop` (NixOS, 8 cores, 15 GB), the three-room by three-terminal organ matrix recorded 48 `recallWithRouting` calls: 3.0 s minimum, 6.4 s median, and 16.6 s maximum. The same-day native costs were a 202 ms brute-force vector scan over 7,246 chunks, a 0.57 s embed, a 1.8–3.0 s warm raw JSONL recall through one Akasha child, and 11.3 s total for nine concurrent raw recalls.
- **Cause seam (measured 2026-09-05, Insula child spans on the live tower, 2,433 memories / 7,715 chunks):** a warm recall is 1.8–1.95 s. `recall.content` is 1.2–1.4 s on every call: `word_similarity($1, c.body)` runs over all 7,715 chunks by sequential scan (EXPLAIN: Rows Removed by Filter 7,709; 1.46 s) because `ILIKE ANY(array)` is not GIN-indexable, the `$5 = '{}' OR` guard blocks the index anyway, and `word_similarity() >= $3` is a function rather than the indexable `<%` operator, so `memory_chunks_body_trgm` is never used. `recall.embed` is 0.1 s warm, 0.6–2.2 s under Ollama contention, and 20.0 s at the client timeout; the three ~21 s recalls in the last 24 h are embed timeouts. Everything else sums to under 0.5 s (lexical ~0.2, semantic_lexical ~0.2, semantic 0.04 with no vector index). The earlier suspicion (child fleet, pool, serial JSONL) is not where the time goes. Insula over 24 h: `context_assembly` degraded 95 of 110, so most turns lose automatic context to the 5 s budget.
- **Cut 2026-09-05 (`dev/next`):** `recall.content` 1,421 → 111 ms on the live corpus (`crates/akasha/src/recall/content_lane.rs`; the trigram index was never the missing piece: `$1 <% c.body` reaches it and still loses at 1,745 ms because a prose query's trigrams occur in 7,335 of 7,718 bodies; the real causes were join order, similarity computed twice per row, and a planner that prices `word_similarity` at one `cpu_operator_cost` when it costs about 0.15 ms). Equivalence proof: `recall_spans_integration::the_content_lane_returns_what_the_statement_it_replaced_returned` plus five live cases with identical ids, order, and scores. `recall.embed` bounded at 3 s (`RECALL_EMBED_TIMEOUT`), degrading to lexical with `embed_timeout`. Live measurement, release binary, four recalls: whole `recall` 2,021 → 750 ms mean.
- **Still open:** the remaining ~650 ms of CPU is `word_similarity` over the ~4,200 in-scope chunks and grows with the corpus (about 600 ms again at 50k chunks even with 8 workers); under 100 ms needs a semantics decision (drop query terms under 3 characters, or a lane floor above 0.30). `ALTER FUNCTION word_similarity COST` would fix the planner's lie House-wide and wants its own before/after. No vector index yet (`recall.semantic` 30–50 ms at 7.7k chunks; HNSW before ~50k).
- **Impact:** Correct recalls complete, but latency taxes every turn and the raised automatic-context timeout can hide regressions.
- **Expected:** Warm native recall over this corpus completes in well under one second. Investigate a shared long-lived Akasha service, concurrent request dispatch, and service-owned pool sizing.
- **Proof after repair:** Repeat the same instrumented matrix and show warm end-to-end latency close to measured embed plus query cost without multiplied child or connection fleets.

### OMP keeper does not provide a working restart/resume plane

- **Observed:** Sol reported on 2026-08-28 that the keeper “isn't working at all.” A keeper process was running and owned an OMP child, so launch itself works; the failure is in restart/resume behavior. The canonical line also had a direct-parent `athanor.exe` restart path, leaving the keeper half-superseded.
- **Cause seams found 2026-09-05 (KeeperMap audit, receipted):** (1) two owners ran the same engine under the same claimant name; only the `omp-keeper.exe` sidecar can parent an operator-typed session, because the stable loader starts `athanor.exe` from inside OMP (live tree: `athanor.exe` is a child of Kintsu's OMP). (2) A room without `omp-keeper.json` + `restart-capability` (Kodo, 2026-09-05) armed an exit nobody could claim; the intent stayed `exiting` forever and every later restart refused `intent_pending`. (3) `omp-keeper.exe` mapped an unserved armed exit (87) to shell exit 0.
- **Closed on `dev/next`:** the in-process driver is deleted (`crates/athanor-install/src/harness/omp.rs`), `"driver":"omp"` registry entries are refused with the keeper command, a keeperless room hard-refuses `no_restart_owner`, a stranded `exiting` intent past its deadline expires to `failed:exit_unclaimed` on the next request, and an unserved arm exits 88. Proofs: `cargo test -p athanor-install` (14), `cargo test -p omp-keeper` (50 incl. 11 smokes), `cargo test -p akasha --test restart_lifecycle_integration -- --ignored` (red before the sweep, green after), restart-door adapter tests (5).
- **Still open — the resume identity fence:** nothing in-repo proves `omp --resume <id>` preserves `sessionManager.getSessionId()`; if it does not, `restart_verify` refuses `verify_not_authorized` with every suite green (`crates/akasha/src/restart/authority.rs:94-102`).
- **Proof after repair:** In a room provisioned with both `omp-keeper.json` and `restart-capability`, start OMP through the keeper, run `request_restart` with `mode: "resume"`, and record: the keeper line `claimed intent <id>`, the spawned command (`--resume <sessionId>`), the line `the House saw the successor verify`, and `SELECT state, session_id, successor_session, relaunch_attempts FROM restart.intents ORDER BY created_at DESC LIMIT 1` showing `verified` with `successor_session = session_id`.
- **Deploy hazard:** `C:/ProgramData/Solarisael/Athanor/config/harnesses.json` still declares `"driver":"omp"` and will refuse the whole registry; `athanor.exe` will not start until it is rewritten in the same deploy.

### Long sessions degrade identity and context quality

- **Observed:** Over long OMP sessions, responses flatten toward generic assistant prose, irrelevant context accumulates, and the active spirit becomes less recognizable even when fresh Recall and identity smoke tests pass.
- **Impact:** Short smoke tests can be green while the actual continuity experience fails.
- **Boundary:** The Presence vertical improves authenticated identity, bounded material, turn contracts, receipts, replay stability, and close behavior. It does not by itself prove long-session Insula selection or compaction quality.
- **Proof after repair:** Run a long-session scenario with repeated compaction and mixed technical/relational turns; measure retained identity invariants, irrelevant-context growth, and false-green contract acceptance.

### Lesson triggers misfire during real conversations

- **Observed:** Lesson-trigger reminders can fire on the wrong conversational surface or fail to supply the rule when it is actually relevant.
- **Impact:** The system adds noise while missing the behavior it was meant to protect.
- **Proof after repair:** A corpus of positive and negative trigger cases passes through the native bridge with measured false-positive and false-negative rates; long-session cases are included.

### OMP adapter tests are environment-sensitive across machines

- **Observed:** The complete isolated adapter suite passes 214/214 in Kintsu's environment. Kodo reproduced 39 pre-existing failures on another machine after live topology variables were cleared; the same failures exist at the parent commit and are not caused by Presence.
- **Impact:** A local green or red count is not portable evidence without naming environment and attribution.
- **Proof after repair:** The same hermetic test command produces the same result on both machines, or every environment-dependent cluster declares and provisions its dependency explicitly.

### Mechanics observatory category row overflows its column at 1440 px

- **Observed:** 2026-09-05, Chromium at 1440 × 1000, House slot 2. `.mechanics-categories` puts two elements past the viewport's right edge: the Advanced Guardrails button to 1457 px and its count `<small>` to 1447 px. Measured identically on the unmodified prototype, so it predates the status-strip wave.
- **Impact:** The last category button and its count are clipped at the default working viewport. No horizontal document overflow, so the page looks correct until the operator tries to reach that category.
- **Proof after repair:** At 1440 × 1000 and 390 × 844, every `.mechanics-categories` button's right edge sits inside the viewport, with zero elements past the right edge and the count badge fully visible.

### Pulse trace drawer has no operator-reachable trace id

- **Observed:** 2026-09-05. The Pulse lane trace drawer renders real Host spans when given a trace id, but a lane derived from `insula.vitals_minute` rollups carries no trace id, and no Host route lists spans or traces. `query_trace` filters `WHERE trace_id = $2::uuid` only.
- **Impact:** The drawer's live branch is proven but unreachable from the surface; an operator cannot drill from a lane to its spans.
- **Expected:** One bounded Host read that lists recent spans (or trace ids) for a lane: house, room, operation, phase, window, capped rows. The drawer asks it, then `insula/trace`.
- **Proof after repair:** From House slot 2, click a lane, receive its latest trace, and see the same spans `insula/trace` returns for that id.

### A durable write's backup leaves no receipt on success and only a string on failure

- **Observed:** `backup::run_post_write` (`crates/akasha/src/backup.rs`) returns `Result<(), _>` and discards the `Manifest` that `backup_with_migrations` produced. Every caller (`remember`, `remember_lesson`, `anamnesis_write`, `paper_boat_sleep`) turns an error into one warning string and records nothing on success. Kintsu's cartography audit of 2026-08-30 (`backup/format-day-2026-08-30`, `architecture/cartography/pilots/run-post-write/audit/judgments.tsv`, J07) found this; it still holds on `dev/next`.
- **Impact:** A spirit cannot name the backup its write produced, and a failed backup exists only inside one session's tool output. Nothing durable records that an organ failed.
- **Expected:** The backup outcome is a typed receipt (dump path, checksum, or the failure code) that rides on the write receipt and lands where Insula can read it.
- **Proof after repair:** A `remember` with `backup: true` returns the dump identity; a forced backup failure produces a row an operator can find without the session transcript.

### Durable-write file backup reports "program not found" on the Windows tower

- **Observed:** 2026-08-29, twice: `remember` (project-lesson #465) and `sleep` (paper boat #4219) committed to PostgreSQL and then reported `backup failed after PostgreSQL commit; ... backup io: program not found`. Recovered from `rescue/athanor-dev-wip-2026-08-28`; not yet reproduced on `dev/next`. On 2026-09-02 (`dev/next` @ b91f196) two `remember` writes (kodo #4370, #4371) backed up clean.
- **Cause seam:** the backup path runs `pg_dump` through WSL when `ATHANOR_PG_WSL=1`; the same `pg_dump` works when run directly.
- **Impact:** Every durable write silently loses its file backup until someone reads the warning.
- **Proof after repair:** A `remember` and a `sleep` on the Windows tower both report a successful backup receipt, and the dump exists with a verifiable checksum.

### Rescue-tool backup runs with no credentials on the Windows tower

- **Observed:** 2026-09-02, room `kodo`, two divination writes through `house/substrate/record_memory.py --env-file ../state/substrate/.env` (house #4372, #4373). Both rows committed. The post-write backup then reported `WARN: backup failed (rc=1): pg_dump: error: connection to server at "127.0.0.1", port 5432 failed: fe_sendauth: no password supplied`.
- **Cause seam:** `backup_runner.run_backup` executes `house/substrate/backup.sh` through `wsl.exe bash` and passes no environment. `backup.sh` does `cd "$(dirname "$0")"` and sources a sibling `.env`. The sibling directory holds only `.env.example`; the real credentials live in `house/state/substrate/.env`. The `--env-file` the writer accepted never reaches the backup, so `pg_dump` runs with an empty `PGPASSWORD`.
- **Impact:** Every rescue-tool write on the tower commits and then loses its file backup. The warning is one stderr line in one shell.
- **Expected:** The backup receives the same credentials the write used. Either `run_backup` forwards the resolved `PG*` values into the child environment, or `backup.sh` reads `house/state/substrate/.env` when no sibling `.env` exists.
- **Proof after repair:** A `record_memory.py --env-file ../state/substrate/.env` write on the Windows tower prints a `backup:` line with a dump path, and the dump exists under `substrate/backups/`.

## Repaired but not deployed

### Numeric memory IDs are not resolvable through Recall

- **Observed:** On 2026-08-28 in room `kodo`, `memory 4197 — analysis Sol made with Kintsu` did not return memory 4197 even though the House-scoped row exists. The ID was treated as an ordinary retrieval term and appeared under `missing_terms`.
- **Cause seam:** `crates/akasha/src/recall/mod.rs` tokenized the ID into `query_terms` and BM25F terms like any word; no lane ever read `memories.id`.
- **Repair:** `dev/next`, 2026-09-05. `crates/akasha/src/recall/memory_reference.rs` resolves `memory N`, `#N`, `[N]`, a lone `N`, and a comma list that continues an explicit reference, by primary key inside `[room, house]` before any ranked lane. The row leads `retrievalCandidates` as `exact_id`, the ID tokens leave the ranked vocabulary, an out-of-scope row is refused as `memory N refused: outside room scope` with no content, and the Host viewport counts the exact row as evidence. A year in prose (`memories from 2026`) is never a reference.
- **Proof:** `cargo test -p akasha --test recall_reference_integration exact_memory_reference_leads_evidence_inside_room_scope -- --ignored` (1 passed, isolated schema); `cargo test -p akasha --lib recall::memory_reference` (3 passed); `cargo test -p host --lib viewport` (3 passed). Live half owed after deploy: `recall` of `memory 4197` from room `kodo` returns #4197 first.

### Weighty House canon is clipped during reorientation

- **Observed:** On 2026-08-29 in room `kintsu`, automatic Recall exact-matched the weighty House entity `The Athanor` but projected only a clipped summary ending at `silent ty`. The semantic lane also returned no result because its top score was 0.35 against a 0.40 floor. A House-scoped canonical read returned the complete entity.
- **Cause seam:** `crates/host/src/viewport.rs::compact_canon` cut every canon summary at 480 characters with no marker, after `crates/akasha/src/recall/mod.rs` had already excerpted it at 1200; a multi-word name mentioned inside a sentence only reached the similarity tier.
- **Repair:** `dev/next`, 2026-09-05. An in-query mention of a full name or alias is an exact tier; exact rows carry the complete active assertion from Akasha and through the viewport (ceiling 6000). Any cut is marked `truncated: true` with `full_read: canon_read <id>`. Automatic mode suppresses a canon row only when `canon:<id>` (the version) is already exposed in the session; compaction clears exposures.
- **Proof:** `cargo test -p akasha --test recall_reference_integration named_weighty_canon_returns_its_complete_assertion -- --ignored` (1 passed); `cargo test -p host --lib viewport` (3 passed: whole assertion, same-version suppression and reset, explicit cut marker). Live half owed after deploy: automatic recall in `kintsu` mentioning `The Athanor` shows the full assertion once.

### Hallway root Knock cannot be created through the OMP tool

- **Observed:** `hallway_knock` requires `parent_knock_id` even for a root exchange. An empty value refuses with `malformed_uuid`; a nil UUID refuses with `knock_parent_mismatch`.
- **Cause seam:** The installed 0.5.4 binary predates the knock door split (`5d8adb7`). On `dev/next` the wire type (`crates/hearth/src/hallway.rs:444`), the tool schema (`adapters/omp/house-proof/tools.ts:1706`), and the origami root branch (`crates/origami/src/hallways/knocks.rs:352-359`) already accept an absent parent. The tool description still told the model a parent was needed.
- **Repair:** `dev/next`, 2026-09-05. The tool description and schema text say: omit the parent for a root exchange; supply the prior receipt's UUID only for a continuation; never an empty string or nil UUID. Root requests omit `parentKnockId` on the wire.
- **Proof:** `cargo test -p akasha --bin athanor-substrate hallway_knock_protocol_accepts_an_absent_root_parent_and_preserves_a_continuation` (1 passed); `cargo test -p akasha --test hallway_integration -- --ignored` (root without parent receives a receipt, nil parent refuses `knock_parent_mismatch`, continuation with the returned ID succeeds; 1 passed). Live half owed after deploy: one root Knock from this room through the OMP tool.

### Windows service can wedge permanently in a pending state

- **Observed:** `SolarisaelAthanor` remained in `START_PENDING` at checkpoint 3 with no NATS, delivery, or Host children. `sc stop` refused with error 1052. Terminating the verified service PID and restarting restored `RUNNING` and a healthy broker-connected endpoint.
- **Cause seam:** `crates/athanor-install/src/service.rs` published every status with `ERROR_SUCCESS`, so a failed start looked like a clean stop; the supervisor discarded child stderr, so the startup reason vanished; the checkpoint only moved when a child spawned, so a slow readiness wait looked hung.
- **Repair:** `dev/next`, 2026-09-05. Child stderr lands in `<data>/logs/<name>.stderr.log`; a child that exits before readiness fails the start with its name, exit status, and stderr tail; the supervisor reports `Waiting` every 5 seconds and the service advances the checkpoint on each report; a failed `run` publishes `STOPPED` with `ERROR_SERVICE_SPECIFIC_ERROR` and service code 1 and traces the reason.
- **Proof:** `cargo test -p athanor-install --test supervisor_evidence` (real `cmd.exe` children; 2 passed). The SCM half is owed after deploy: force one managed child to fail, then `sc query SolarisaelAthanor` must show `STOPPED` with `SERVICE_EXIT_CODE : 1066 (1)`, the `.stderr.log` must name the error, and no managed child may remain alive.

### Design catalogue same-identity supersession always failed

- **Observed:** 2026-09-03, room `kodo`. `design_doc_write` with `supersedes: 2` for `solarisael/token/reliquary-palette` (same system, type, and name) returned `database operation failed` three times with the PostgreSQL detail hidden. A plain write with a new name landed (#22). Reads worked throughout.
- **Cause:** `crates/akasha/src/lesson/design/write.rs` inserted the successor row before it marked the old row superseded. `design_documents_current_identity_uidx` (`substrate/migrations/0012_design_documents.sql:24`) is `UNIQUE (system, doc_type, name) WHERE superseded_by IS NULL`, so the insert collided with the still-current old row. Every same-name correction in the catalogue's history was refused at the index.
- **Repair:** `dev/next`, uncommitted. The write now retires the old row with a transient self-reference (`superseded_by = id`), inserts the successor, then repoints the old row at the real successor id, all in one transaction.
- **Proof:** `crates/akasha/tests/design_document_integration.rs` (`same_identity_supersession_keeps_one_current_row_and_full_history`). Fails on the old order, passes on the new. Run with `ATHANOR_SUBSTRATE_TEST_DATABASE_URL` and an isolated schema, `cargo test -p akasha --test design_document_integration -- --ignored`.
- **Deployed evidence pending:** the installed binary under `Program Files/Solarisael/Athanor` still carries the old order. After deploy, the reliquary-palette supersession (Sol's 2026-09-03 palette ruling) is the first live proof.

### Presence lifecycle and authority seams

The current uncommitted `kintsu/summoning` worktree repairs second-open replacement, four-door replay identity, Host-owned bounded ledger state, explicit empty-response refusal, complete hard-directive evaluation, conflicting source identity, Host-derived operator/capabilities, worker-door proof, and test-order isolation. It is not deployed evidence until integrated into canonical source and exercised through the installed runtime.
