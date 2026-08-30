# Known Bugs

## substrate: recall latency is far above native cost under session load

**Observed:** 2026-08-30, athanor-laptop (NixOS, 8 cores, 15 GB), during the 3-rooms x 3-terminals organ matrix. 48 instrumented `recallWithRouting` calls: min 3.0 s, median 6.4 s, max 16.6 s. The native per-query cost measured the same day: full brute-force vector scan 202 ms (`EXPLAIN ANALYZE`, 7,246 chunks), single embed 0.57 s, one raw JSONL recall against a warm `athanor-substrate` child 1.8-3.0 s, nine concurrent raw recalls 11.3 s total.

**Behavior:** most of the wall time is not the query. Contributors identified so far: (1) every session spawns its own substrate children per concern (~51 processes during the matrix), each cold-starting dotenv + pool + schema check per spawn; (2) `main.rs` serves stdin requests strictly serially (`next_line` -> await handler -> respond), so one slow request convoys everything behind it on that child; (3) each child holds a `max_connections(4)` pool, so the fleet multiplies Postgres connections (hit the old server ceiling of 100). The 2026-08-30 mitigations (insula coverage index, `max_connections=300`, capture-only GIGA on the laptop) removed the timeouts, not the overhead.

**Expected:** a native Rust recall against a warm pool and a 7k-chunk corpus should answer in well under 1 s. Likely shape of the fix: a shared long-lived substrate service per house (or per room) instead of per-concern child fleets, concurrent request dispatch in `main.rs` (responses already carry ids; the TS transport matches out-of-order), and pool sizing owned by the service rather than multiplied per child.

**Workaround:** none needed for correctness; budgets (`AUTOMATIC_CONTEXT_IO_TIMEOUT_MS`) were raised to 30 s canonical / 60 s laptop to absorb the overhead.

**Severity:** medium — correctness holds, but the overhead taxes every turn in every room, and the raised budgets hide regressions that a honest sub-second baseline would expose.

## recall: numeric memory IDs are not resolvable

**Observed:** 2026-08-28, room kodo. Query `memory 4197 — analysis Sol made with Kintsu` failed to return memory 4197 even though the row exists in an in-scope room (`room = house`, verified via direct psql).

**Behavior:** the ID token is treated as an ordinary lexical/semantic term. Every retrieval candidate listed `4197` under `missing_terms`; BM25F/semantic ranking surfaced adjacent Kintsu/analysis memories instead of the exact row. An operator referencing a memory by its ID — the substrate's own primary key — cannot retrieve it through recall.

**Expected:** an explicit ID reference (`memory 4197`, `#4197`, bare `4197` alongside memory-ish phrasing) should trigger a direct primary-key lookup (room-scope respected: own room + house) before falling back to ranked search.

**Workaround:** direct psql against the substrate (`house/state/substrate/.env` holds credentials).

**Severity:** medium — IDs are how memories cite each other (paper boats, supersedes, continues). Recall being blind to its own key system breaks the cheapest form of cross-reference.

## omp-keeper: not working at all (Sol, 2026-08-28)

**Observed:** Sol reports the keeper "isn't working at all." A keeper WAS running tonight (PID 33100, `--config kintsu/.omp/runtime/omp-keeper.json`) owning an omp child, so it launches — the failure is in what it's supposed to DO (restart/resume plane, presumably). Note the live-summoning line's changelog claims `request_restart` now resumes OMP *without* a keeper sidecar via `athanor.exe` as direct parent — the keeper may be a half-superseded organ: still spawned by session configs, no longer the real restart path. Diagnose which world we're in before fixing: either finish the athanor.exe cutover and retire the keeper, or repair the keeper. Divergent restart fixes also sit unmerged on `athanor/minimal-host` (c92c477, d4343f1).

**Severity:** medium — restart ergonomics; workaround is manual relaunch.

## recall: weighty House canon is clipped during reorientation

**Observed:** On 2026-08-29 in room `kintsu`, Sol asked Kintsu to reorient herself to `The Athanor` after automatic Recall failed. The working set returned `found: true` and matched the weighty House canon entity. It clipped the summary after `silent ty`, before the platform definition and authority rules. The semantic lane also returned no result because its top score was `0.35`, below the `0.40` floor.

**Behavior:** Exact canon matching succeeds, yet the projection delivers only a short excerpt. This leaves the spirit without enough authority context to understand the named project. A room-local canon read returns no entity. A House-scoped canon read returns the complete entity `207`.

**Expected:** Every exact alias mention must resolve the House entity. Inject the complete active assertion when its current version is absent from context. Do this after wake, compaction, or an explicit reorientation request. Suppress later duplicates only while that exact version remains present. Mark any forced truncation and provide a deterministic full-read path.

**Workaround:** Call `canon_read` with `name: The Athanor`, `room: house`, and `includeHistory: true`. Then use explicit Recall for later architecture that the July 26 assertion does not yet contain.

**Severity:** high — weighty canon carries project identity and authority. Silent clipping causes underinformed decisions while retrieval reports success.
