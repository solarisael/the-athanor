# Hallway knock lifecycle

Status: `census` (2026-11-06).

## What it is

A knock asks a peer room for one bounded turn about one Hallway message.
Its identity is the knock id, a UUID that `knocks::knock` creates.
A root knock opens an exchange. Each child knock continues the exchange on the root's clock.
Four layers track one knock: the SQL row, the Rust store writers, the Host plan, and the adapter doorman.
The SQL row in `hallway_knocks` is the authority.
The doorman claims the knock, injects it into OMP as a turn, and settles the result.
A separate policy row decides which rooms can knock on a room.

## States

### SQL (`substrate/migrations/0021_hallway_knock.sql`, `0044_hallway_knock_expired.sql`)

`hallway_knocks.status` holds six values (`0044:22-24`):

- `pending`: no claim. All claim columns are null (`0044:35-36`).
- `claimed`: a presence holds a 30-second lease (`knocks.rs:41-45`, `knocks.rs:56`).
- `started`: the recipient turn is open. `started_at` is set (`0044:28`).
- `completed`: terminal. `started_at` and `settled_at` are set (`0044:29`).
- `failed`: terminal. A recipient held the turn and did not finish it. `settled_at` is set (`0044:30`).
- `expired`: terminal. Nobody claimed the turn before `expires_at`. Claim columns stay null, and `settled_at` is set (`0044:31`, ruling R1).

Shape CHECKs bind each status to its columns: `0044:25-41`.
Migration 0031 moves the turn ceiling to 20 and the default to 10 (`0031_hallway_knock_turns.sql:25-37`).
Two more states are derived and have no value:

- `Reclaimable`: `claimed` with `lease_expires_at <= NOW()` (`knocks.rs:698`).
- `Overdue`: any `pending`, `claimed`, or `started` row with `expires_at <= NOW()`. The next sweep closes it.
  The lifetime is 3 minutes for each turn, with a minimum of 15 minutes (`knocks.rs:26-36`).

`hallway_knock_policies` has two states: current (`superseded_at` null) and superseded (`0021:22`).
A partial unique index permits one current row for each hallway and room (`0021:30-32`).
No current row means "deny" (`knocks.rs:412-417`).

### Store and Host (`crates/origami/src/hallways/knocks.rs`, `crates/host/src/lifecycle.rs`)

The store writes the row and keeps no state.
`HallwayKnockStatus` and `knock_step(status, event)` in `crates/hearth/src/machines/hallway_knock.rs` own every status move.
The store calls `knock_step` before each UPDATE (`knocks.rs:115-129`). The pointer and the settle receipt carry `HallwayKnockStatus`.
`HallwayKnockOutcome` is the settle request: `Started`, `Completed`, `Failed` (`hallway.rs:498-502`).
`expire_overdue` (`knocks.rs:154-178`) closes every overdue row. `knock`, `claim`, and `settle` call it first, in its own transaction (ruling R4).

The Host stores no knock state.
`KnockPlan` (`lifecycle.rs:27-34`) is a pure decision on the adapter's flags: `lifecycle.rs:124-158`.
It returns `deliver`, a list of settlements, and an optional failure.
Its timers are 25 s for the start settle, 60 s for the turn start, and 25 s for the completion settle (`lifecycle.rs:135-146`).

### Adapter doorman (`adapters/omp/house-proof/knock.ts`)

`KnockDoormanState` (`knock.ts:119-140`) holds the state in fields:

- `active`: the claimed knock pointer, or null.
- `claiming`, `delivered`, `startSettled`, `turnStarted`, `turnEnded`, `settling`: phase latches.
- `interruptRequested`, `interruptSent`, `interruptedTurnEnded`: the interrupt sub-machine.
- `deliveredAt`, `turnEndedAt`: clocks for `KnockPlan`.
- `claimFailures`, `nextClaimAt`, `degradedSince`: claim backoff.

That is nine booleans and four nullable fields.
The effective states are `Idle`, `Claiming`, `Claimed`, `Delivered`, `Started`, and `Ended`.
`Degraded` is a backoff overlay on `Idle`.
No field names these states.

## Transitions today

