# Medium and low machines

Status: `census` (2026-11-06).

This file holds one section for each medium or low machine.
Each section uses the seven sections of the other files in a short form.
The GUI chat send flags and the say lifecycle belong to [chat-say.md](chat-say.md). This file does not repeat them.
A claim marked `unverified` has no direct read or run behind it.

## GUI rounds (`gui-prototype`)

### What it is

The prototype GUI asks the Host and the local proxy in rounds.
A round is one request with a state that the GUI shows.
Six rounds and one navigation record live in module variables. None of them is persisted.

### States

- `connection`: `pending`, `live`, `failed` as free strings: `app.js:87`.
- Health round: `idle`, `pending`, `live`, `failed`: `health.js:15`. Retry clock `{timer, attempt, delayMs}`: `health.js:25`. Gate `healthInFlight`: `health.js:162`.
- Room round: the same four values: `health.js:16`. Gate `roomInFlight`: `health.js:126`.
- Chat round: the same four values, plus `reason` and `transport`: `chat.js:3`.
- Pulse round: the same four values: `pulse.js:25`. Drilldown `spans` and `trace`: `pulse.js:33-34`.
- Repair round: `idle`, `pending`, `answered`, `failed`, plus `matrix` and `result`: `repair.js:4`.
- View state: `mode`, `activeId`, `activeView`, `houseReturn`, `selectedMessageKey`, `drawerView`: `app.js:90-117`.

### Transitions today

| Round | Move | Writer |
|---|---|---|
| connection | `pending` → `live` | `app.js:2047` |
| connection | `pending` → `failed` | `app.js:2051` |
| health | cold or `failed` → `pending` | `health.js:173` |
| health | `pending` → `live`, heartbeat set | `health.js:181-182` |
| health | `pending` → `failed`, retry set | `health.js:185-186`, `scheduleRetry` `health.js:92-96` |
| health | `live` → `failed` (failure that another door reports) | `noteHostFailure`, `health.js:34-39` |
| room | cold or `failed` → `pending` | `health.js:131` |
| room | `pending` → `live` or `failed` | `health.js:138`, `health.js:141-146` |
| chat | any → `pending` | `chat.js:92` |
| chat | `pending` → `live` | `chat.js:115` |
| chat | → `failed` | `failRound`, `chat.js:80-83`; called at `chat.js:125` and `chat.js:187` |
| pulse | → `pending` | `pulse.js:238` |
| pulse | `pending` → `live` or `failed` | `pulse.js:260-261`, `pulse.js:263` |
| repair | → `pending` (status, start) | `repair.js:13`, `repair.js:26` |
| repair | `pending` → `answered` or `failed` | `repair.js:17`, `repair.js:19`, `repair.js:36`, `repair.js:38` |
| view | `mode` | `app.js:576`, `app.js:1379` (in `render`), `app.js:1474` |
| view | `activeView` | `app.js:552`, `app.js:577`, `app.js:1435`, `app.js:1446` |
| view | `activeId` | `app.js:529`, `app.js:2045` |
| view | `houseReturn` | `app.js:573`, `app.js:575`, `app.js:1434`, `app.js:1445`, `app.js:1473` |
| view | `selectedMessageKey` | `app.js:530`, `app.js:554`, `app.js:1163` (in `renderInspector`), `app.js:1525`, `app.js:1587`, `app.js:1607` |
| view | `drawerView` | `app.js:1250` |

### Rules

