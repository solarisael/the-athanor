# GIGA queue and review

Status: `census` (2026-11-06).

## What it is

GIGA turns captured conversation into review candidates.
Five linked machines carry this work: the queue event, the attempt, the turn identity, the window replacement, and the candidate review.
A queue event is one row in `giga_events`. A worker claims it with a lease, classifies it, and finishes it.
Each claim writes one attempt row. A replay by an operator starts a failed event again.
A turn identity binds one captured turn to the one event that classifies it.
A candidate is the output of a classification. A reviewer moves it through review, and a promotion writes durable memory or a lesson.
Postgres stores every state. Rust in akasha writes every transition, and SQL migrations write some transitions once.

Path convention: a Rust path without a prefix is in `crates/akasha/src/giga/`.
A path with the prefix `hearth/` is in `crates/hearth/src/giga/`.
A path that starts with a four-digit number is in `substrate/migrations/`.

## States

### Queue event (`giga_events.queue_state`)

- SQL: `pending`, `running`, `succeeded`, `failed` (`0003_giga.sql:16`).
- Rust: `GigaQueueState` (`hearth/queue.rs:5-10`). The enum has `parse` and `as_str` only (`hearth/queue.rs:12-34`). It has no transition table.
- Companion fields: `retry_count` (`0003_giga.sql:17`), `locked_at` (`0003_giga.sql:19`), `locked_by` and `lease_expires_at` (`0004_giga_runtime.sql:4-5`), `attempt_count` 0 to 5 (`0004_giga_runtime.sql:6`), `replay_count` (`0004_giga_runtime.sql:9`), `waiting_since` (`0043_giga_turn_queue.sql:5-6`).
- `Waiting` is a sub-state of `pending`: `waiting_since` is set. The claim skips it.

SQL CHECKs on the shape:

- `waiting_since` set requires `pending` (`0043_giga_turn_queue.sql:8-9`).
- `failed` requires `last_error` (`0005_giga_resonance.sql:25-27`).

No CHECK ties `locked_by`, `locked_at`, or `lease_expires_at` to `running`.
No CHECK ties `processed_at` to `succeeded` or `failed`.

### Attempt (`giga_event_attempts.outcome`)

- SQL: `NULL` (open), `succeeded`, `retry`, `failed`, `lease_expired` (`0004_giga_runtime.sql:41`).
- The key is `(event_id, replay_count, attempt_count)` (`0004_giga_runtime.sql:46`).
- `finished_at` set requires an outcome (`0004_giga_runtime.sql:48`).
- `error_class` is NULL exactly for an open or `succeeded` attempt (`0005_giga_resonance.sql:16-20`).
- Rust: `GigaEventFinishOutcome` has `Succeeded`, `Retry`, `Failed` (`hearth/event.rs:184-188`). It has no `lease_expired` value.
- Limit: `GIGA_MAX_EVENT_ATTEMPTS = 5` (`hearth/event.rs:147`).

### Replay ledger (`giga_event_replays`)

- One row for each replay (`0004_giga_runtime.sql:52-63`).
- `previous_state` must be `failed` (`0004_giga_runtime.sql:58`).

### Turn identity (`giga_turn_identities.state`)

- SQL: `active`, `retired`, `conflict`, as a generated column (`0043_giga_turn_queue.sql:26-28`).
- `conflict` is the `conflict` flag. `retired` is `event_id IS NULL`. `active` is `event_id` set.
- The shape CHECK requires empty evidence for a conflict and full evidence otherwise (`0043_giga_turn_queue.sql:32-36`).
- `event_id` references `giga_events` with `ON DELETE SET NULL` (`0043_giga_turn_queue.sql:25`). A purge of the event retires the identity.
- Rust: no enum. `health.rs:61` compares the literal `'conflict'`.

### Window replacement (`giga_window_replacements`)

- A row marks an archived legacy window (`0043_giga_turn_queue.sql:44-48`).
- The migration comment says that the window "is never claimed or replayed" (`0043_giga_turn_queue.sql:42-43`).
- States: `not replaced` (no row) and `replaced` (row present).

### Candidate review (`giga_candidates.review_state`)

- SQL: `unreviewed`, `in_review`, `promoted`, `merged`, `corrected`, `dismissed`, `unresolved`, `curio`, `expired`, `superseded` (`0003_giga.sql:68-71`).
- Rust: `GigaReviewState` (`hearth/candidate.rs:171-182`) with `can_transition` (`hearth/candidate.rs:216-240`).
- Ledger: `giga_reviews` (`0003_giga.sql:109-131`). The CHECKs limit `previous_state` and `new_state` to the ten values (`0003_giga.sql:115-122`). They do not limit the pair.
- One `promote` row for each candidate (`0004_giga_runtime.sql:80-82`).

