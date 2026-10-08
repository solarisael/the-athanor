# Athanor Bugs

One row is one concrete defect. Each row separates source behavior, observed evidence, and remaining proof.
The original census used `a6ab453`. Later repairs carry their own dates and deployment state.
Historical logs remain in [`docs/history/2026-10-04-bugs-live-log.md`](docs/history/2026-10-04-bugs-live-log.md).
Platform and design limits remain in [`docs/LIMITATIONS.md`](docs/LIMITATIONS.md).

The 2026-10-05 Windows pass separates source repairs, installed behavior, and missing acceptance evidence.
The native security and persistence repairs below still require a coordinated Host and OMP restart before deployment.
Rows 29 and 30 are Linux-only and are outside this pass.

### 1. Recall latency is far above native cost under session load

- State: Open
- Truth: Recent production Recall spans still have a latency tail. Across 386 calls, mean duration was 1,704.9 ms and p95 was 4,163.3 ms.
- Evidence: The 2026-10-05 Insula query covered seven days. One bounded probe took 234 ms through Recall and 388 ms through a new native process.
- Proof owed: Reproduce the slow path under session load and attribute its cost. The bounded probe does not close this defect.

### 2. The OMP keeper does not provide a working restart and resume plane

- State: Live-proven
- Truth: Each room starts through `athanor.exe keeper --config`, and after an armed exit the keeper relaunches `omp --resume <session_id>`.
- Evidence: `C:/ProgramData/Solarisael/Athanor/config/harnesses.json:4-29`; `crates/omp-keeper/src/keeper.rs:813-823`; `adapters/omp/house-proof/restart-door.ts:474-524,533-569`
- Proof owed: Host-side Presence Insula points carry the session, not `host:<room>`. The current behavior is Not re-verified at a6ab453 (the Presence Insula emitter in `crates/host` decides).

### 3. Long sessions degrade identity and context quality

- State: Open
- Truth: Not re-verified at a6ab453 (a long-session Insula and compaction trial decides): over long OMP sessions, replies flatten toward generic assistant prose.
- Evidence: `docs/history/2026-10-04-bugs-live-log.md:40`
- Proof owed: Run a long-session scenario with repeated compaction, and measure retained identity invariants and irrelevant-context growth.
- Current boundary: No accepted long-session corpus was found. Existing Presence and compaction checks do not establish conversation quality.

### 4. Lesson triggers misfire during real conversations

- State: Open
- Truth: Not re-verified at a6ab453 (the lesson bridge in `adapters/omp/house-proof` decides): lesson reminders fire on the wrong surface or stay silent when relevant.
- Evidence: `docs/history/2026-10-04-bugs-live-log.md:47`
- Proof owed: Pass a corpus of positive and negative trigger cases through the native bridge, and measure false-positive and false-negative rates.
- Current boundary: The adapter passes approved rules to OMP. A positive/negative conversation corpus and measured trigger errors remain missing.

### 5. OMP adapter tests are environment-sensitive across machines

- State: Open
- Truth: Not re-verified at a6ab453 (the `adapters/omp` suite decides): the adapter suite passes on one machine and fails 39 tests on another.
- Evidence: `docs/history/2026-10-04-bugs-live-log.md:53`; `adapters/omp/deploy-local.ps1:34-67`
- Proof owed: The same hermetic command gives the same result on both machines, or each environment-dependent cluster declares its dependency.
- Current Windows check: The canonical isolated adapter command passed 280 tests on 2026-10-05. The other machine's failure set remains unreconciled.

### 6. Rescue-tool backup runs with no credentials on the Windows tower

- State: Live-proven for the repaired legacy path
- Truth: The vault's `house/substrate/backup_runner.py` forwards the writer's resolved PostgreSQL environment into WSL. `backup.sh` publishes only completed dumps.
- Evidence: On 2026-10-05, the real Windows writer saved a synthetic record in an isolated database. Its WSL dump restored that record.
- Proof owed: None for this isolated write/backup/restore path. The scripts are outside this repository and its release payload.

### 7. The Host receipt bridge stays degraded after a broker restart

- State: Repaired, not deployed
- Truth: A real broker restart reproduced stale `degraded` health after reconnection. Retired-client callbacks no longer write current bridge health.
- Evidence: The corrected Windows Host recovered after an authenticated NATS restart and recreated its receipt consumer. Health remained connected afterward.
- Proof owed: Deploy the repaired Host and repeat the installed broker-restart check.

### 8. The deploy driver reports failure after a landed install

- State: Live-proven
- Truth: The local deploy installs the adapter after the native install and before Doctor, and it tries the health proof three times, 10 s apart.
- Evidence: `substrate/deploy-local.ps1:105-117,122-140`
- Proof owed: Not re-verified at a6ab453 (`installer/native-release-contract.test.ps1` decides): the contract test pins this driver.