| # | Rule | Status | Evidence |
|---|---|---|---|
| L1 | Each round has at most one request in flight. | holds | `health.js:166`, `health.js:129`, `chat.js:86`, `pulse.js:236`, `repair.js:12`, `repair.js:25` |
| L2 | Each `failed` state has an exit. | **broken** for `connection` | See defect D1. |
| L3 | A chat snapshot changes the round only for the current room. | holds | `chat.js:97` compares `result.room` with `roomState().room` before `chat.js:115`. |
| L4 | One loop owns the Host retry clock. | holds | `health.js:32-39`, `health.js:92-96`. A failure that chat reports during a pending health round is dropped (`health.js:35`). The pending round decides. |
| L5 | `reason` is set only in `failed`. | **broken** for chat | See defect D2. |
| L6 | `houseReturn` is set only while `activeId` is `house`. | **broken** in the data, with no effect | `openConversation` (`app.js:516-533`) does not clear it. Its only reader, `leaveHouse` (`app.js:1431-1438`), runs only when `activeId` is `house` (`app.js:1441`, `app.js:1880`). |

### Defects

- **D1. A failed connection stays failed.**
  `connectRooms` runs once, at `app.js:2063`. No other writer moves `connection` out of `failed` (`app.js:2051`).
  The recovery listener (`app.js:2067-2071`) does not call `connectRooms`.
  The repair banner then shows `connection.reason` (`app.js:1361-1363`) until a page reload.
  The local proxy answers `/local/rooms` itself (`proxy.rs:74-94`, `proxy.rs:290-291`), so Host recovery has no effect on this round.
- **D2. A pending chat round keeps the old failure reason.**
  `chat.js:92` copies `reason` from the failed round into `pending`.
  `chatBlockReason` then shows the old reason (`chat.js:52`), and `schedulePoll` stays off (`chat.js:76`) until `chat.js:115` clears it.
- **D3. A failed repair round keeps the old matrix.**
  `repair.js:19` and `repair.js:38` copy `matrix` into `failed`. `renderRepair` shows those rows (`repair.js:46`).
- **D4. The view state has no single writer.**
  More than eight functions and event handlers write the six fields. `render` writes `mode` (`app.js:1379`), and `renderInspector` clears `selectedMessageKey` (`app.js:1163`).
- **D5. The retry clock permits a null timer with `attempt > 0`.**
  `health.js:170` clears the timer and keeps `attempt` while the round is in flight. `health.js:182` or `health.js:186` sets it again.
- **D6. A throw from `requestRender` at `health.js:132` leaves `roomInFlight` true.**
  That call is outside the `try` block (`health.js:133-147`). After such a throw, the room round never runs again. `unverified`: not run.

### Target shape

1. **Owner.** One JavaScript module owns every round. It exposes one pure `step(round, event) → round`.
   A round is one tagged value: `idle`, `pending`, `live(data)`, or `failed(reason)`. Only `failed` carries `reason`. This fixes D2, D3, and D5.
2. **Connection.** Add a `retry` event to `connection`. The repair round and the Host recovery listener send it. This fixes D1.
3. **View.** Move the six view fields behind one `navigate(event)` function. `render` only reads. This fixes D4.
4. **Rust and SQL.** No change. No Rust step function or SQL trigger applies, because no layer persists these values.
5. **Bend2.** None. Unit tests on `step` are sufficient for local values.

### Open rulings

- **R1. Connection recovery.** Proposal: run `connectRooms` again after each repair action and on Host recovery.
- **R2. Bend2 scope.** Proposal: keep the GUI rounds out of Bend2.

## gui-desktop repair proxy

### What it is

The desktop proxy answers `/local/repair/status` and `/local/repair/start` (`proxy.rs:281-296`).
Each request starts the repair executable as a child process (`proxy.rs:96-123`, `proxy.rs:312-360`).
The proxy keeps no record of a running repair. `Proxy` fields are set once in `new` (`proxy.rs:53-67`).
A search of `gui-desktop/src` for `Mutex`, `Atomic`, `RwLock`, `Semaphore`, and `OnceLock` found no match.

### States

The proxy has no state field.
Each request has an implicit state: `running` from `spawn_blocking` (`proxy.rs:118`) to its answer.

### Transitions today

| Move | Writer |
|---|---|
| request → `running` | `run_repair`, `proxy.rs:116-118` |
| `running` → JSON answer | `proxy.rs:119` |
| `running` → 502 with `hop: "repair"` | `proxy.rs:120-121`, `repair_error` `proxy.rs:304-310` |