## Transitions today

| Machine | Move | Writer |
|---|---|---|
| Queue | → `pending` (claimable) | `giga_event_ingest`, `event_ingest.rs:44-49` → `insert_event`, `event_ingest.rs:17-22` |
| Queue | → `pending` + waiting | `conversation_ingest.rs:131` → `insert_event` with `waiting = true` (`event_ingest.rs:21`) |
| Queue | waiting → claimable | `giga_turn_release`, `turn_release.rs:31-40` |
| Queue | `pending` → `running` | `giga_event_claim`, `event_claim.rs:144-159`; selection `event_claim.rs:91-101` |
| Queue | `running` (lease expired) → `running` (new lease) | `event_claim.rs:144-159`; `retry_count + 1` at `event_claim.rs:147` |
| Queue | `running` (lease expired, 5 attempts) → `failed` | `event_claim.rs:49-60`, `event_claim.rs:78-88` |
| Queue | `running` → `failed` (stored event invalid) | `event_claim.rs:189-197` |
| Queue | `running` → `succeeded` / `pending` (retry) / `failed` | `giga_event_finish`, `event_finish.rs:132-142`, `event_finish.rs:164-180` |
| Queue | `running` → `pending` / `failed` (worker deadline) | `giga_event_deadline_settle`, `event_finish.rs:229-273` |
| Queue | `failed` → `pending` (replay) | `giga_event_replay`, `event_replay.rs:102-113` |
| Queue | `pending` / `failed` / expired `running` → deleted | `giga_queue_maintenance` purge, `queue.rs:168-205` |
| Queue | any → `failed` (source role missing) | migration, `0004_giga_runtime.sql:16-24` |
| Queue | → `pending` or `failed` (canonical turn job) | migration, `0043_giga_turn_queue.sql:99-116` |
| Queue | `pending` / `running` → `failed` (window replaced) | migration, `0043_giga_turn_queue.sql:169-175` |
| Attempt | → open | `event_claim.rs:160-174` |
| Attempt | open → `lease_expired` | `event_claim.rs:65-77`, `event_claim.rs:126-139` |
| Attempt | open → `failed` (stored event invalid) | `event_claim.rs:178-188` |
| Attempt | open → `succeeded` / `retry` / `failed` | `event_finish.rs:143-163` |
| Attempt | open → `failed` (window replaced) | migration, `0043_giga_turn_queue.sql:161-167` |
| Replay ledger | → row | `event_replay.rs:89-101` |
| Identity | → `active` (capture) | `conversation_ingest.rs:114-130`, then `event_id` set at `conversation_ingest.rs:136-145` |
| Identity | `active` → `retired` | `ON DELETE SET NULL` (`0043_giga_turn_queue.sql:25`), fired by the purge at `queue.rs:168-205` |
| Identity | → `active` / `retired` / `conflict` (legacy) | migration, `0043_giga_turn_queue.sql:134-151` |
| Replacement | → `replaced` | migration, `0043_giga_turn_queue.sql:155-159` |
| Review | → `unreviewed` | `giga_candidate_store_tx`, `candidate_store.rs:45-56`; also inside finish, `event_finish.rs:206-223` |
| Review | any allowed pair except `promoted`, `merged`, `corrected`, `superseded` | `giga_review`, `review.rs:243-281` |
| Review | `in_review` → `promoted` | `giga_promote`, `promotion.rs:330-358` |

Callers: the worker loop calls release, claim, and deadline settlement (`crates/akasha/src/giga_worker/worker.rs:173`, `:198`, `:112`).
The native command router calls ingest, capture, claim, replay, purge, promote, and review (`crates/akasha/src/native/mod.rs:895-957`).

The cross-machine links:

- A claim opens one attempt in the same transaction as `pending` → `running`.
- A finish closes the open attempt in the same transaction as the queue move.
- A capture writes the identity and its event in one transaction.
- A purge deletes the event, and the foreign key retires the identity.
- A finish can store candidates in the same transaction (`event_finish.rs:206-223`).

## Rules