| Layer | Move | Writer |
|---|---|---|
| SQL | policy → current, old current → superseded | `knocks::policy`, `knocks.rs:323-347`; serialized by the room-state upsert, `knocks.rs:315-322` |
| SQL | → `pending` | `knocks::knock`, `knocks.rs:621-652`; admission `knocks.rs:610`, chain `knocks.rs:612-620` |
| SQL | `pending` → `claimed` | `knocks::claim`, `knocks.rs:684-750`; `knock_step(_, Claim)` |
| SQL | `Reclaimable` → `claimed` (new holder) | same query, `knocks.rs:698` |
| SQL | `pending` past `expires_at` → `expired` | `expire_overdue`, `knocks.rs:157-178`; `knock_step(_, Expire)` |
| SQL | `claimed`/`started` past `expires_at` → `failed` | same sweep |
| SQL | `claimed` (lease active) → `started` | `knocks::settle`, `knocks.rs:829-830`; `knock_step(_, Settle(Started))` |
| SQL | `started` → `completed` | same lines; `knock_step(_, Settle(Completed))` |
| SQL | `claimed` (lease active)/`started` → `failed` | same lines; `knock_step(_, Settle(Failed))` |
| SQL | same outcome and reason again → no move | `knocks.rs:805-813` |
| Host | not delivered → `deliver` | `lifecycle.rs:132-134` |
| Host | timer exceeded → `failure` | `lifecycle.rs:135-146` |
| Host | flags → settlements | `lifecycle.rs:147-156` |
| Adapter | `Idle` → `Claiming` → `Claimed` | `tickDoorman`, `knock.ts:339-358` |
| Adapter | `Claiming` → `Idle` (no knock, or error with backoff) | `knock.ts:356`, `knock.ts:360-381` |
| Adapter | `Claimed` → `Delivered` | `deliverActiveKnock`, `knock.ts:252-264` |
| Adapter | `Delivered` → `Started` (settle `started`) | `settleObservedTurn`, `knock.ts:307-310`; called at `knock.ts:265` |
| Adapter | busy session: send interrupt | `interruptActiveTurn`, `knock.ts:220-245`; called at `knock.ts:266` |
| Adapter | turn start observed | `noteHallwayKnockTurnStart`, `knock.ts:447-464`; called at `index.ts:812` (`message_start`) |
| Adapter | `Started` → `Ended` | `noteHallwayKnockTurnEnd`, `knock.ts:466-479`; called at `index.ts:1083` (`agent_end`) |
| Adapter | `Ended` → `Idle` (settle `completed`) | `knock.ts:311` |
| Adapter | any active state → `Idle` (settle `failed`) | `failActiveKnock`, `knock.ts:208-218`; called at `knock.ts:230`, `knock.ts:238`, `knock.ts:268`, `knock.ts:304`, `knock.ts:329`, `knock.ts:404`, `knock.ts:489` |
| Adapter | session switch retires the old doorman | `knock.ts:399-409` |
| Adapter | session stop | `stopHallwayKnockDoorman`, `knock.ts:481-491`; called at `index.ts:445` |

The cross-layer links:

- Adapter `Claimed` requires SQL `claimed` and its 30-second lease.
- Adapter `Delivered` makes `KnockPlan` return `started` at once (`lifecycle.rs:152`). SQL `started` therefore means "delivered to OMP".
- Adapter `Ended` makes `KnockPlan` return `completed`, which makes SQL `completed`.
- An adapter failure makes SQL `failed` when the Host answers. When the Host does not answer, the row waits for the sweep.
- The Host refuses claims and settles when `KnockAutonomy` is off (`server.rs:1838-1841`). The adapter also reads the same variable (`knock.ts:385`).

## Rules

| # | Rule | Status | Evidence |
|---|---|---|---|
| L1 | Every non-terminal SQL status leaves within a bounded time. | holds when any room calls `knock`, `claim`, or `settle` | D1 and D2 fixed: `expire_overdue` (`knocks.rs:157-178`). No timer runs the sweep in an idle House. |
| L2 | One current policy exists for each hallway and room. | holds | `0021:30-32`, `knocks.rs:315-329` |
| L3 | Each status has exactly its own column shape. | holds | `0021:74-86` as replaced by `0044:25-41` |
| L4 | A status moves only forward: `pending` → `claimed` → `started` → `completed` or `failed`; `pending` → `expired`. | holds in the only writer | `knock_step` (`hallway_knock.rs`); no SQL trigger enforces it |
| L5 | Only the lease holder settles a knock. | holds | `knocks.rs:779-796` |
| L6 | A claim never takes a `started` knock from its holder. | holds | `knocks.rs:696-699` |
| L7 | One message asks one recipient once. | holds | `0021:64`, `knocks.rs:642-652` |
| L8 | An exchange never exceeds its `max_turns`. | holds | `knocks.rs:481-488`, `0021:67` |
| L9 | The terminal status does not depend on call order. | holds | D3 fixed: `settle` sweeps first and refuses an overdue row (`knocks.rs:767`, `knocks.rs:815-821`). |
| L10 | The adapter holds at most one active knock. | holds | `knock.ts:321-338` |
| L11 | Every adapter state except `Idle` leaves within a bounded time. | holds | D4 fixed in `bfb4364`: a started turn fails at `expiresAt` (`hearth/src/machines/knock_turn.rs`) |
| L12 | SQL `completed` means that the knock turn ran. | **broken in a race** (`unverified`) | See defect D6. |
| L13 | The Hearth turn limits agree with the SQL CHECKs. | holds today | 20 and 10 at `hallway.rs:9-10` and `0031:28-37`; no read source checks the agreement |

## Defects

- **D1. Fixed.** An expired `pending` knock stayed `pending` forever.
  Migration 0044 adds `expired` (ruling R1). The sweep moves an overdue `pending` row to `expired` (`knocks.rs:157-178`).
  The unique key at `0021:64` still refuses a new knock on the same message (`knocks.rs:642-652`).
- **D2. Fixed.** Expiry ran only inside the recipient's claim.
  `knock`, `claim`, and `settle` now run the sweep for every hallway (ruling R4). A sender's next knock closes rows that a silent recipient left open.