### Rules

| # | Rule | Status | Evidence |
|---|---|---|---|
| L1 | One repair start runs at a time for each device. | **broken** | See defect D1. |
| L2 | Only a same-origin POST starts a repair. | holds | `proxy.rs:265-273`, `proxy.rs:287-289` |
| L3 | A status request starts nothing. | holds | `proxy.rs:97` passes `start = false`; `proxy.rs:339` sends `status`. |

### Defects

- **D1. Two repair starts can run at the same time.**
  The GUI guard (`repair.js:25`) covers one window only. Two windows, or a status and a start, start two children.
  The repair executable's own guard is `unverified`.
- **D2. A repair child has no time limit.**
  The non-elevated path waits on `output()` (`proxy.rs:338-341`) with no limit. The GUI repair round stays `pending` until it returns.
  The elevated path (`elevated_start`, `proxy.rs:363-438`) is `unverified`.

### Target shape

1. **Owner.** Rust, in `Proxy`. Add one `try_lock` around `run_repair` for `start`. A second start gets 409.
2. **Limit.** Kill the child after a fixed time. This fixes D2.
3. **SQL.** None. **Bend2.** None.

### Open rulings

- **R1. Second start.** Proposal: refuse with 409 and the text "a repair is already running". A queue would hide a stuck first run.
- **R2. Time limit.** Proposal: 60 seconds for `status`. Sol sets the limit for `start`, because a service start can be slow.

## Orihon views merging

### What it is

Each room has two persisted views, `chat` and `compactor` (`origami/src/orihon/live/mod.rs:314-324`).
Each view has a `merging` flag with hysteresis and a `revision` counter.
The Host organ calls `maintain` at each message boundary and saves both views (`host/src/organ/orihon/live.rs:865-873`).

### States

- Rust: `View { revision, units, merging, lines }`, `mod.rs:317-324`. Default: `revision = 1`, `merging = false` (`mod.rs:326-335`).
- SQL: `orihon_views`, `0041_orihon_live.sql:262-270`. `revision >= 1` (`:265`). `merging BOOLEAN NOT NULL` (`:267`).

### Transitions today

| Move | Writer |
|---|---|
| chat `false` → `true` (above 128 KB) | `mod.rs:501-503` |
| chat `true` → `false` (at 64 KB) | `mod.rs:506` |
| compactor `false` → `true` (above 32 KB) | `mod.rs:508-510` |
| compactor `true` → `false` (at 16 KB) | `mod.rs:513` |
| both `revision + 1` when either view changes | `mod.rs:515-518` |
| persist | `save_views`, `live/store.rs:1107-1134`; upsert `store.rs:1115-1119` |

### Rules

| # | Rule | Status | Evidence |
|---|---|---|---|
| L1 | One writer owns a room's views. | holds | Owner lock `store.rs:484-523`: advisory lock `:489`, file lock `:511`; `open` refuses without it (`store.rs:531-533`). |
| L2 | After `maintain`, `merging` is true only above the low mark. | holds | `mod.rs:506`, `mod.rs:513` |
| L3 | The stored `revision` never decreases. | holds in Rust; **broken** in SQL | `maintain` only adds (`mod.rs:516-517`). The upsert accepts any value (`store.rs:1117`). |
| L4 | A saved view tiles `0..units` with no gap. | holds | `validate`, `mod.rs:368-384`; called at `store.rs:1108-1109`. |

### Defects

- **D1. The upsert accepts a lower revision.**
  `store.rs:1117` sets `revision = EXCLUDED.revision` with no comparison. The CHECK at `0041:265` permits any value of 1 or more.
  A writer without the owner lock, or a restore, can move the revision back. No such writer is known today.

### Target shape

1. **Owner.** Rust. `maintain` is already the single step function.
2. **SQL.** Add `WHERE orihon_views.revision <= EXCLUDED.revision` to the upsert. `save_views` fails when a row is not written. This fixes D1.
3. **Bend2.** Optional. A small model of the two hysteresis bands can prove L2.

