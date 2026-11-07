# Restart lifecycle

Status: `census` (2026-11-06).

## What it is

A restart is one self-restart of one OMP session.
Its identity is the intent id in `restart.intents`, which `restart_request` creates.
Four layers track one restart: PostgreSQL, the Host journal, the adapter, and the keeper.
PostgreSQL is the authority for the intent.
The Host keeps its own copy of the stage in `RestartRecord.stage`, and no function maps that copy to the SQL state.
The adapter exits the predecessor and verifies a keeper-launched successor.
The keeper (`crates/omp-keeper`) claims and relaunches through the same SQL doors (`keeper.rs:313-316`, `keeper.rs:367-378`).

## States

### SQL (`substrate/migrations/0026_restart.sql`, `0027_restart_successor_proof.sql`)

`restart.intents.state` (`0026_restart.sql:131`) has seven values: `requested`, `claimed`, `exiting`, `relaunching`, `verified`, `failed`, `expired` (`0026_restart.sql:159-163`).
The prose order is `requested` → `exiting` → `claimed` → `relaunching` → `verified` (`0026_restart.sql:6-10`).

- `failed_stage` names the stage a failed row died in (`0026_restart.sql:166-173`).
- The claim tuple is all empty with `claim_epoch = 0`, or all set with `claim_epoch >= 1` (`0026_restart.sql:176-192`).
- `verified_at` and the successor triple are set only on `verified` (`0026_restart.sql:195-206`).
- `restart.successor_proofs` holds one row for the current relaunch attempt (`0027_restart_successor_proof.sql:51-64`).
- `restart.intent_events` is an append-only ledger with the same seven kinds (`0026_restart.sql:241-245`, trigger `0026_restart.sql:273-276`).

The Rust mirror is `RestartState` in `crates/protocol/src/restart/mod.rs:52-60`.
`RestartTransitionTarget` (`crates/protocol/src/restart/mod.rs:67-71`) names the three targets a caller may ask for: `exiting`, `relaunching`, `failed`.

### Store writers (`crates/akasha/src/restart/mod.rs`, `proof.rs`)

Five doors write the intent: `restart_request`, `restart_claim`, `restart_transition`, `restart_verify`, and the lazy sweeps inside `restart_request` (`restart/mod.rs:5-8`, `restart/mod.rs:511-513`).
Each door locks the row with `SELECT … FOR UPDATE` and checks the state in Rust before it writes.
No single function holds the legal moves. `keeper_move` (`restart/mod.rs:816-863`) covers only the keeper moves.

### Host (`crates/host/src/portable.rs`, `portable/control.rs`, `portable/restart.rs`, `portable/ledger.rs`, `portable/attach.rs`)

`RestartRecord.stage` (`portable.rs:237-244`, field `portable.rs:268`) has six values: `Requested`, `AwaitingExit`, `Relaunching`, `HandedToKeeper`, `Verified`, `Failed`.
The journal stores the stage as free text (`ledger.rs:57`).
The restart command also carries `Command.state` (`portable.rs:184-196`, field `portable.rs:229`).
Its terminal set is every state except `Pending`, `Queued`, `Delivered` (`portable.rs:211-213`).

The Host has no stage for SQL `claimed`.
On the Tern path, the Host claims while the record stays `AwaitingExit` and stores the token (`restart.rs:423-426`).

### Adapter (`adapters/omp/house-proof/restart-door.ts`, `session.ts`)

- `verifyingIntent`, `verifiedIntent`: process-local strings for successor verification (`restart-door.ts:203-204`).
- `continuedRestarts`: process-local set that stops a second continuation turn (`session.ts:358`).

The effective states are `Unverified`, `Verifying`, and `Verified` for each intent id.

## Transitions today

