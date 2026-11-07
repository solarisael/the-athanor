# Worker lifecycle

Status: `census` (2026-11-06).

## What it is

A worker is one bounded task child that a parent session spawns.
One native task call owns a swarm. Each task slot owns one book.
Each book holds one or more instances, and each successor instance links to its predecessor.
An instance moves through `working`, `wrapping`, `handoff_only`, and `retired` as its context fills.
The rules live in one engine, `hearth::worker_lifecycle`.
Two stores persist the engine output: PostgreSQL (AKASHA) and the Host Vault journal.
The OMP adapter keeps a local copy of the state in each parent session and in each worker session.

## States

### SQL (`substrate/migrations/0034_worker_lifecycle.sql`)

- `instances.state`: `working`, `wrapping`, `handoff_only`, `retired` (`0034:168-169`).
- `instances.settled_outcome`: NULL, `completed`, `failed`, `aborted` (`0034:174`).
- Instance side fields: `warned` (`0034:170`), `handoff_requests` 0 to 2 (`0034:171`), `checkpoint_sequence` (`0034:172`), `handoff` (`0034:173`), `bound_session` (`0034:165`).
- One combination CHECK: a retired instance has a handoff (`0034:182-183`).
- `swarms.disposed_at`: NULL is live, set is disposed (`0034:138`).
- `entries` is append-only until the swarm is disposed (trigger `0034:214-231`).
- `retained` and `dispositions` are append-only (triggers `0034:259-275`).

### Engine (`crates/hearth/src/worker_lifecycle/`)

- `InstanceState` (`model.rs:8-13`) derives `Ord` (`model.rs:6`). The order is the transition order.
- `settled_outcome` is `Option<String>` (`model.rs:114`). The typed `SettleOutcome` exists only in the request (`request.rs:91-95`).
- `SwarmRecord.disposed_ms` (`model.rs:46`) is the disposal latch.
- `InstanceRecord::inactive` (`model.rs:128-130`): `Retired`, or any `settled_outcome`.
- Thresholds: warn at 40 %, handoff-only at 60 %, 2 handoff requests (`mod.rs:39-43`).
- `apply` (`engine.rs:422-508`) is the one entry. It takes the call, the swarm aggregate, the time, and an id source. It returns a `Step` with effects.

### Stores

- AKASHA: `worker_lifecycle` (`crates/akasha/src/worker_lifecycle/mod.rs:43-110`) locks the swarm, replays a committed request, calls `apply`, and persists.
- Vault: the Host journal (`crates/host/src/organ/worker_lifecycle.rs:1-13`) does the same steps over an append-only JSONL file.

### Adapter (`adapters/omp/house-proof/worker-lifecycle.ts`)

- `WorkerState` (`worker-lifecycle.ts:93-110`) holds `state` (`LifecycleState`, `:69`), a separate `retired` flag, `handoffComplete`, and the counters `handoffRequests`, `remainingRequests`, `checkpointFailures`, `notebookGaps`.
- `Reservation` (`:76-91`) holds two independent flags, `consumed` and `released`.
- `SlotRecord` (`:121-132`) holds two independent flags, `started` and `settled`.

The effective reservation states are `Pending`, `Reserved`, `Consumed`, and `Released`.
The effective slot states are `Unstarted`, `Started`, and `Settled`.
No field names these states.

## Transitions today

| Layer | Move | Writer |
|---|---|---|
| Engine | → `working` (new swarm, book, instance) | `prepare_new`, `engine.rs:568`; `new_instance`, `engine.rs:539` |
| Engine | → `working` (successor) | `prepare_successor`, `engine.rs:636-716` |
| Engine | bind session, clear `settled_outcome` | `open_reservation`, `engine.rs:794`, `engine.rs:800` |
| Engine | `working` → `wrapping` → `handoff_only` (usage) | `observe_usage`, `engine.rs:1033`; `warned` at `engine.rs:1036` |
| Engine | `handoff_only`: count one request | `engine.rs:1039-1041` |
| Engine | `handoff_only`: budget spent, signal only | `retire_required`, `engine.rs:1043`, `engine.rs:1067` |
| Engine | any → `retired` (terminal checkpoint, or checkpoint in `handoff_only`) | `checkpoint_report` → `retire`, `engine.rs:1151-1162` |
| Engine | checkpoint, no state change | `engine.rs:1171` |
| Engine | any → `retired` + `settled_outcome` failed or aborted | `settle_instance`, `engine.rs:1215-1227` |
| Engine | set `settled_outcome = completed`, state unchanged | `engine.rs:1229-1245` |
| Engine | repair an incomplete handoff | `repair_handoff`, `engine.rs:1353-1357` |
| Engine | swarm live → disposed, books cleared | `dispose_swarm`, `engine.rs:1439-1440` |
| AKASHA | instance full-image upsert | `akasha/.../mod.rs:447` |
| AKASHA | swarm `disposed_at` | `mod.rs:362` |
| AKASHA | disposition row, then cascade delete | `mod.rs:372`, `mod.rs:386-390` |
| Vault | journal line, full book image | `append`, `worker_lifecycle.rs:522-536`; replay `absorb`, `worker_lifecycle.rs:277` |
| Adapter | reservation → `Pending` | `worker-lifecycle.ts:774` |
| Adapter | `Pending` → `Reserved` | `worker-lifecycle.ts:755-756` |
| Adapter | `Reserved` → `Released` | `worker-lifecycle.ts:839-840` |
| Adapter | `Reserved` → `Consumed` | `worker-lifecycle.ts:864-865` |
| Adapter | slot `Unstarted` → `Started` | `linkSlot`, `worker-lifecycle.ts:381` |
| Adapter | slot → `Settled` (Host answered `retired`) | `settleSlot`, `worker-lifecycle.ts:905-907` |
| Adapter | slot `Unstarted` → `Settled` (never started) | `cancelUnstartedSlots`, `worker-lifecycle.ts:983-984` |
| Adapter | worker state from open | `worker-lifecycle.ts:1050`, persist `:1055` or `:1059` |
| Adapter | worker state from observe | `applyObservation`, `worker-lifecycle.ts:1115`, persist `:1119` |
| Adapter | worker state from checkpoint | `worker-lifecycle.ts:1329-1331` |
| Adapter | worker → `retired` | `markRetired`, `worker-lifecycle.ts:700-705` |
| Adapter | replay from the session file | `restore`, `worker-lifecycle.ts:388-477` |