### Open rulings

- **R1. Revision guard in SQL.** Proposal: add the guard, and permit an equal revision. A failed save keeps `dirty` set (`host/src/organ/orihon/live.rs:869-871`). The next save then sends the same revision, and the first save can be in the table when only its answer was lost.

## Crane outbox

### What it is

A paper-boat memory insert queues one outbound event in `crane_outbox`.
A publisher leases the row, publishes it to JetStream, and records the result.
`Nats-Msg-Id` carries the event id (`origami/src/cranes/broker.rs:316`), so JetStream collapses a repeat publish inside its duplicate window.

### States

- SQL: `pending`, `leased`, `published`, `dead_letter`, `0016_boat_ready_delivery.sql:14-15`. Shape CHECKs `0016:26-31`. Rename from `boat_ready_outbox`: `0017_crane_delivery.sql:18-31`.
- Rust: no enum. Writers use string literals in `cranes/outbox.rs`.
- Counter: `attempts`, `0016:16` (`>= 0`, no upper bound).

### Transitions today

| Move | Writer |
|---|---|
| → `pending` | trigger `substrate_enqueue_boat_ready`, `0017:214-267`; `AFTER INSERT ON memories`, `0017:270-273` |
| `pending` or expired `leased` → `leased`, `attempts + 1` | `claim_next`, `outbox.rs:121-150` (`:126-127`, `:133-136`) |
| `leased` → `published` | `mark_published`, `outbox.rs:158-162` |
| `leased` → `pending` (retry) | `mark_publish_failure`, `outbox.rs:188-192` |
| `leased` → `dead_letter`, plus a `crane_dead_letters` row | `dead_letter_claim`, `outbox.rs:209-243` |
| callers | `delivery.rs:107`, `:116`, `:123`, `:138`, `:151` |

### Rules

| # | Rule | Status | Evidence |
|---|---|---|---|
| L1 | Only the lease owner completes a row. | holds | `outbox.rs:162`, `:192`, `:222` compare `state = 'leased' AND lease_owner`. |
| L2 | A row ends after 10 attempts. | **broken** | See defect D1. |
| L3 | `published` and `dead_letter` are terminal. | holds in Rust only | Each completion requires `leased`; `claim_next` selects only `pending` and `leased` (`outbox.rs:126-127`). No SQL rule refuses a move out. |
| L4 | A publisher dead letter has its `crane_dead_letters` row. | holds | One transaction, `outbox.rs:217-243`. |
| L5 | Lease columns, `published_at`, and `dead_lettered_at` match the state. | holds | CHECKs `0016:26-31` |

### Defects

- **D1. The claim does not cap `attempts`.**
  `claim_next` adds 1 on each claim (`outbox.rs:134`). The cap applies only in `mark_publish_failure` (`outbox.rs:183`).
  A publisher that stops after each claim lets `attempts` grow past 10, and the row never becomes a dead letter.
- **D2. Two copies decide exhaustion.**
  `outbox.rs:183` and `delivery.rs:140` both compare `attempts >= MAX_PUBLISH_ATTEMPTS`.
- **D3. A completion after lease expiry is accepted when nobody reclaimed the row.**
  The completion updates do not compare `lease_expires_at` with `NOW()` (`outbox.rs:162`, `:192`, `:222`).
  After a reclaim, the first publisher's completion fails (`outbox.rs:169-173`), and JetStream drops the repeat by `Nats-Msg-Id`.
- **D4. The trigger upsert rewrites a row in any state.**
  `ON CONFLICT` sets `room`, `integrity_sha256`, and `payload` with no state condition (`0017:254-263`).
  A conflict needs a second insert of the same memory id. The FK is `ON DELETE RESTRICT` (`0016:10`). So no path to D4 is known today. `unverified`: not run.

### Target shape