| # | Rule | Status | Evidence |
|---|---|---|---|
| L1 | A `running` event leaves `running` within a bounded time. | holds while a worker runs in the room | Reclaim `event_claim.rs:95`; exhaustion `event_claim.rs:49-89`; job deadline 120 s and settle deadline 30 s are less than the 300 s lease (`worker.rs:20-25`). Nothing moves the event when no worker runs in its room. |
| L2 | Only the lease owner finishes an event, and only before the lease expires. | holds | `event_finish.rs:20-22` (row lock), `event_finish.rs:94-114` |
| L3 | An event has at most one open attempt. | holds in Rust; no SQL guard | `event_claim.rs:126-139` closes the old attempt before `event_claim.rs:160-174` opens one. `event_finish.rs:159-163` requires exactly one row. |
| L4 | An event has at most 5 attempts for each replay. | holds | `0004_giga_runtime.sql:6`; claim filter `event_claim.rs:93`; retry refusal `event_finish.rs:115-121` |
| L5 | A waiting event is `pending` and the claim skips it. | holds | `0043_giga_turn_queue.sql:8-9`; `event_claim.rs:94` |
| L6 | A replay starts only from `failed` and writes one ledger row. | holds in Rust; no SQL guard | `event_replay.rs:58-62`, `event_replay.rs:89-113` (one transaction). SQL accepts `failed` → `pending` without a ledger row. |
| L7 | A replaced window is never claimed or replayed. | holds in Rust; no SQL guard | `0043_giga_turn_queue.sql:169-175` makes it `failed`; claim takes only `pending` and `running` (`event_claim.rs:93-96`); replay refuses it (`event_replay.rs:63-73`); purge skips it (`queue.rs:180-182`). |
| L8 | A turn identity has at most one classifying event, and `retired` never returns to `active`. | holds | The only Rust writer of `event_id` is `conversation_ingest.rs:136-145`. It runs only after the insert at `conversation_ingest.rs:114-130` creates the row. |
| L9 | A capture refuses a turn whose stored evidence differs. | holds | `conversation_ingest.rs:159-181` |
| L10 | A new candidate is `unreviewed`. | holds | `hearth/candidate.rs:304-309` |
| L11 | Every review move is a pair that `can_transition` allows. | holds | `hearth/review.rs:83-88`; `review.rs:172-178`. The promotion writes only `in_review` → `promoted`, which is an allowed pair (`hearth/promotion.rs:369-374`). |
| L12 | A candidate has at most one promotion. | holds | `0004_giga_runtime.sql:80-82`; row lock `promotion.rs:65-70` |
| L13 | `review_state` equals the `new_state` of the latest ledger row. | holds in Rust; no SQL guard | Ledger and state change in one transaction: `review.rs:243-281`, `promotion.rs:330-358`. See defect D5. |
| L14 | Every review state has a writer. | **broken** | See defect D4. |
| L15 | A conflict identity has an exit. | **broken** | See defect D8. |

## Defects

- **D1. SQL permits illegal queue moves and shapes.**
  No trigger guards a move on `giga_events.queue_state` (`0003_giga.sql:16`).
  A `running` row with a NULL lease, a `pending` row with a lease, and a `succeeded` row with a NULL `processed_at` are representable.
  The migration at `0004_giga_runtime.sql:16-24` set `failed` with no `queue_state` filter.
  That statement could move `succeeded` or `running` rows to `failed`, and it left `locked_at` set.
  The Rust writers keep the shapes consistent today.
- **D2. The purge deletes claimable `pending` work and retires its turns.**
  The purge deletes every `pending` event with `waiting_since IS NULL` that has no candidate (`queue.rs:168-187`).
  This set includes new work that no worker has claimed yet.
  The foreign key then retires the turn identity (`0043_giga_turn_queue.sql:25`).
  A later capture of the same turn counts as a duplicate and writes no event (`conversation_ingest.rs:182-183`).
  The turn is then never classified.
- **D3. Two writers own `review_state`.**
  `giga_review` writes it with a compare-and-set (`review.rs:267-281`).
  `giga_promote` writes it with a hard-coded `in_review` → `promoted` (`promotion.rs:350-358`).
  The promotion does not call `can_transition` and does not check `rows_affected`.
  The row lock at `promotion.rs:65-70` and the check at `hearth/promotion.rs:369-374` keep it correct today.
- **D4. Four review states have no path in.**
  `giga_review` refuses `promoted`, `merged`, `corrected`, and `superseded` (`review.rs:179-189`).
  `giga_promote` writes only `promoted`.
  No writer makes `merged`, `corrected`, or `superseded`, and `can_transition` permits all three (`hearth/candidate.rs:222-238`).
  No writer reads `expires_at` (`0003_giga.sql:80`) to move a candidate to `expired`. Only a manual review makes `expired`.
  No writer sets `successor_candidate_id` (`0003_giga.sql:81`).
