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

### SQL (`substrate/migrations/0021_hallway_knock.sql`)

`hallway_knocks.status` holds five values (`0021:51-52`):

- `pending`: no claim. All claim columns are null (`0021:81-82`).
- `claimed`: a presence holds a 30-second lease (`knocks.rs:41-45`, `knocks.rs:56`).
- `started`: the recipient turn is open. `started_at` is set (`0021:76`).
- `completed`: terminal. `started_at` and `settled_at` are set (`0021:77`).
- `failed`: terminal. `settled_at` is set (`0021:78`).

Shape CHECKs bind each status to its columns: `0021:74-86`.
Migration 0031 moves the turn ceiling to 20 and the default to 10 (`0031_hallway_knock_turns.sql:25-37`).
Two more states are derived and have no value:

- `Reclaimable`: `claimed` with `lease_expires_at <= NOW()` (`knocks.rs:632`).
- `Expired`: any non-terminal row with `expires_at <= NOW()`.
  The lifetime is 3 minutes for each turn, with a minimum of 15 minutes (`knocks.rs:26-36`).

`hallway_knock_policies` has two states: current (`superseded_at` null) and superseded (`0021:22`).
A partial unique index permits one current row for each hallway and room (`0021:30-32`).
No current row means "deny" (`knocks.rs:336-341`).

### Store and Host (`crates/origami/src/hallways/knocks.rs`, `crates/host/src/lifecycle.rs`)

The store writes the row and keeps no state.
`HallwayKnockOutcome` names three of the five statuses: `Started`, `Completed`, `Failed` (`hallway.rs:498-502`).
The pointer carries `status` as a free string (`hallway.rs:469`).
`pending` and `claimed` exist only as SQL literals in `knocks.rs`.

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
| SQL | policy → current, old current → superseded | `knocks::policy`, `knocks.rs:247-271`; serialized by the room-state upsert, `knocks.rs:239-246` |
| SQL | → `pending` | `knocks::knock`, `knocks.rs:544-575`; admission `knocks.rs:533`, chain `knocks.rs:535-543` |
| SQL | `pending` → `claimed` | `knocks::claim`, `knocks.rs:618-679` |
| SQL | `Reclaimable` → `claimed` (new holder) | same query, `knocks.rs:632` |
| SQL | `claimed`/`started` past `expires_at` → `failed` | sweep inside `claim`, `knocks.rs:606-617` |
| SQL | `claimed` (lease active) → `started` | `knocks::settle`, `knocks.rs:745-755` |
| SQL | `started` → `completed` | `knocks.rs:756-766` |
| SQL | `claimed` (lease active)/`started` → `failed` | `knocks.rs:767-777` |
| SQL | same outcome and reason again → no move | `knocks.rs:732-740` |
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
| L1 | Every non-terminal SQL status leaves within a bounded time. | **broken** | See defects D1 and D2. |
| L2 | One current policy exists for each hallway and room. | holds | `0021:30-32`, `knocks.rs:239-253` |
| L3 | Each status has exactly its own column shape. | holds | `0021:74-86` |
| L4 | A status moves only forward: `pending` → `claimed` → `started` → `completed` or `failed`. | holds in the only writer | guards `knocks.rs:743-777`, `knocks.rs:606-613`; no SQL trigger enforces it |
| L5 | Only the lease holder settles a knock. | holds | `knocks.rs:706-723` |
| L6 | A claim never takes a `started` knock from its holder. | holds | `knocks.rs:630-633` |
| L7 | One message asks one recipient once. | holds | `0021:64`, `knocks.rs:565-575` |
| L8 | An exchange never exceeds its `max_turns`. | holds | `knocks.rs:405-412`, `0021:67` |
| L9 | The terminal status does not depend on call order. | **broken** | See defect D3. |
| L10 | The adapter holds at most one active knock. | holds | `knock.ts:321-338` |
| L11 | Every adapter state except `Idle` leaves within a bounded time. | **broken** | See defect D4. |
| L12 | SQL `completed` means that the knock turn ran. | **broken in a race** (`unverified`) | See defect D6. |
| L13 | The Hearth turn limits agree with the SQL CHECKs. | holds today | 20 and 10 at `hallway.rs:9-10` and `0031:28-37`; no read source checks the agreement |

