# Docket lifecycle

Status: `census` (2026-11-06).

## What it is

The Docket is the House plane for delegated work.
A goal holds intent. A quest is one unit of work under a goal or alone.
A room claims a quest, works under a lease, and submits evidence.
A different room settles each acceptance item, and the last accepted item settles the quest.
Four columns in PostgreSQL hold the state, and `quest_events` records each move.
All writers live in `crates/akasha/src/docket`. No other crate or adapter writes these tables.

## States

### SQL (`substrate/migrations/0023_docket.sql`, `0025_docket_draft_abandon.sql`)

Each state column is free text with a value CHECK. No trigger guards a move.

- `goals.status`: `draft`, `offered`, `active`, `blocked`, `retired`, `cancelled`. Default `draft` (`0023:188`), CHECK `0023:196-197`.
- `quests.state`: `draft`, `offered`, `claimed`, `submitted`, `settled`, `refused`, `blocked`, `quarantined`, `cancelled`. Default `draft` (`0023:246`), CHECK `0023:256-260`.
- `quest_attempts.state`: `active`, `yielded`, `finished`, `failed`, `abandoned`, `reclaimed`. Default `active` (`0023:324`), CHECK `0023:333-334`.
- `quest_acceptance_items.verdict`: `pending`, `met`, `not_met`, `unknown`, `inconclusive`, `not_applicable`, `refused`. Default `pending` (`0023:357`), CHECK `0023:365-369`.

Side fields carry more state:

- `quests.claim_epoch` (`0023:248`) counts claims. `quest_attempts.claim_epoch` (`0023:315`) names the claim of one attempt.
- `quests.cancel_requested` and `cancel_requested_by` (`0023:249-250`) have no writer.
- `quests.settled_at` (`0023:253`) and `goals.retired_at` (`0023:192`) are not tied to a state.
- `quest_attempts.lease_expires_at` and `heartbeat_at` (`0023:322`, `0023:326`) hold the lease.
- `quest_events.event_kind` (`0023:399`) and `quest_receipts.kind` (`0023:442`) are free text with no CHECK.

### Rust (`crates/akasha/src/docket`)

Rust has no enum for any of the four columns.

- `QUEST_STATES` (`board.rs:7-17`) copies the quest CHECK list as strings. Only the board filter reads it.
- `ITEM_VERDICTS` (`report/params.rs:5-12`) copies the six settled verdicts.
- `SETTLED_VERDICTS` (`report/settle.rs:9`) names the two accepted verdicts: `met` and `not_applicable`.
- Each writer uses an inline SQL literal for the target state.

The live states are the states that a writer can reach:

- Goal: `draft`, `active`.
- Quest: `draft`, `offered`, `claimed`, `submitted`, `settled`.
- Attempt: `active`, `yielded`, `reclaimed`.
- Item: `pending` and all six verdicts.

## Transitions today

| Machine | Move | Writer |
|---|---|---|
| Goal | → `draft` (replay returns the same row) | `post/goal.rs:16-17` |
| Goal | `draft`/`offered` → `active` | guard `post/goal.rs:51-59`, write `post/goal.rs:60-61` |
| Quest | → `draft` | `post/quest.rs:22-23` |
| Quest | `draft` → `offered`, and insert one item for each criterion | guard `post/quest.rs:60-70`, write `post/quest.rs:89-90`, items `post/quest.rs:104-115` |
| Quest | `offered` → `claimed`, and insert attempt `active` | guard `claim.rs:55-56`, `claim.rs:102-106`; attempt `claim.rs:116-117`; quest `claim.rs:132-133` |
| Quest | `claimed` → `claimed` (new epoch), and attempt `active` → `reclaimed` | `claim.rs:87-101`, then `claim.rs:116-133` |
| Attempt | `active` → `active` (lease renew) | `report/mod.rs:110-111` |
| Quest | `claimed` → `submitted`, and attempt `active` → `yielded` | guard `report/mod.rs:83-103`, `report/mod.rs:121-127`; write `report/submit.rs:21-28` |
| Item | `pending` → any verdict | guard `report/mod.rs:130-154`, `report/settle.rs:21-34`; write `report/settle.rs:36-37` |
| Quest | `submitted` → `settled` (all items accepted) | `report/settle.rs:72-74` |
| Quest | → `offered` (recurrence re-arm, new row) | `report/settle.rs:114-115`, items `report/settle.rs:123-124` |