1. **Owner.** Both. Rust keeps the moves in `outbox.rs`, with one `exhausted(attempts)` function. This fixes D2.
2. **SQL.** Add a `BEFORE UPDATE` trigger that refuses a move out of `published` or `dead_letter`, and add a state condition to the trigger upsert. This makes L3 hold in SQL and closes D4.
3. **Claim.** `claim_next` moves a row whose `attempts` reached the cap to `dead_letter`. This fixes D1.
4. **Bend2.** Model two publishers, lease expiry, and reclaim. Prove L1, L2, and L3.

### Open rulings

- **R1. Cap at claim.** Proposal: a claim of a row at the cap writes `dead_letter` with reason `publish_exhausted`.
- **R2. Completion after expiry (D3).** Proposal: accept it. The owner check already proves that no other publisher holds the row.

## Insula writer latch

### What it is

The Insula emitter queues telemetry rows and drains them to PostgreSQL in one task.
The `accepting` flag stops new rows at shutdown.
The emitter is a process singleton (`EMITTER`), created once (`akasha/src/insula_writer.rs:149-175`).

### States

- `accepting: AtomicBool`, `insula_writer.rs:37`: `accepting` / `draining`.

### Transitions today

| Move | Writer |
|---|---|
| → `accepting` | init, `insula_writer.rs:164` |
| `accepting` → `draining` | `flush_insula_emitter`, `insula_writer.rs:415` |

Readers: `insula_writer.rs:55`, `:94`, `:190`, `:248`, `:339`, `:385`.
The write at `insula_writer.rs:1009` is in the test module, which starts at `insula_writer.rs:639`.

### Rules

| # | Rule | Status | Evidence |
|---|---|---|---|
| L1 | The latch moves one way. | holds | Only `:415` writes `false` in production code. `init` returns when the emitter exists (`:150`). |
| L2 | Each queued row is persisted, dropped with a receipt, or counted in `unaccounted` at shutdown. | **broken** at shutdown | See defect D1. |

### Defects

- **D1. A row can pass the latch after shutdown starts.**
  `enqueue` reads the latch (`:55`), then adds to `unaccounted` (`:59`), then sends (`:60`).
  `flush_insula_emitter` can set the latch (`:415`) between these steps. The drain can exit (`:572-573`) before the send.
  If `:59` runs after the read at `:442`, the shutdown report undercounts. `unverified`: not run.

### Target shape

1. **Owner.** Rust. The latch already has one writer.
2. **Fix.** Read `unaccounted` after the drain task ends, and close the sender before the drain exits. This fixes D1.
3. **SQL.** None. **Bend2.** None.

### Open rulings

- **R1. D1: fix or accept.** Proposal: accept. The loss is at most a few telemetry rows at process exit, and the report text already admits unaccounted rows.

## Orihon folder closed and cache

### What it is

The Orihon folder engine runs summary calls.
The `closed` flag stops new calls when the Host closes the engine.
The cache gate makes sure that one call at a time writes each hosted cache prefix.

### States

- `closed: AtomicBool`, `akasha/src/orihon_folder/engine.rs:88`: `open` / `closed`.
- Cache entry for each key: absent, `Writing`, or `Written(at)`, `engine.rs:874-878`. "Absent" has no variant.

### Transitions today

| Machine | Move | Writer |
|---|---|---|
| closed | → `open` | init, `engine.rs:166` |
| closed | `open` → `closed` | `close`, `engine.rs:224` |
| cache | absent → `Writing` | `admit`, `engine.rs:907` |
| cache | `Writing` → `Written` | `Admission::drop`, `engine.rs:956` |
| cache | `Writing` → absent | `Admission::drop`, `engine.rs:958` |
| cache | `Written` → `Written` (lifetime renewed) | `engine.rs:964-966` |
| cache | expired `Written` → absent | `engine.rs:891-893` |

### Rules