| Layer | Move | Writer |
|---|---|---|
| SQL | → `requested` | `restart/mod.rs:519`, ledger row `restart/mod.rs:547-563` |
| SQL | `requested` → `expired` (TTL) | `expire_lapsed_request`, `restart/mod.rs:278`; called at `restart/mod.rs:498`, `restart/mod.rs:610`, `restart/mod.rs:702`, sweep `restart/mod.rs:320` |
| SQL | `requested` → `exiting` | `restart/mod.rs:736`; guard `restart/mod.rs:728`; storm guard `restart/mod.rs:734` |
| SQL | `requested`/`exiting` → `claimed` | `restart/mod.rs:626`; guard `restart/mod.rs:609-619` |
| SQL | `exiting` → `failed:exiting` (no claim, deadline past) | sweep `restart/mod.rs:360`, `restart/mod.rs:368` |
| SQL | `claimed` → `relaunching` | `keeper_move` `restart/mod.rs:823`, `restart/mod.rs:846` |
| SQL | `relaunching` → `relaunching` (retry) | `restart/mod.rs:826-831`, `restart/mod.rs:846` |
| SQL | `relaunching` → `failed:relaunching` (budget spent) | `restart/mod.rs:827-828`, `restart/mod.rs:856` |
| SQL | `claimed`/`relaunching` → `failed` (keeper) | `restart/mod.rs:833-834`, `restart/mod.rs:856` |
| SQL | `claimed`/`relaunching` → `failed` (`claimant_lost`) | sweep `restart/mod.rs:415`, `restart/mod.rs:425` |
| SQL | `relaunching` → `verified` | `restart/mod.rs:935`; guards `restart/mod.rs:902`, `restart/mod.rs:911`, `restart/mod.rs:925` |
| SQL | proof row: write or rotate | `proof.rs:24-28`, called at `restart/mod.rs:774-781` |
| SQL | proof row: delete | `restart/mod.rs:432`, `restart/mod.rs:784`, `restart/mod.rs:944` |
| Host | new record → `Requested` | `control.rs:178-198`, insert `control.rs:297` |
| Host | `Requested` record removed (journal write failed) | `control.rs:310-313` |
| Host | `Requested` → `AwaitingExit` | `control.rs:447`, after `restart::arm` at `control.rs:420` |
| Host | `AwaitingExit` → `Requested` (journal rollback) | `control.rs:455-457` |
| Host | `Requested` → `Failed` (arm refused) | `control.rs:430-433` |
| Host | `Requested` → `Failed` (adapter refused or failed) | `control.rs:469-472` |
| Host | `AwaitingExit` → `Failed` (exit deadline) | `restart.rs:384-390` → `finish_failed`, `restart.rs:577` |
| Host | `AwaitingExit` → `HandedToKeeper` | `restart.rs:404-406` |
| Host | `AwaitingExit` → `Relaunching` | `restart.rs:455-459` |
| Host | `Relaunching` → `Verified` | `verify_successor`, `restart.rs:661-666`; called at `attach.rs:221` and `restart.rs:521` |
| Host | `Relaunching` → `Failed` | `restart.rs:410-411` → `restart.rs:577` |
| Host | load: any text except `verified`/`handedToKeeper` → `Failed` | `ledger.rs:86-92`, `ledger.rs:181-187` |
| Host command | → `Pending` | `control.rs:226` |
| Host command | `Pending` → `Queued` | `control.rs:305` |
| Host command | `Pending` → `Refused`/`OutcomeUnknown` | `settle_pending` `control.rs:332`; called at `control.rs:279`, `control.rs:314-319` |
| Host command | `Queued` → `Delivered` | `poll`, `control.rs:47` |
| Host command | → `Applied` | `control.rs:443` |
| Host command | `Applied` → `Delivered` (journal rollback) | `control.rs:452` |
| Host command | → `Failed` (arm refused) | `control.rs:427` |
| Host command | → `Refused`/`Failed` (adapter) | `control.rs:474` |
| Host command | → `Expired` | deadline `portable.rs:350`; retire `portable.rs:328`; conversation switch `attach.rs:252` |
| Host command | load: non-terminal → `OutcomeUnknown` | `ledger.rs:159-161` |
| Adapter | `Unverified` → `Verifying` | `restart-door.ts:248` |
| Adapter | `Verifying` → `Verified` | `restart-door.ts:260` |
| Adapter | `Verifying` → `Unverified` (refusal or error) | `restart-door.ts:268-270` |

The cross-layer links:

- Host `Requested` requires SQL `requested` (`control.rs:257-286`).
- Host `AwaitingExit` requires SQL `exiting` (`restart.rs:218-267`, `restart.rs:193-214`).
- The adapter exits only after the Host answers the `applied` acknowledge (`session.ts:585-598`).
- Host `Relaunching` requires SQL `relaunching` and a proof (`restart.rs:429-459`).
- Host `Verified` requires SQL `verified` (`restart.rs:648-666`).
- Host `HandedToKeeper` maps to every SQL state after `exiting`. The keeper owns the rest.

## Rules

| # | Rule | Status | Evidence |
|---|---|---|---|
| L1 | Each SQL state change writes one ledger row, and the ledger accepts INSERT only. | holds | `restart/mod.rs:284`, `restart/mod.rs:375`, `restart/mod.rs:433`, `restart/mod.rs:636`, `restart/mod.rs:790`, `restart/mod.rs:945`; trigger `0026_restart.sql:273-276` |
| L2 | One workspace holds at most one live intent. | holds | `0026_restart.sql:223-225`; `restart/mod.rs:477`, `restart/mod.rs:515` |
| L3 | SQL refuses an illegal state move. | **broken** | No trigger guards `state`. The CHECKs test shape only (`0026_restart.sql:159-206`). The Rust doors hold: `restart/mod.rs:614`, `restart/mod.rs:728`, `restart/mod.rs:836-840`, `restart/mod.rs:902`. |
| L4 | A proof row exists only while its intent is `relaunching`. | holds in Rust only | Deletes at `restart/mod.rs:432`, `restart/mod.rs:784`, `restart/mod.rs:944`. No SQL link (`0027_restart_successor_proof.sql:51-64`). |
| L5 | Every live intent leaves within a bounded time. | **broken without a later request** | The sweeps run only inside `restart_request` (`restart/mod.rs:511-513`). See defect D5. |
| L6 | The relaunch count stays inside the budget. | holds | `restart/mod.rs:103`, `restart/mod.rs:827-828` |
| L7 | One intent verifies once. | holds | `restart/mod.rs:902`, `restart/mod.rs:944`; `0026_restart.sql:195-206`; adapter guard `restart-door.ts:211` |
| L8 | Every Host record reaches `Verified` or `Failed`. | **broken** | See defects D1 and D2. |
| L9 | A terminal Host stage agrees with the SQL state. | **broken** | See defects D3, D5, and D6. |
| L10 | A Host `Verified` survives a Host restart. | **broken** | See defect D4. |

## Defects

- **D1. `HandedToKeeper` has no exit.**
  `restart.rs:405` is the only writer.
  `successor_matches` requires `Relaunching` (`restart.rs:606`), so the Host never verifies a keeper successor.
  No Host code reads the SQL outcome for this record, and the journal keeps the stage (`ledger.rs:89`).
- **D2. An expired restart command leaves its record at `Requested`.**
  The restart command deadline is the intent TTL (`control.rs:208-213`).
  `expire` (`portable.rs:342-353`) and `retire` (`portable.rs:321-331`) change commands only.
  No writer moves the record out of `Requested`.
  `attach.rs:130-145` counts `Requested` as live, so the instance cannot switch conversation until the Host restarts.
- **D3. `finish_failed` can overwrite `Verified`.**
  The relaunch loop reads the stage at `restart.rs:490` and releases the lock at `restart.rs:540`.
  An attach can verify at `attach.rs:221` before the deadline test at `restart.rs:541`.
  The loop then returns an error, and `finish_failed` writes `Failed` (`restart.rs:575-579`).
  `update` (`restart.rs:343-359`) has no from-stage guard.
  SQL refuses the `failed` move (`restart/mod.rs:836-840`), so the Host says `Failed` and SQL says `verified`.
  The window is narrow.
- **D4. A `Verified` record can stay unsaved.**
  The early-attach path runs in the driver task (`restart.rs:520-538`).
  `verify_successor` changes the record with no journal write (`restart.rs:661-666`).
  The next save comes only after a later mutating request (`portable.rs:540-546`).
  A Host restart before that save loads `relaunching` as `Failed` (`ledger.rs:86-92`), and SQL says `verified`.
- **D5. A Host `Failed` with no claim token leaves the intent live.**
  The Host writes `Failed` before it calls `fail` (`control.rs:430-435`, `control.rs:469-478`, `restart.rs:575-587`).
  When the claim is refused, `fail` returns with a warning only (`restart.rs:275-287`).
  The intent then holds the workspace until a later `restart_request` sweeps it (`restart/mod.rs:511-513`).