Each writer appends one `quest_events` row in the same transaction: `ledger.rs:5-26` and `ledger.rs:32-51`.
The append-only trigger refuses UPDATE and DELETE on that table (`0023:421-432`).

The cross-machine links:

- A claim is legal only when the quest is `offered`, or `claimed` with an expired `active` attempt.
- Progress and submit need an `active` attempt whose epoch equals the quest epoch, a live lease, the matching token, and the claimant room (`report/mod.rs:66-103`).
- A settle needs a `yielded` attempt of the current epoch, a non-executor role, a room that is not the claimant room, and a `submitted` quest (`report/mod.rs:66-69`, `report/mod.rs:130-154`).

## Rules

| # | Rule | Status | Evidence |
|---|---|---|---|
| L1 | A quest moves only along `draft` → `offered` → `claimed` → `submitted` → `settled`. | holds in Rust; **broken in SQL** | Rust guards: `post/quest.rs:68`, `claim.rs:87-106`, `report/mod.rs:122`, `report/settle.rs:72`. SQL has no trigger. See defect D6. |
| L2 | Activation freezes the authority tuple of a goal and a quest. | holds | `0023:199-208`; `0023:267-277`, replaced by `0025:14-24` |
| L3 | A quest has at most one `active` attempt. | holds through the writers | `claim.rs:56` locks the quest; `claim.rs:87-101` retires the old attempt first. SQL permits two (`0023:329-330`). |
| L4 | A stale hand cannot publish progress or a submission. | holds | `report/mod.rs:83-88`, `report/mod.rs:94-103` |
| L5 | An executor never settles an acceptance item. | holds | `report/mod.rs:131-147`; role CHECK `0023:370-371` |
| L6 | Every state except `settled` has an exit. | **broken** | See defects D1 and D2. |
| L7 | Every declared state has a writer. | **broken** | See defect D3. |
| L8 | Every quest row has a ledger history from its first state. | **broken for re-armed quests** | See defect D4. |
| L9 | A replayed post returns the existing row. | **broken for quest `draft`** | See defect D5. |
| L10 | Recurrence re-arms only under a live goal. | **broken** | See defect D7. |
| L11 | A settled verdict carries its settler. | holds | `0023:374-383`; `report/settle.rs:36-37` |

## Defects

- **D1. A submitted quest with one rejected item has no exit.**
  `report/settle.rs:72` settles only when every item is `met` or `not_applicable`.
  Any other verdict leaves the quest `submitted` (`report/settle.rs:92-94`).
  `report/settle.rs:30-34` refuses a second verdict on the same item.
  No writer moves the quest to `refused`, `blocked`, or `offered`.
  The 0023 header promises a steward replanning door for REFUSED (`0023:18-19`). No code implements it.
- **D2. An expired lease has no active exit.**
  An attempt stays `active` and the quest stays `claimed` after `lease_expires_at`.
  Only a new claim moves it, through the lazy reclaim at `claim.rs:87-101`.
  No sweeper exists. The clock still lists the quest as `claimed` (`clock.rs:110`).
- **D3. Eleven declared states have no writer.**
  Quest: `refused`, `blocked`, `quarantined`, `cancelled`. The only Rust literals are `board.rs:13-16`.
  Attempt: `finished`, `failed`, `abandoned`.
  Goal: `offered`, `blocked`, `retired`, `cancelled`. So the `offered` arm of the guard at `post/goal.rs:57` cannot match.
  `0025:1-6` opens `draft` → `cancelled` for a poster. No writer uses it.
  `cancel_requested` (`0023:249`) also has no writer.
- **D4. A re-armed quest starts with an empty ledger.**
  `report/settle.rs:114-115` inserts the new quest directly as `offered`.
  The `rearmed` event goes to the old quest id (`report/settle.rs:130-137`).
  The new quest has no `drafted` or `activated` row.
- **D5. A replayed quest draft makes a twin.**
  `docket.quests` has no idempotency column (`0023:227-254`).
  `post/quest.rs:22-23` inserts a new row on each call.
  The event key index is unique per quest (`0023:409-411`), so the replay key does not conflict.
  `crates/akasha/FEATURES.md:135` says that a replayed draft returns the existing row. That holds only for a goal (`post/goal.rs:16-17`).