| # | Rule | Status | Evidence |
|---|---|---|---|
| L1 | `closed` moves one way. | holds | Only `engine.rs:224` writes `true`. |
| L2 | No new call starts after close. | holds | Checks before and after the permit, `engine.rs:190`, `engine.rs:201`. `close` takes every permit, `engine.rs:225`. |
| L3 | One call at a time writes a key. | holds | `admit` waits on any `Writing` key in the chain (`engine.rs:894-899`) under one mutex (`engine.rs:890`). |
| L4 | Every `Writing` entry leaves. | holds | `Admission::drop` runs on success, failure, and cancellation (`engine.rs:946-960`). |

### Defects

None found.

### Target shape

No change. Rust owns both machines with one writer each. **SQL.** None. **Bend2.** None.

### Open rulings

None.

## HarnessOwner

### What it is

`HarnessOwner` starts, stops, and watches the harness processes of one Athanor (`athanor-install/src/harness/owner.rs:36-41`).
One mutex guards its state (`owner.rs:39`).

### States

- `OwnedState`, `owner.rs:23-28`: `children`, `failures`, `shutting_down`.
- Each harness is `Running` (in `children`), `Failed` (in `failures`), or `Stopped` (in neither).
- `status` also reports `Running` when another owner holds the keeper lock (`owner.rs:216-238`).
- `shutting_down` is a latch.

### Transitions today

| Move | Writer |
|---|---|
| `Stopped`/`Failed` → `Running` | `adopt`, `owner.rs:176-179` |
| → `Failed` (spawn error) | `owner.rs:181-186` |
| keeper held elsewhere, failure cleared | `owner.rs:164-166` |
| `Running` → `Stopped` or `Failed` (exit observed) | `observe`, `owner.rs:203-209` |
| `Running` → `Stopped` (stop) | `stop_owned`, `owner.rs:254-255` |
| `Running` → released (cutover) | `release_owned`, `owner.rs:272-273` |
| → `shutting_down` | `shutdown`, `owner.rs:132` |

### Rules

| # | Rule | Status | Evidence |
|---|---|---|---|
| L1 | A harness is never in `children` and `failures` at once. | holds by the writers | `owner.rs:177-178`, `:204-207`, `:254-255`, `:272-273`. `:182-184` runs only when no child exists (`:97-99`, `:119-121`). |
| L2 | No start or restart after shutdown starts. | holds | `owner.rs:93-95`, `owner.rs:115-117` |
| L3 | `shutting_down` moves one way. | holds | Only `owner.rs:132` writes it. |
| L4 | A status request answers without a long wait. | **broken** | See defect D1. |

### Defects

- **D1. The state lock stays held during a stop.**
  `stop_owned` waits up to `STOP_TIMEOUT`, 15 seconds (`owner.rs:13`, `owner.rs:295-301`), under the lock.
  `shutdown` holds the lock across every harness (`owner.rs:131-150`). `list` waits for the lock (`owner.rs:83`).
- **D2. The type permits both `children` and `failures` for one harness.**
  Two maps hold one state (`owner.rs:25-26`). The writers keep them exclusive today.

### Target shape

1. **Owner.** Rust. One map from harness id to one enum: `Running(child)`, `Stopping`, `Failed(text)`. This fixes D2.
2. **Lock.** Set `Stopping`, release the lock, wait, then take the lock again. Start and restart refuse a `Stopping` harness. This fixes D1.
3. **SQL.** None. **Bend2.** None, because the state is process-local.

### Open rulings

- **R1. D1: add `Stopping`.** Proposal: add it. A status call then shows a stop in progress, and nothing waits 15 seconds.

## Adapter portable attachment (`session.ts`)

### What it is

The OMP adapter attaches each conversation to a room on the Host.
An attachment record holds the result, and timers keep its lease alive.
All values live in process memory.

### States

- `Attachment.status`: `pending`, `attached`, `failed` (`adapters/omp/house-proof/session.ts:45`).
- `Attachment.connected` (`session.ts:67`) and `Attachment.exiting` (`session.ts:66`).
- Process sets: `attachments` (`session.ts:135`), `homeOff` (`:139`), `keeperInstanceRoom` (`:166`), `roomInstances` (`:167`), `retiredByHost` (`:172`).