### 9. The Mechanics observatory category row overflows its column

- State: Live-proven
- Truth: Not re-verified at a6ab453 (the `gui-prototype` CSS decides): category chips wrap, and no element passes the right edge at 1440 × 1000 or 390 × 844.
- Evidence: `docs/history/2026-10-04-bugs-live-log.md:94`
- Proof owed: none

### 10. The Pulse trace drawer has no operator-reachable trace

- State: Live-proven
- Truth: The Host serves room-scoped Insula spans at `/athanor/v1/insula/spans`, and the Pulse proxy exposes them at `/live/insula/spans`.
- Evidence: `crates/host/src/insula.rs:41-55,321-372,392-656`; `gui-prototype/live-routes.json:2-16`
- Proof owed: none

### 11. A durable write's backup leaves no receipt

- State: Live-proven
- Truth: Each write receipt carries a `backup` result; `sleep` backs up by default, and `remember` backs up only when the caller asks.
- Evidence: `crates/protocol/src/lib.rs:168-176,1397,1751,1785,2159,2174`
- Proof owed: none

### 12. The durable-write file backup reports "program not found" on the Windows tower

- State: Live-proven
- Truth: Not re-verified at a6ab453 (`crates/akasha/src/backup.rs` decides): `resolve_pg_tool` probes each candidate in order and names the chosen route in the receipt.
- Evidence: `docs/history/2026-10-04-bugs-live-log.md:117-119`
- Proof owed: none

### 13. Numeric memory IDs do not resolve through Recall

- State: Deployed; live proof recorded; independent review pending
- Truth: Recall resolves numeric memory IDs within the room and House scope.
- Evidence: Docket quest `8bac5c8b-2740-4c0f-b3d8-42e028471a28`, receipt `f42b1240-d48b-4675-81f4-0fed1ef66f59`, records deployment and live checks on 2026-09-05.
- Evidence: On 2026-10-04, Kintsu's live `recall("memory 4520")` returns record 4520 with reason `exact memory id`.
- Proof owed: Independent review of the quest's success, scope-refusal, and absent-ID receipts. The acceptance verdicts remain pending.
- Fresh isolated proof: All three `recall_reference_integration` cases passed on 2026-10-05, including exact IDs, room scope, and absent IDs.

### 14. Manual Recall clips selected records

- State: Live-proven for the recorded example
- Truth: Manual Recall returns the selected record body separately from its excerpt.
- Evidence: On 2026-10-04, Kintsu's live `recall("memory 4520")` returns a 3,427-character body and a separate 900-character excerpt.
- Proof owed: None for this example. This observation does not certify every query or ranking result.

### 15. Weighty House canon is clipped during reorientation

- State: Live-proven for the recorded case
- Truth: Exact canon is preserved up to the declared 6,000-character ceiling. Larger assertions carry a marked cut.
- Evidence: Installed automatic Recall carried the full exact assertion. The isolated complete-assertion and viewport boundary checks passed on 2026-10-05.
- Proof owed: None for this case. This does not establish long-session identity quality.

### 16. A Hallway root Knock cannot be created through the OMP tool

- State: Installed native protocol verified; OMP exchange proof pending
- Truth: The installed native protocol accepts an omitted root `parentKnockId`. Current OMP schema and forwarding preserve the omission.
- Evidence: The 2026-10-05 isolated PostgreSQL scenario created a root. The installed `0e4ea3c` binary also accepted its idempotent JSONL request.
- Proof owed: A fresh root exchange through the installed OMP tool, without disturbing unrelated room work.

### 17. A child Knock replaces an omitted inherited budget

- State: Installed native protocol verified; OMP exchange proof pending
- Truth: An omitted child budget inherits the stored parent budget.
- Evidence: The isolated scenario inherited six turns and refused a conflicting budget. The installed `0e4ea3c` JSONL replay also retained six turns.
- Proof owed: A fresh root-and-child exchange through the installed OMP tool.

### 18. Chat returns an empty response before completion

- State: Live-proven
- Truth: The adapter settles a chat turn at `agent_end` with the first settled answer that belongs to the chat origin.
- Evidence: `adapters/omp/house-proof/chat.ts:226-258,421-426`
- Proof owed: none

### 19. The Windows service can wedge permanently in a pending state

- State: Live-proven
- Truth: The supervisor starts one child at a time, and a child that exits before it is ready fails the start and stops the started children.
- Evidence: `crates/athanor-install/src/supervisor.rs:30-33,170-185,334-382`; `crates/athanor-install/src/service.rs:264-308`
- Proof owed: none

### 20. Design catalogue same-identity supersession always failed

- State: Live-proven
- Truth: Not re-verified at a6ab453 (`crates/akasha/src/lesson/design/write.rs` decides): a same-identity `design_doc_write` retires the old row and inserts the successor in one transaction.
- Evidence: `docs/history/2026-10-04-bugs-live-log.md:176-178`
- Proof owed: TODO(census): is the `write.rs` repair committed at `a6ab453`? The old row called it uncommitted.