- **D6. `failed_stage` names a stage the restart never reached.**
  `fail` claims the intent only to fail it (`restart.rs:269-276`).
  A restart that the adapter refused at `requested` therefore lands on `failed:claimed` (`restart/mod.rs:833-834`, `restart/mod.rs:856`).
  A Tern exit past its deadline also lands on `failed:claimed`, while the SQL sweep records the same fact as `failed:exiting` with `exit_unclaimed` (`restart/mod.rs:368-384`).
  The CHECK allows `failed_stage = 'requested'` (`0026_restart.sql:171`), and no writer sets it.
- **D7. The SQL types permit contradictory combinations.**
  The claim CHECK ignores `state`, so `claimed` with `claim_epoch = 0` is representable (`0026_restart.sql:176-192`).
  `exiting_deadline_at` and `relaunching_deadline_at` are not tied to a state (`0026_restart.sql:139-140`).
  A proof row can name a `claim_epoch` other than the intent's (`0027_restart_successor_proof.sql:54`).
  The writers keep the claim tuple and the proof consistent today.
  The deadline columns stay set after `claimed` and `verified` (`restart/mod.rs:626`, `restart/mod.rs:935`).
- **D8. The state list exists in three copies.**
  `RestartState::from_str` (`crates/protocol/src/restart/mod.rs:123-134`) and the CHECK (`0026_restart.sql:159-163`) hold the SQL list.
  The Host journal reads unknown stage text as `Failed` (`ledger.rs:86-92`) and unknown command text as `OutcomeUnknown` (`ledger.rs:73-83`), with no error.
- **D9. Host restart records are never pruned.**
  The only removal is `control.rs:312`, on a failed journal write.
  The journal grows by one row for each restart.

## Target shape

1. **Bend2 model.** Model the SQL machine, the Host stage, and the cross-layer links, then prove L1 to L10.
   The proof must fail on L5, L8, L9, and L10 against today's transitions; this shows that it catches real defects.
2. **SQL.** Add one `BEFORE UPDATE` trigger on `restart.intents` that holds the legal edge table.
   The same trigger refuses a proof row unless the intent is `relaunching` with the same `claim_epoch`.
   This fixes L3 and the SQL half of D7.
3. **Store.** Replace the checks at `restart/mod.rs:614`, `restart/mod.rs:728`, `restart/mod.rs:822-840`, and `restart/mod.rs:902` with one pure `step(state, event) → (state, effects)`.
   Each door calls `step`, writes the row, and writes the ledger row.
   The trigger and `step` hold the same edge table, and a contract test compares them.
4. **Host.** Keep `RestartRecord.stage` as a cache of the SQL state, not as a second machine.
   Write it through one `advance(from, to)` that refuses a move out of `Verified` or `Failed`.
   This fixes D3.
   Save the journal in `verify_successor`. This fixes D4.
   Read `restart_status` for `HandedToKeeper` and expired-command records, and settle them from the SQL state. This fixes D1 and D2.
5. **Contract test.** CI runs the same random event sequences through the Bend2 model, through `step`, and through the SQL trigger, and compares the states.
6. **Adapter.** No change. Its state is process-local, and the Host and SQL fence a second verify.

## Open rulings

- **R1. L5: lazy expiry or a clock.** Proposal: keep lazy expiry in SQL. The Host calls the three sweeps for its own workspace when one of its records passes its stage deadline. This frees the workspace with no new service.
- **R2. D5 and D6: how the Host fails an intent it never claimed.** Proposal: add a House-principal fail door from `requested` and `exiting`. It writes `failed:requested` or `failed:exiting` with no claim, so the stage is true and a refused claim cannot leave the intent live.
- **R3. D1: who settles `HandedToKeeper`.** Proposal: the Host reads `restart_status` for the intent and maps `verified` to `Verified` and `failed`/`expired` to `Failed`.
- **R4. D9: record retention.** Proposal: drop terminal records older than 24 hours when the journal saves.
- **R5. Transition owner.** Proposal: both a Rust step function and an SQL trigger, with one edge table and one contract test. SQL stays the authority, and Rust gives the named refusals.