The cross-layer links:

- Every adapter write goes to the Host as a `worker_lifecycle` call, which runs `apply` once.
- The engine answers `retireRequired`. The adapter then sends `settle failed` (`worker-lifecycle.ts:1194-1195`, `:718-733`).
- The parent settles each slot with the native task status when the child ends (`worker-lifecycle.ts:894-903`).
- Orihon reads liveness from the store: SQL text at `crates/host/src/organ/orihon/mod.rs:467`, or the Vault journal at `worker_lifecycle.rs:417`.

## Rules

| # | Rule | Status | Evidence |
|---|---|---|---|
| L1 | An instance state only moves right. | holds in the engine | `max` at `engine.rs:1033`; only `retire` writes `Retired` (`engine.rs:1123`). The stores do not enforce it: see D1. |
| L2 | A retired instance has a handoff record. | holds | CHECK `0034:182-183`; `engine.rs:1114-1123` |
| L3 | A retired instance cannot open, read, append, observe, or checkpoint. | holds | `engine.rs:750`, `:875`, `:903`, `:1011`, `:1147` |
| L4 | A successor needs the last instance, retired, with a usable handoff. | holds | `engine.rs:656-674` |
| L5 | `handoff_requests` stays in 0 to 2 and grows only in `handoff_only`. | holds | `engine.rs:1039-1044`; CHECK `0034:171` |
| L6 | An instance with a spent budget retires. | **broken when the settle call fails** | See defect D2. |
| L7 | `failed` and `aborted` settlement retire the instance. | holds in the engine | `engine.rs:1215-1227`. SQL does not check it: see D3. |
| L8 | Disposal runs once, over inactive instances only. | holds | `engine.rs:1375-1388` |
| L9 | Entries change only by whole-swarm disposal. | holds | trigger `0034:214-231`; `disposed_at` is written at `mod.rs:362` before the deletes at `mod.rs:386-390` |
| L10 | All liveness checks agree. | holds today | `model.rs:128-130`, `orihon/mod.rs:467`, `worker_lifecycle.rs:417`. Three copies: see D5. |
| L11 | The adapter state matches the Host state. | **broken** | See defects D2 and D7. |
| L12 | Each slot settles once. | holds | guard `worker-lifecycle.ts:905`, filter `:968` |
| L13 | A reservation is consumed or released, never both. | holds in the writers | Release needs a marker plan (`worker-lifecycle.ts:832`). Consume needs a plan without a marker (`:863`). The type permits both: see D8. |

## Defects

- **D1. The stores do not enforce monotonic order.**
  The AKASHA upsert writes `state=EXCLUDED.state` with no condition (`akasha/.../mod.rs:447`).
  The SQL CHECK lists the values only (`0034:168-169`).
  The Vault journal replays full book images with no order check (`worker_lifecycle.rs:299-305`).
  The engine is the only writer today, so the order holds.
- **D2. A spent handoff budget depends on the adapter to retire.**
  The engine sets `retire_required` and keeps `handoff_only` (`engine.rs:1038-1044`).
  The adapter then sends `settle failed` (`worker-lifecycle.ts:1194-1195`).
  If that call fails, the adapter retires locally and the Host keeps `handoff_only` (`worker-lifecycle.ts:729-731`).
  No Host timer retires the instance.
  The next fix is the parent settlement when the native task ends (`worker-lifecycle.ts:894-903`).
