# Athanor Bugs

One row is one concrete defect. Each row gives its state, one true sentence about the code at `a6ab453`, the evidence, and the proof that it still owes.
States are Open, Repaired, not deployed, Live-proven, and Closed. The dated live logs are in [`docs/history/2026-10-04-bugs-live-log.md`](docs/history/2026-10-04-bugs-live-log.md). Platform and design limits are in [`docs/LIMITATIONS.md`](docs/LIMITATIONS.md).

### 1. Recall latency is far above native cost under session load

- State: Open
- Truth: Not re-verified at a6ab453 (`crates/akasha/src/recall` decides): warm Recall under session load costs several times the native embed and query cost.
- Evidence: `docs/history/2026-10-04-bugs-live-log.md:11-15`
- Proof owed: Repeat the instrumented three-room matrix and show warm latency close to the embed cost plus the query cost.

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

### 4. Lesson triggers misfire during real conversations

- State: Open
- Truth: Not re-verified at a6ab453 (the lesson bridge in `adapters/omp/house-proof` decides): lesson reminders fire on the wrong surface or stay silent when relevant.
- Evidence: `docs/history/2026-10-04-bugs-live-log.md:47`
- Proof owed: Pass a corpus of positive and negative trigger cases through the native bridge, and measure false-positive and false-negative rates.

### 5. OMP adapter tests are environment-sensitive across machines

- State: Open
- Truth: Not re-verified at a6ab453 (the `adapters/omp` suite decides): the adapter suite passes on one machine and fails 39 tests on another.
- Evidence: `docs/history/2026-10-04-bugs-live-log.md:53`; `adapters/omp/deploy-local.ps1:34-67`
- Proof owed: The same hermetic command gives the same result on both machines, or each environment-dependent cluster declares its dependency.

### 6. Rescue-tool backup runs with no credentials on the Windows tower

- State: Open
- Truth: Not re-verified at a6ab453 (`house/substrate/backup.sh` decides): a `record_memory.py --env-file` write commits, but its backup runs with an empty `PGPASSWORD`.
- Evidence: `docs/history/2026-10-04-bugs-live-log.md:59-60`
- Proof owed: A `record_memory.py --env-file ../state/substrate/.env` write on the Windows tower prints a `backup:` line, and the dump exists.

### 7. The Host receipt bridge stays degraded after a broker restart

- State: Live-proven
- Truth: `/health` reports the AKASHA delivery broker state, and the installed loader only warns when the Host stops answering; it never starts the Host.
- Evidence: `crates/host/src/server.rs:252,261-275`; `adapters/omp/installed-loader.ts:530-552`
- Proof owed: Not re-verified at a6ab453 (`run_receipt_bridge` in `crates/host/src/server.rs` decides): the bridge rebuilds its consumer after a broker restart.

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

### 14. Manual Recall clips selected records

- State: Live-proven for the recorded example
- Truth: Manual Recall returns the selected record body separately from its excerpt.
- Evidence: On 2026-10-04, Kintsu's live `recall("memory 4520")` returns a 3,427-character body and a separate 900-character excerpt.
- Proof owed: None for this example. This observation does not certify every query or ranking result.

### 15. Weighty House canon is clipped during reorientation

- State: Repaired, not deployed
- Truth: Not re-verified at a6ab453 (`crates/host/src/viewport.rs` decides): an exact canon match carries the full assertion up to 6000 characters, and each cut is marked `truncated`.
- Evidence: `docs/history/2026-10-04-bugs-live-log.md:140`
- Proof owed: Automatic Recall in `kintsu` that mentions `The Athanor` shows the full assertion once.

### 16. A Hallway root Knock cannot be created through the OMP tool

- State: Repaired, not deployed
- Truth: Not re-verified at a6ab453 (`crates/hearth/src/hallway.rs` and `adapters/omp/house-proof/tools.ts` decide): a root Knock omits `parentKnockId`, and a continuation supplies the prior receipt's UUID.
- Evidence: `docs/history/2026-10-04-bugs-live-log.md:146-147`
- Proof owed: Send one root Knock from this room through the OMP tool.