### 21. Presence refuses to reopen after sleep

- State: Live-proven
- Truth: Presence open is get-or-create by session, and each presence session is a PostgreSQL row that the Host caches.
- Evidence: `crates/host/src/presence.rs:12-14,62-69,174-188,223-234`; `crates/host/src/server.rs:1129-1136`
- Proof owed: none

### 22. Presence lifecycle and authority seams

- State: Implemented; full lifecycle acceptance pending
- Truth: The nine historical repairs are present across current Host, AKASHA, Summoning, and OMP source. The old uncommitted-worktree claim was stale.
- Evidence: Independent source review located the implementations. Current workspace checks and the real PostgreSQL Presence persistence scenario passed on 2026-10-05.
- Proof owed: Complete the installed, registered-OMP lifecycle scenarios. Source checks do not replace the long-session acceptance work in row 3.

### 23. The Pulse proxies check no Origin or Host header

- State: Live-proven in the separately installed Pulse
- Truth: Both proxies reject foreign Host and Origin headers before forwarding or local repair. Non-GET/HEAD requests require the canonical Origin.
- Evidence: Pulse was installed and exercised on 2026-10-04. Native requests returned the expected 403/200 results; Chrome rendered the connected surface.
- Proof owed: None for that installed boundary. Operator authentication remains separate work; the full native House release was not replaced.

### 24. The chat ring is lost on a Host restart

- State: Repaired, not deployed
- Truth: The bounded ring, sequence, and drafts have atomic room-local checkpoints. Streaming drafts do not rewrite the history ring.
- Evidence: Real Windows Host process restarts retained messages and drafts. A settled draft stayed retired. Failed writes were refused without changing accepted state.
- Proof owed: Deploy and repeat the installed restart path. These bounded checkpoints are not a complete conversation archive.

### 25. LogConversation writes under a caller-chosen room_dir

- State: Repaired, not deployed
- Truth: Conversation logging resolves the supplied directory against the configured room and refuses a foreign path before writing.
- Evidence: A real WebSocket command against an isolated Host refused a foreign directory and left it untouched. The adapter now surfaces that refusal.
- Proof owed: Deploy the Host and adapter repairs.

### 26. serve.ts says it writes nothing, but chat/say writes

- State: Corrected in source
- Truth: The serving harness documentation now includes the Host write boundary.
- Evidence: `gui-prototype/serve.ts` and its unchanged chat route allow-list.
- Proof owed: None for this comment correction.

### 27. Pulse sediment sends a client-chosen room

- State: Repaired, not deployed
- Truth: Host memory reads use the configured room plus House commons. A client filter cannot grant another room's scope.
- Evidence: Real HTTP/PostgreSQL checks admitted own/shared rows and refused a foreign filter and ID. The timeline integration suite passed.
- Proof owed: Deploy the Host repair. This room boundary does not provide person-level authentication.

### 28. A restart resume is silent unless the session is a TUI

- State: Repaired, not deployed
- Truth: Successor verification no longer requires TUI mode. Non-UI outcomes go to stderr, without corrupting JSON/RPC stdout.
- Evidence: Registered-hook regression cases cover both TUI and print successors, including one continuation and duplicate-start suppression.
- Proof owed: An installed headless keeper restart and continuation after adapter deployment.

### 29. The installed loader requires windows-x64 and USERPROFILE

- State: Open
- Truth: The installed OMP loader refuses a native manifest that is not `windows-x64`, and it throws when `USERPROFILE` is not set.
- Evidence: `adapters/omp/installed-loader.ts:376,565`
- Proof owed: The installed loader starts an OMP session on Linux.

### 30. Fixed C:/ProgramData paths

- State: Open
- Truth: The Pulse prototype and desktop proxy still read fixed `C:/ProgramData/Solarisael/Athanor` paths.
  The adapter NATS fallback was removed by the Host ownership cutover.
- Evidence: `gui-prototype/serve.ts`; `gui-desktop/src/main.rs`; `adapters/omp/house-proof/organ.ts`
- Proof owed: Pulse starts on Linux without fixed Windows paths.

### 31. The managed NATS broker has no credentials

- State: Repaired, not deployed
- Truth: Separate Host and AKASHA credentials enforce subject permissions and private reply prefixes. Delivery API 2 prevents an unsafe downgrade.
- Evidence: A real NATS 2.14.4 process accepted authorized traffic and refused anonymous, wrong-password, forbidden-topic, and cross-identity inbox access.
- Proof owed: Close old Host and OMP processes, deploy through the canonical driver, and verify the installed credentials and user access.
- Compatibility boundary: The first API 2 release cannot roll back to an API 1 generation. A later rollback requires another API 2 release.