## Defects

- **D1. An expired `pending` knock stays `pending` forever.**
  The sweep at `knocks.rs:606-617` moves only `claimed` and `started`.
  The claim query skips expired rows (`knocks.rs:629`).
  The shape CHECK forbids `failed` with null claim columns (`0021:80-86`), so no writer can fail a `pending` row today.
  The unique key at `0021:64` then refuses a new knock on the same message (`knocks.rs:565-575`).
  A revoked policy also holds its `pending` rows, because the claim joins the current policy (`knocks.rs:622-628`).
- **D2. Expiry runs only inside the recipient's claim.**
  The sweep is in `claim` only (`knocks.rs:606`).
  The doorman does not claim while it holds an active knock (`knock.ts:321-338`).
  A room with `KnockAutonomy` off never claims (`server.rs:1839`, `knock.ts:385`).
  In both cases, an expired `claimed` or `started` row stays non-terminal.
- **D3. Settle ignores `expires_at`.**
  `settle` checks the lease and the status only (`knocks.rs:742-777`).
  A `started` row past `expires_at` becomes `completed` when the settle arrives first.
  The same row becomes `failed` when a claim sweep arrives first.
- **D4. A started turn that never ends has no exit in the adapter.**
  `KnockPlan` fails a knock only on the three timers at `lifecycle.rs:135-146`.
  No rule covers `turnStarted && !turnEnded`.
  The doorman stays active, and the room claims no new knock (D2).
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
- **D9. The status vocabulary has no single type.**
  `HallwayKnockOutcome` names three statuses (`hallway.rs:498-502`). The pointer uses a string (`hallway.rs:469`, `knock.ts:28`).
  The doc comment at `hallway.rs:449` says that omitted roots use four turns. The default is 10 (`hallway.rs:10`, `knocks.rs:469`).

## Target shape

1. **Bend2 model.** Model the SQL machine, the `KnockPlan` decision, and the adapter machine, then prove L1 to L13.
   The proof must fail on L1, L9, L11, and L12 against today's transitions; this shows that it catches real defects.
2. **SQL and store (both owners).** Add one Rust step function `knock_step(status, event) → status` in Hearth, with one `HallwayKnockStatus` enum for all five values.
   `knocks.rs` calls `knock_step` before each UPDATE.
   A SQL `BEFORE UPDATE` trigger on `hallway_knocks` refuses every edge outside the same set.
   Add a terminal state for expired `pending` rows (R1). Expire rows in `knock`, `claim`, and `settle` (R4).
   This fixes D1, D2, D3, and D9.
3. **Host.** Keep `KnockPlan` as the one decision owner. Add a rule for `turnStarted && !turnEnded` (R2).
   This fixes D4.
4. **Adapter.** Replace the fields with one tagged state and one pure `step(state, event) → (state, effects)`.
   Bind the turn end to the knock turn that `message_start` observed.
   This fixes D6 and D8.
   Persist the active knock id in the session file (R3). This fixes D5.
5. **Contract test.** CI runs the same random event sequences through the Bend2 model, `knock_step`, and the adapter `step`, and compares the states.
6. **Policy.** No change. The partial unique index and the room-state lock already give one owner.

## Open rulings

- **R1. Terminal state for an expired `pending` row.** Proposal: add the status `expired`. The shape CHECK then permits null claim columns for `expired`. The status `failed` keeps its meaning: a recipient held the turn and did not finish it.
- **R2. Timeout for a started turn.** Proposal: use the pointer's `expiresAt` as the hard limit, and fail the knock at that instant. The exchange lifetime already scales with the turn budget (`knocks.rs:26-36`).
- **R3. D5: fix or accept.** Proposal: fix, and persist the active knock id in the session file. After a restart, the doorman fails that knock with a restart reason before it claims new work.
- **R4. Expiry location.** Proposal: run the expiry UPDATE for the hallway inside `knock`, `claim`, and `settle`. `settle` then refuses `completed` past `expires_at`. Expiry then no longer depends on the recipient's claim loop.
- **R5. D7: name of `started`.** Proposal: keep the SQL value, and define it as "delivered to the recipient harness". The 30-second lease is too short to wait for a busy turn to start. A rename would change the protocol for no new behavior.