### Transitions today

| Move | Writer |
|---|---|
| → `attached`, `connected` | `attachConversation`, `session.ts:285-313`, stored `:321` |
| → `failed` (attach refused) | `failedAttachment`, `session.ts:337-354` |
| `attached` → `failed` (`instance_stale`) | `recoverLease`, `session.ts:425-429` |
| `attached` → `failed` (`adapter_restart_required`) | `session.ts:443` |
| `connected` → true | heartbeat `session.ts:462`, poll `session.ts:666` |
| `connected` → false | `session.ts:426`, `:440`, `:453` |
| `exiting` → true | `session.ts:595` |
| removed (detach) | `detachAttachment`, `session.ts:326-335` |
| `retiredByHost` add / delete | add `:274`, `:429`; delete `:276`, `:320` |
| `homeOff` add / delete | `:771-772`, `:1085` |

### Rules

| # | Rule | Status | Evidence |
|---|---|---|---|
| L1 | Each declared status has a writer. | **broken** | See defect D1. |
| L2 | A `failed` attachment is not `connected`. | holds at the failure writers; **broken** by a race | `:350`, `:426`, `:440`. See defect D2. |
| L3 | A Host-retired room re-attaches only by an explicit `/spirit`. | holds | `session.ts:785-788`; cleared only on attach, `:276`, `:320` |
| L4 | An exiting attachment does not detach or heartbeat. | holds | `session.ts:328`, `session.ts:459` |

### Defects

- **D1. `pending` has no writer.**
  A search of `session.ts` for `"pending"` finds the type (`:45`) and message text (`:1100`) only.
- **D2. A heartbeat can mark a failed attachment connected.**
  `heartbeat` awaits the Host (`:461`) and then sets `connected = true` (`:462`) with no status check.
  A poll that gets `instance_stale` in that window sets `failed` first (`:425-426`). `unverified`: not run.

### Target shape

1. **Owner.** The adapter, in TypeScript. One tagged union: `attached(connected)`, `failed(error)`, `exiting`. One pure `step` writes it. This fixes D1 and D2.
2. **Rust and SQL.** No change on this side. The Host lease belongs to the Host files.
3. **Bend2.** None for this machine.

### Open rulings

- **R1. `pending`.** Proposal: remove it from the type. No path needs it.

## Top-level session fence

### What it is

The fence maps each room to its one top-level OMP session in this process (`adapters/omp/house-proof/top-level-session-fence.ts:1`).
Chat, presence, restart, and worker gates read it.
`heldBinding` in `index.ts` is a second record of the same holder (`adapters/omp/index.ts:437`).

### States

- Fence: room → session, or absent (`top-level-session-fence.ts:1`).
- `heldBinding`: `null` or one binding (`index.ts:437`).

### Transitions today

| Move | Writer |
|---|---|
| set (replace) | `registerTopLevelSession`, `fence.ts:3-8`; called at `index.ts:512` |
| set if absent (first wins) | `adoptTopLevelSession`, `fence.ts:12-16`; called at `index.ts:475` |
| clear (holder only) | `retireTopLevelSession`, `fence.ts:19-24`; called at `index.ts:444` |
| `heldBinding` set | `index.ts:471` |
| `heldBinding` clear | `releaseHeldBinding`, `index.ts:438-447` |

Readers: `chat.ts:129`, `:148`, `:155`, `:224`, `:242`, `:285`; `presence.ts:38`; `restart-door.ts:200`; `tools.ts:150-152`; `index.ts:407`, `:479`, `:828`.

### Rules