- **D6. SQL accepts any move and contradictory rows.**
  Any UPDATE that passes the value CHECKs succeeds. `settled` → `offered` and `yielded` → `active` are legal.
  `claim_epoch` can decrease, because `0023:283-284` checks only `>= 0`.
  `settled_at` can be set on a non-settled quest, and a `settled` quest can have a NULL `settled_at`.
  A `pending` item can carry `settled_by_*`, because `0023:374-383` checks one direction only.
  Two `active` attempts on one quest are representable (`0023:329-330`, `0023:343-346`).
- **D7. Re-arm ignores the goal status.**
  The re-arm query joins `docket.goals` and tests only `recurrence_interval IS NOT NULL` (`report/settle.rs:115`).
  A `draft` goal can therefore re-arm a quest. A `retired` or `cancelled` goal can too, once a writer for those states exists.
- **D8. Replays disagree by action.**
  A replayed claim returns the first result (`claim.rs:63-80`).
  A replayed submit fails with `stale_lease`, because the attempt is now `yielded` (`report/mod.rs:68`, `report/mod.rs:83-88`).
  A replayed settle fails with "already settled" (`report/settle.rs:30-34`).
  A replayed activate fails with "not activatable" (`post/quest.rs:68-70`).
- **D9. The Rust lists can drift from the CHECKs.**
  `QUEST_STATES` (`board.rs:7-17`) and `ITEM_VERDICTS` (`report/params.rs:5-12`) copy the SQL lists by hand.
  Goal status and attempt state have no Rust list.

## Target shape

1. **Bend2 model.** Model the goal, quest, attempt, and item machines and the cross-machine links. Then prove L1 to L11.
   The proof must fail on L6, L7, L8, L9, and L10 against today's transitions; this shows that it catches real defects.
2. **Rust step function.** Add one `step(state, event) → (state, effects)` in `crates/akasha/src/docket` for each machine.
   Each writer calls it and writes only its result. Replace the inline literals with enums that serialize to the CHECK values.
   This fixes D9.
3. **SQL trigger.** Add one BEFORE UPDATE trigger on each state column that refuses a move outside the transition table.
   Add the missing combination CHECKs: `settled_at` with `settled`, and `pending` with NULL `settled_by_*`.
   Add a partial unique index on `quest_attempts (quest_id) WHERE state = 'active'`.
   This fixes D6. The trigger is the second owner of legality, and Rust is the only writer.
4. **Exits.** Add the missing doors after rulings R1 to R3. This fixes D1, D2, and D3.
5. **Ledger.** Write `drafted` and `activated` rows for a re-armed quest, under its own id. This fixes D4.
6. **Replay.** Give quests a mint key like goals, and return the first result for every replayed action. This fixes D5 and D8.
7. **Contract test.** CI runs the same random event sequences through the Bend2 model and through `step`, and compares the states.

## Open rulings

- **R1. Exit from a rejected submission (D1).** Proposal: a `not_met`, `refused`, `unknown`, or `inconclusive` verdict moves the quest to `refused` and the attempt to `failed`. The steward then re-offers the quest or cancels it. This matches the replanning door in `0023:18-19`.
- **R2. Expired leases (D2).** Proposal: keep the lazy reclaim, and add a clock sweep that moves an expired `active` attempt to `abandoned`. The quest returns to `offered` in the same transaction. The board then shows the truth.
- **R3. Unwritten states (D3).** Proposal: write `cancelled` for quest and goal now, because `0025` already asks for it. Remove `blocked`, `quarantined`, and goal `offered` from the CHECKs until a ruling names a writer for each.
- **R4. Re-arm under a goal (D7).** Proposal: re-arm only when the goal is `active`.
- **R5. Dependencies.** `quest_dependencies` (`0023:295`) has no reader in `crates/`. Its acyclic rule (`0023:293`) has only a self-loop CHECK (`0023:302-303`). Proposal: a claim refuses a quest while a dependency is not `settled`. A trigger refuses an edge that closes a cycle.
- **R6. Who may settle as `operator`.** The role CHECK allows `operator` (`0023:370-371`). The report params allow `executor`, `reviewer`, and `steward` (`report/params.rs:81`). Proposal: add `operator` to the params, or remove it from the CHECK. Pick one list.