- **D5. The review ledger is mutable, and SQL does not bind its pairs.**
  `giga_reviews` has no append-only trigger. A grep of `substrate/migrations/` finds `giga_reviews` only in 0003, 0004, and 0005.
  The CHECKs accept any pair of states and any `action` with any `new_state` (`0003_giga.sql:112-122`).
  An outside write can make `review_state` disagree with the ledger.
- **D6. A second ingest door bypasses turn identities.**
  `giga_event_ingest` (`event_ingest.rs:44-49`) accepts a `conversation_window` event with turn sources (`hearth/event.rs:52`).
  It writes no `giga_turn_identities` row.
  If that event id equals a canonical turn id, a later capture of the turn refuses the whole batch (`conversation_ingest.rs:131-135`).
  The native router exposes this door (`crates/akasha/src/native/mod.rs:895-896`).
  Whether any current caller sends `conversation_window` through it: `unverified`.
- **D7. The Rust types do not cover the stored values.**
  `lease_expired` (`0004_giga_runtime.sql:41`, written at `event_claim.rs:67` and `event_claim.rs:129`) has no `GigaEventFinishOutcome` value (`hearth/event.rs:184-188`).
  `GigaQueueState` has no transition table (`hearth/queue.rs:12-34`). akasha writes queue states as string literals.
  The turn identity state has no Rust type (`health.rs:61`).
- **D8. A conflict identity has no exit.**
  Only the migration writes `conflict = TRUE` (`0043_giga_turn_queue.sql:134-151`).
  No Rust writer changes a conflict row. The only Rust writers are `conversation_ingest.rs:115` and `conversation_ingest.rs:137`.
  A capture of a conflict turn always fails (`conversation_ingest.rs:159-163`).
  The replay refusal tells the operator to repair the evidence (`event_replay.rs:71`). No tool does this repair.

## Target shape

1. **Bend2 model.** Model the queue event, the attempt, the lease clock, the turn identity, and the candidate review. Prove L1 to L15.
   The proof must fail on L14 and L15 against today's transitions; this shows that it catches real defects.
2. **Rust step functions in hearth.** Add `GigaQueueState::step(state, event) → (state, effects)` with the events `Claim`, `LeaseExpired`, `Finish(outcome)`, `DeadlineSettle`, `Replay`, `Release`, and `Purge`.
   Add `LeaseExpired` to the attempt outcome type. This fixes D7.
   Make `giga_promote` call `can_transition` and check `rows_affected`. This fixes D3.
3. **SQL triggers.** Add a `BEFORE UPDATE` trigger on `giga_events` and on `giga_candidates` that refuses a move outside the transition table.
   Add CHECKs that tie the lease fields to `running` and `processed_at` to the terminal states. This fixes D1.
   Make `giga_reviews` append-only, and refuse an update of `review_state` without a matching ledger row. This fixes D5.
   The owner is both layers: Rust decides each move, and SQL refuses an illegal move from any writer, including migrations.
4. **Contract test.** CI runs the same random event sequences through the Bend2 model and through the step functions, and compares the states.
5. **Ingest.** Refuse `conversation_window` in `giga_event_ingest`. Turn capture uses only `giga_conversation_ingest`. This fixes D6.
6. **Rulings.** D2, D4, and D8 need the rulings below before a change.

## Open rulings

- **R1. Purge scope (D2).** Proposal: purge only `failed` events and expired `running` events. Keep `pending` events. A `pending` event is work in the queue; a purge of it loses a turn for ever.
- **R2. Review states with no writer (D4).** Proposal: remove the edges into `merged`, `corrected`, and `superseded` from `can_transition` until a writer with authority exists. Keep the values in the SQL CHECK. The model then proves only the moves that the code can make.
- **R3. Candidate expiry (D4).** Proposal: keep `expired` as a manual review result, and record `expires_at` as advisory. Add a worker sweep only when Sol wants automatic expiry.
- **R4. Conflict repair (D8).** Proposal: add one operator tool that replaces the conflict row with chosen evidence. The tool writes a ledger row with the operator identity and the authorization basis, as the replay does.
- **R5. Second ingest door (D6).** Proposal: refuse `conversation_window` in `giga_event_ingest`. The capture path is then the only writer of turn events.