- **D3. SQL permits contradictory combinations.**
  `settled_outcome` `failed` or `aborted` with a state other than `retired` is legal (`0034:174`).
  `handoff` with a state other than `retired` is legal. Only the converse has a CHECK (`0034:182-183`).
  `warned = false` with state `wrapping` or later is legal (`0034:170`). The engine sets both in one step (`engine.rs:1033-1036`).
  `handoff_requests > 0` with a state other than `handoff_only` is legal (`0034:171`).
  The writers keep them consistent today.
- **D4. `settled_outcome` has no Rust type.**
  The model stores `Option<String>` (`model.rs:114`). AKASHA reads the raw text (`akasha/.../mod.rs:259`).
  The SQL CHECK stops an unknown value in PostgreSQL (`0034:174`).
  The Vault journal deserializes the same `Option<String>`, so it accepts any text (`worker_lifecycle.rs:38-52`).
- **D5. The liveness predicate exists three times.**
  `InstanceRecord::inactive` (`model.rs:128-130`) is the engine copy.
  The Orihon PostgreSQL path restates it in SQL text and adds `disposed_at` (`orihon/mod.rs:467`).
  The Orihon Vault path calls `inactive()` and adds `disposed_ms` (`worker_lifecycle.rs:417`).
  The three agree today. A change to one copy does not change the others.
- **D6. `swarms.disposed_at` has no guard.**
  The `swarms` table has no trigger (`0034:130-143`).
  An UPDATE can clear `disposed_at` after the entries are deleted.
  An UPDATE can set `disposed_at` with no `dispositions` row.
  `swarms.disposed_at` and `dispositions.disposed_at` hold the same value twice (`0034:138`, `0034:256`).
  The engine writes both from one `now_ms` (`engine.rs:1437-1439`).
- **D7. The adapter replay is weaker than the live path.**
  Replay changes an unknown `bound` state to `working` (`worker-lifecycle.ts:456`). The live path throws on an unknown state (`:309-315`).
  Replay accepts a `state` line in any order (`:460`). A `state` line after a `retired` line gives `retired === true` with a state other than `retired`.
  `state` and `retired` hold the same fact twice (`:99-100`).
  `markRetired` on an open that answers `retired` (`:1071`) does not run: the engine refuses that open (`engine.rs:750`). This is an inference from the code; no test confirms it (unverified).
- **D8. The adapter flags permit contradictory combinations.**
  `consumed` and `released` are independent (`worker-lifecycle.ts:86-87`).
  `linkSlot` sets `started` with no `settled` check (`:380-381`), so a settled slot can become started.
  The writers keep them consistent today.
- **D9. The predecessor link has no foreign key.**
  `predecessor_instance_id` has no REFERENCES clause (`0034:161`).
  The engine sets it only from a found instance (`engine.rs:651-655`, `:693`).

## Target shape

1. **Bend2 model.** Model the instance, the swarm, the reservation, the slot, and the adapter copy. Prove L1 to L13.
   The proof must fail on L6 and L11 against today's transitions; this shows that it catches real defects.
2. **Engine.** Keep `apply` as the single transition owner. It is already one step function over one aggregate.
   Retire the instance inside `observe_usage` when the budget is spent. This fixes D2 and L6.
   Replace `settled_outcome: Option<String>` with `Option<SettleOutcome>`. This fixes D4.
   Move the liveness predicate to one function that also takes the swarm. Both Orihon paths call it. This fixes D5.
3. **SQL.** Add guards in a new migration, and keep the engine as the owner.
   Add CHECKs for the D3 combinations.
   Add a BEFORE UPDATE trigger on `instances` that refuses a state decrease. This fixes D1 for PostgreSQL.
   Add a BEFORE UPDATE trigger on `swarms` that refuses a change to a set `disposed_at`. This fixes D6.
4. **Vault journal.** On replay, refuse a book image that moves a state left. This fixes D1 for the Vault store.
5. **Adapter.** Replace `state` and `retired` with one tagged state. Replay goes through the same parse and order check as the live path. This fixes D7.
   Replace `consumed` and `released` with one reservation status. Replace `started` and `settled` with one slot status. This fixes D8.
6. **Contract test.** CI runs the same random event sequences through the Bend2 model and through `apply`, and compares the states.

## Open rulings

- **R1. Who retires a spent budget.** Proposal: the engine retires the instance in `observe_usage` with `complete=false` and outcome `failed`, in the same commit. The adapter then only reads the answer. This removes the dependence on a second call.
- **R2. Does `completed` retire.** Proposal: keep today's behavior. `completed` marks the instance inactive and a reopen clears it (`engine.rs:800`, `model.rs:113`). A successor still needs `failed`, `aborted`, or a terminal checkpoint. Record this rule in the module docs.
- **R3. SQL guards.** Proposal: add the CHECKs and the two triggers from Target shape item 3. The engine stays the owner; the guards only refuse a move that the engine never makes.
- **R4. D9: fix or accept.** Proposal: add a self-referencing foreign key on `predecessor_instance_id` with the default `NO ACTION`. Disposal deletes all instances of a swarm in one statement (`akasha/.../mod.rs:388`). PostgreSQL checks a `NO ACTION` key at the end of that statement, so the delete stays legal.