- **D3. Fixed.** Settle ignored `expires_at`.
  `settle` runs the sweep, then refuses an overdue row that another transaction held during the sweep (`knocks.rs:815-821`).
  A `started` row past `expires_at` now always becomes `failed`.
- **D4. Fixed in `bfb4364`.** A started turn that never ended had no exit in the adapter.
  The decision moved from `KnockPlan` in `host/src/lifecycle.rs` to `plan_knock_turn` in `crates/hearth/src/machines/knock_turn.rs`.
  The adapter sends `expiresInMs`, the time left until the pointer's `expiresAt` (`knock.ts:283-296`).
  A turn that started and did not end fails when that time reaches zero (ruling R2).
- **D5. A crash loses the active knock.**
  The doorman state lives only in memory (`knockDoormen`, `knock.ts:146`).
  A `claimed` row becomes `Reclaimable` after 30 s, and the next claim injects it again (`knocks.rs:632`).
  A `started` row has no reclaim path. It fails at `expires_at`, 15 to 60 minutes later, and only when D2 permits.
  A clean stop fails the knock at once (`knock.ts:489`).
- **D6. The turn end is not bound to the knock id.**
  `noteHallwayKnockTurnEnd` runs on every `agent_end` (`index.ts:1041-1084`). It checks only `state.active` (`knock.ts:468`).
  For a busy session, the `started` settle at `knock.ts:265` awaits two Host round trips before the interrupt at `knock.ts:266`.
  When the busy turn ends inside that window, `interruptSent` is false.
  Lines `knock.ts:474-478` then treat that end as the knock turn, and the next plan settles `completed`.
  The later `message_start` of the real knock turn returns early at `knock.ts:458`.
  `unverified`: this order comes from code reading. No test observed it.
- **D7. SQL `started` records the delivery, not the turn start.**
  `KnockPlan` returns `started` when `delivered` is true and `turnStarted` is false (`lifecycle.rs:152`).
  `started_at` therefore holds the delivery time.
  The 60-second "did not start" failure (`lifecycle.rs:139-141`) fires on a row that SQL already calls `started`.
- **D8. The adapter types permit contradictory combinations.**
  `startSettled` with `delivered === false`, and `turnEnded` with `active === null`, are representable (`knock.ts:119-140`).
  `clearActiveKnock` (`knock.ts:160-171`) keeps the phase latches consistent. It does not reset `settling`.
- **D9. Fixed in Rust.** The status vocabulary had no single type.
  `HallwayKnockStatus` names all six values. The pointer and the settle receipt use it (`hallway.rs:469`, `hallway.rs:533`).
  The adapter keeps `status: string` (`knock.ts:28`). It never branches on the value.

## Target shape

1. **Bend2 model.** Model the SQL machine, the `KnockPlan` decision, and the adapter machine, then prove L1 to L13.
   The proof must fail on L1, L9, L11, and L12 against today's transitions; this shows that it catches real defects.
2. **SQL and store (both owners).** Done for the store: `knock_step(status, event)` and `HallwayKnockStatus` are in `crates/hearth/src/machines/hallway_knock.rs`.
   `knocks.rs` calls `knock_step` before each UPDATE.
   Open: a SQL `BEFORE UPDATE` trigger on `hallway_knocks` that refuses every edge outside the same set.
   R1 and R4 are in place. This fixed D1, D2, D3, and D9.
3. **Hearth.** `plan_knock_turn` in `crates/hearth/src/machines/knock_turn.rs` is the one decision owner. The Host only calls it.
   The started-turn expiry rule is in place (R2). This fixed D4.
4. **Adapter.** Replace the fields with one tagged state and one pure `step(state, event) → (state, effects)`.
   Bind the turn end to the knock turn that `message_start` observed.
   This fixes D6 and D8.
   Persist the active knock id in the session file (R3). This fixes D5.
5. **Contract test.** CI runs the same random event sequences through the Bend2 model, `knock_step`, and the adapter `step`, and compares the states.
6. **Policy.** No change. The partial unique index and the room-state lock already give one owner.

## Open rulings

- **R1. Terminal state for an expired `pending` row.** Ruled 2026-11-08 and done in migration 0044: the status `expired`. Its claim columns stay null. The status `failed` keeps its meaning: a recipient held the turn and did not finish it.
- **R2. Timeout for a started turn.** Ruled 2026-11-07 and done in `bfb4364`: the pointer's `expiresAt` is the hard limit, and the knock fails at that instant.
- **R3. D5: fix or accept.** Proposal: fix, and persist the active knock id in the session file. After a restart, the doorman fails that knock with a restart reason before it claims new work.
- **R4. Expiry location.** Ruled 2026-11-08 and done: `knock`, `claim`, and `settle` run `expire_overdue` first, for every hallway. `settle` refuses `completed` past `expires_at`. Expiry no longer depends on the recipient's claim loop.
- **R5. D7: name of `started`.** Proposal: keep the SQL value, and define it as "delivered to the recipient harness". The 30-second lease is too short to wait for a busy turn to start. A rename would change the protocol for no new behavior.