| # | Rule | Status | Evidence |
|---|---|---|---|
| L1 | A worker never holds the room. | holds | `index.ts:467`, `index.ts:505`; worker predicate `isWorkerSession`, `worker-lifecycle.ts:485-500` |
| L2 | Only the holder clears its entry. | holds | `fence.ts:21` |
| L3 | With no holder, worker tools refuse. | holds | `tools.ts:150-151` |
| L4 | `heldBinding` names the fence holder. | **broken** in the data | See defect D1. |

### Defects

- **D1. The two records can disagree.**
  `index.ts:471` sets `heldBinding` before the first-wins adoption at `index.ts:475`. If another session holds the fence, the adoption does nothing.
  The chat doorman then idles (`chat.ts:129`). The knock doorman starts at `index.ts:477`, and `knock.ts` does not read the fence.
  No path that reaches this state is known. `unverified`: not run.

### Target shape

1. **Owner.** The adapter, in TypeScript. Merge `heldBinding` into the fence module as one record with `hold`, `switch`, and `release` events. This fixes D1.
2. **Rust and SQL.** None. **Bend2.** None.

### Open rulings

- **R1. Knock doorman gate.** Proposal: the knock doorman reads the fence, the same as the chat doorman.

## Chat log sequence and draft generation

### What it is

`ChatLog` keeps the Host chat ring and the open drafts (`host/src/chat.rs:27-34`).
It persists them in two files, `chat.json` and `chat-drafts.json`.
`next_sequence` numbers ring lines. `draft_generation` tells which draft file matches the ring file.
The say lifecycle on top of this ring belongs to [chat-say.md](chat-say.md).

### States

- `next_sequence: u64`, `chat.rs:31`. `draft_generation: u64`, `chat.rs:32`.
- `ChatCheckpoint { entries, draft_ids, next_sequence, draft_generation }`, `chat.rs:37-44`.
- `DraftCheckpoint { generation, drafts }`, `chat.rs:46-51`.

### Transitions today

| Move | Writer |
|---|---|
| `next_sequence + 1`, write `chat.json` | `append`, `chat.rs:278-281`, `:293`, `persist` `:300` |
| `next_sequence - 1` (write failed) | `chat.rs:305` |
| `draft_generation + 1`, write `chat-drafts.json` | `persist_drafts`, `chat.rs:122-136` |
| load and compare | `open`, `chat.rs:66-103`; called at `server.rs:207` |

### Rules

| # | Rule | Status | Evidence |
|---|---|---|---|
| L1 | Each ring line has a sequence lower than `next_sequence`. | holds | `append` stamps and then adds 1 (`chat.rs:283`, `chat.rs:293`). Open checks the last line (`chat.rs:95-98`). |
| L2 | A failed write leaves memory as it was. | holds | `chat.rs:300-306`, `chat.rs:224-232`, `chat.rs:183-187` |
| L3 | The draft file generation is never older than the ring file's. | holds while both files exist | `persist_drafts` sets the generation after the write (`chat.rs:132-134`); `persist` writes that generation (`chat.rs:117`). Both writes are atomic (`store.rs:673`). |
| L4 | Open restores only the drafts that the ring names. | holds | `chat.rs:73-85` |
| L5 | The loss of `chat-drafts.json` does not stop the Host. | **broken** | See defect D1. |

### Defects

- **D1. A missing draft file stops the Host start.**
  `read_checkpoint` returns generation 0 for a missing file (`chat.rs:57`).
  When `chat.json` holds a higher `draft_generation`, `open` fails (`chat.rs:70-71`). The Host constructor returns that error (`server.rs:188-193`, `server.rs:207`).
  The drafts are transient. A missing `chat.json` gives an empty ring and no error.

### Target shape

1. **Owner.** Rust. `ChatLog` methods are the single owner.
2. **Open.** When the draft file is missing or older, open with no drafts and log one warning. This fixes D1.
3. **SQL.** None. **Bend2.** None. A Rust test that removes the draft file and opens the log is sufficient.

### Open rulings

- **R1. D1: repair or refuse.** Proposal: repair. Drafts are transient, and the settled turn replaces them. A stopped Host costs more than a lost draft.