### 17. A child Knock replaces an omitted inherited budget

- State: Repaired, not deployed
- Truth: Not re-verified at a6ab453 (the adapter and Rust Knock decoders decide): a child Knock that omits `max_turns` inherits the budget of its parent.
- Evidence: `docs/history/2026-10-04-bugs-live-log.md:154-155`
- Proof owed: Run a fresh root-and-child exchange after the shared Host restarts onto the installed repair.

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

- State: Open
- Truth: The repairs for nine Presence seams exist only in the uncommitted `kintsu/summoning` worktree, outside `a6ab453`.
- Evidence: `docs/history/2026-10-04-bugs-live-log.md:192`
- Proof owed: Land the repairs in `dev/next`, and exercise them through the installed runtime.

### 23. The Pulse proxies check no Origin or Host header

- State: Repaired in source, not deployed
- Truth: Both proxies reject foreign Host and Origin headers before forwarding or local repair. Non-GET/HEAD requests require the canonical Origin.
- Evidence: `gui-prototype/serve.ts` and `gui-desktop/src/proxy.rs`. The real Bun proxy smoke rejects foreign, null, missing, and wrong-port origins.
- Proof owed: Deployment and an installed browser check. Native clients can forge headers; operator authentication remains separate work.

### 24. The chat ring is lost on a Host restart

- State: Open
- Truth: The chat ring lives only in Host memory, so a Host restart loses every line and draft.
- Evidence: `crates/host/src/chat.rs:6-7,25-30`; `crates/host/src/server.rs:138,218`
- Proof owed: After a Host restart, a chat snapshot returns the lines from before the restart.

### 25. LogConversation writes under a caller-chosen room_dir

- State: Open
- Truth: `LogConversation` creates directories and appends files under a caller-chosen `room_dir` with no check, although the comment at `server.rs:2064-2066` says otherwise.
- Evidence: `crates/host/src/server.rs:2117-2127,2131-2235`
- Proof owed: A `LogConversation` with a `room_dir` outside its room is refused.

### 26. serve.ts says it writes nothing, but chat/say writes

- State: Corrected in source
- Truth: The serving harness documentation now includes the Host write boundary.
- Evidence: `gui-prototype/serve.ts` and its unchanged chat route allow-list.
- Proof owed: None for this comment correction.

### 27. Pulse sediment sends a client-chosen room

- State: Open
- Truth: A Pulse room shelf sends `room = item.room ?? item.id` for whichever shelf is open, not the connected room.
- Evidence: `gui-prototype/sediment/index.js:112`; `gui-prototype/app.js:1188-1190`
- Proof owed: TODO(census): does the Host refuse a foreign room on the sediment memory and timeline routes?

### 28. A restart resume is silent unless the session is a TUI

- State: Open
- Truth: Successor verification and continuation return early with no notice unless `ctx.mode === 'tui'`, so a headless relaunch never verifies.
- Evidence: `adapters/omp/house-proof/restart-door.ts:481`
- Proof owed: A headless keeper relaunch verifies and continues, or it reports why it cannot.

### 29. The installed loader requires windows-x64 and USERPROFILE

- State: Open
- Truth: The installed OMP loader refuses a native manifest that is not `windows-x64`, and it throws when `USERPROFILE` is not set.
- Evidence: `adapters/omp/installed-loader.ts:376,565`
- Proof owed: The installed loader starts an OMP session on Linux.

### 30. Fixed C:/ProgramData paths

- State: Open
- Truth: The Pulse prototype, the desktop proxy, and the adapter NATS fallback read fixed `C:/ProgramData/Solarisael/Athanor` paths.
- Evidence: `gui-prototype/serve.ts:17-18`; `gui-desktop/src/main.rs:54-56`; `adapters/omp/rust-transport.ts:60`
- Proof owed: On Linux, Pulse starts and the adapter fills a NATS URL with no hand-set variables.
