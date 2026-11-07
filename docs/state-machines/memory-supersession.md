# Memory, canon, and design-document supersession

Status: `census` (2026-11-06).

## What it is

A supersession replaces one stored record with a newer record.
The old row keeps its body and points to its successor.
Three tables use this pattern: `memories`, `named_entities` (canon), and `design_documents`.
Each table has its own columns, its own writer, and its own schema rules.
No shared contract connects the three.
This document also covers `presence_sessions.closed_at`, because it is a small row lifecycle in the same store.

The writers are in the `akasha` crate:

- Memory: `write_memory_tx`, `crates/akasha/src/remember/memory_write.rs:80-279`.
- Canon: `canon_write`, `crates/akasha/src/canon.rs:112-231`.
- Design document: `design_document_write`, `crates/akasha/src/lesson/design/write.rs:58-158`.
- Presence: `crates/akasha/src/presence.rs:83-162`, called by the Host.

## States

### Memory (`substrate/migrations/0001_initial.sql:19-38`)

Two nullable columns hold the state: `superseded_by` (`0001:33`) and `archived_at` (`0001:34`).
No column names the state.
The effective states are `Current`, `Superseded`, and `Archived`.
`Superseded` and `Archived` are independent flags, so a row can be both.
The FK on `superseded_by` has no `ON DELETE` action (`0001:33`).

### Canon (`substrate/migrations/0015_canon_authority.sql:3-21`)

`authority` holds the state: `active`, `superseded`, or `archived` (`0015:4`, CHECK `0015:11-12`).
The shape CHECK (`0015:13-18`) ties `superseded_by` to the state:

- `active` requires `superseded_by` NULL.
- `superseded` requires `superseded_by` set.
- `archived` accepts any `superseded_by`.

The successor also lists its predecessors in `supersedes[]` (`0015:6`).
The FK on `superseded_by` is `DEFERRABLE INITIALLY DEFERRED` (`0015:19-21`).
The Rust mirror is `CanonAuthority`, `crates/hearth/src/canon.rs:5-9`.

### Design document (`substrate/migrations/0012_design_documents.sql:3-19`)

`superseded_by` holds the state (`0012:13`).
The effective states are `Current` (NULL), `Retiring` (equal to its own `id`), and `Superseded` (another `id`).
`Retiring` exists only inside the writer transaction.
The FK uses `ON DELETE SET NULL` (`0012:13`).

### Presence session (`substrate/migrations/0030_presence_sessions.sql:55-68`)

`closed_at` holds the state: `Live` (NULL) or `Closed` (`0030:63`).
A CHECK requires `closed_at >= opened_at` (`0030:67`).

## Transitions today

| Machine | Move | Writer |
|---|---|---|
| Memory | → `Current` (new row) | `memory_write.rs:153-161` (upsert), `memory_write.rs:111-122` (paper boat) |
| Memory | `Current`/`Superseded` → body rewrite, state unchanged | upsert `memory_write.rs:158-160` |
| Memory | any → `Superseded` (point to the new row) | `memory_write.rs:209-229`, UPDATE `memory_write.rs:215-216` |
| Memory | → `Archived` | no production writer (test only: `crates/akasha/src/giga_worker/context_tests.rs:707`) |
| Canon | → `active` (new row) | INSERT `canon.rs:202-209`, row shape `canon.rs:105-106` |
| Canon | `active` → `superseded` | `canon.rs:179-192`, UPDATE `canon.rs:181`; guard `canon.rs:58` |
| Canon | → `archived` | no production writer (test only: `crates/akasha/tests/canon_integration.rs:258`) |
| Design | `Current` → `Retiring` | `write.rs:115-127`, guard `write.rs:93` |
| Design | → `Current` (new row) | INSERT `write.rs:129-131` |
| Design | `Retiring` → `Superseded` | `write.rs:132-147` |
| Design | `Superseded` → `Current` (successor deleted) | schema `ON DELETE SET NULL`, `0012:13`; no production deleter found |
| Presence | → `Live` (new row, or reopen of a `Closed` row) | `presence.rs:90-104` |
| Presence | `Live` → `Live` (ledger write) | `presence.rs:125-128`; refuses a `Closed` row at `presence.rs:135-140` |
| Presence | `Live` → `Closed` | `presence.rs:152-155` |

The callers:

- Memory: `write_memory_tx` gets `supersedes` from the remember command (`memory_write.rs:87`).
- Canon: one transaction locks the name (`canon.rs:140-146`), locks the predecessors (`canon.rs:147`), supersedes them, and inserts the successor.
- Presence: the Host opens through `persist_presence` (`crates/host/src/server.rs:1329-1357`).
  It writes the ledger first, and on refusal it calls `presence_session_open` (`server.rs:1342-1347`).
  The open, compile, and settle commands call `persist_presence` (`server.rs:1417`, `server.rs:1518`, `server.rs:1550`).
- Presence: the Host closes at `server.rs:1598` and at `crates/host/src/server/context_session/identity.rs:210`.

The readers treat a memory as current only when both flags are NULL (`0015:65-66`).
`resolve_memory_references` reports both flags as "historical" reasons (`crates/akasha/src/recall/memory_reference.rs:190-196`).

## Rules

| # | Rule | Status | Evidence |
|---|---|---|---|
| L1 | Only a current record becomes superseded. | **broken for memory** | Canon `canon.rs:58`, `canon.rs:181`; design `write.rs:93`, `write.rs:116`; memory: see defect D1. |
| L2 | The supersession graph has no cycle. | **broken for memory** | See defect D1. Canon and design always point to a new row (`canon.rs:174-177`, `write.rs:129-134`). |
| L3 | A record does not supersede itself. | holds | Memory `memory_write.rs:211`; canon successor ID is new (`canon.rs:174-177`); design self-pointer clears at `write.rs:134`. No schema CHECK. |
| L4 | A superseded record stays superseded. | **broken in schema for design** | No production writer clears it. `ON DELETE SET NULL` (`0012:13`) clears it. See defect D4. |
| L5 | A predecessor and its successor are in the same room. | holds | Memory `memory_write.rs:215-216`; canon `canon.rs:41`, `canon.rs:47-51`. Design rows have no room. |
| L6 | Canon has at most one active entity for each room and name. | holds in the writer | Index `0015:26-28` is case-sensitive. The writer checks `lower(name)` at `canon.rs:141`, `canon.rs:149`. See defect D5. |
| L7 | Design has at most one current document for each `(system, doc_type, name)`. | holds | `0012:24-26` |
| L8 | Canon `supersedes[]` on the successor matches `superseded_by` on the predecessors. | holds in the writer | `canon.rs:106`, `canon.rs:181`. No schema link. |
| L9 | Every state has a writer. | **broken** | Memory `Archived` and canon `archived` have no production writer. See defect D3. |
| L10 | A closed presence row reopens only through an open command. | **broken across processes** (`unverified`) | See defect D6. |
| L11 | A ledger write never changes a closed presence row. | holds | `presence.rs:128`, `presence.rs:135-140` |

## Defects

- **D1. Memory supersession repoints a superseded row and permits a cycle.**
  The UPDATE at `memory_write.rs:215-216` has no `superseded_by IS NULL` predicate.
  A memory that is already superseded gets a new successor.
  The successor can also be a superseded row, because the upsert at `memory_write.rs:158-160` returns an existing row in any state.
  The cycle sequence is: B supersedes A, then a rewrite of A with `supersedes = [B]`.
  The result is A → B and B → A.
  Only the self-skip at `memory_write.rs:211` exists.
- **D2. A memory rewrite keeps the old state.**
  The upsert SET list at `memory_write.rs:159-160` omits `superseded_by` and `archived_at`.
  A rewrite of a superseded memory changes its body, and recall still hides it (`0015:65-66`).
  The writer gets no warning.
- **D3. Two states have no production writer.**
  Memory `archived_at` has one writer, in test code (`context_tests.rs:707`).
  That test sets `archived_at` and `superseded_by` together, so the two flags combine.
  Canon `archived` has one writer, in test code (`canon_integration.rs:258`).
  The canon CHECK accepts `archived` with any `superseded_by` (`0015:16`).
- **D4. A design-document delete brings back its predecessor.**
  `ON DELETE SET NULL` (`0012:13`) makes the predecessor `Current` again.
  When another current row has the same identity, the unique index `0012:24-26` refuses the delete.
  The memory FK (`0001:33`) refuses the same delete instead.
  The three tables use different delete policies.
- **D5. The canon schema accepts two active names that differ only in case.**
  `named_entities_active_room_name_key` (`0015:26-28`) compares `name` exactly.
  Only `canon_write` compares case-insensitively (`canon.rs:141`, `canon.rs:149`).
  A direct SQL write can create `Kodo` and `kodo` as two active entities.
- **D6. A ledger write reopens a closed presence row.**
  `persist_presence` calls `presence_session_open` when the ledger write refuses (`server.rs:1342-1347`).
  The compile and settle commands use this path (`server.rs:1518`, `server.rs:1550`).
  In one process, a close also removes the in-memory session (`crates/host/src/presence.rs:396`).
  Then `persist_presence` returns at `server.rs:1339-1341` and writes nothing.
  The reopen therefore needs a live in-memory session in a second process.
  The in-memory session lives in `PresenceRuntime.sessions`, `crates/host/src/presence.rs:47-48`.
  Whether two Host processes can serve one session is `unverified`.

## Target shape

1. **Owner: SQL trigger.** The states live only in rows, and test code and manual SQL write the rows directly.
   One `BEFORE UPDATE` trigger on each table owns the supersession move.
   The trigger refuses a move when the predecessor is not current.
   The trigger refuses a move when the successor is not current.
   The trigger refuses any move from a superseded state back to current.
   With these three refusals, the graph has no cycle: the last edge of a cycle would need a current successor that already has a successor.
   This fixes D1 and enforces L1 to L4 in the schema.
2. **Rust writers.** Keep the checks in `canon.rs`, `memory_write.rs`, and `write.rs` for clear refusal messages.
   Add `AND superseded_by IS NULL` to `memory_write.rs:215-216`, and refuse when the new memory is not current.
   The trigger stays the owner; the Rust check gives the message.
3. **Schema.** Change the canon CHECK to require `superseded_by` NULL for `archived` (`0015:16`).
   Make the canon unique index use `lower(name)` (`0015:26-28`). This fixes D5.
   Use one delete policy on all three FKs (ruling R3). This fixes D4.
4. **Bend2 model.** Model one generic table: rows with `next: Option<id>`, and the events `Insert`, `Supersede(old, new)`, `Rewrite(id)`, and `Delete(id)`.
   Prove L1 to L4 and L7.
   The proof must fail on L2 against today's memory writer; this shows that it catches D1.
   Presence stays out of Bend2 scope: it has two states and one owner.
5. **Contract test.** CI runs the same random event sequences through the Bend2 model and through the triggers on a real Postgres, and compares the rows.
6. **Presence.** The owner is `crates/akasha/src/presence.rs`. Change only the Host caller after ruling R4.

## Open rulings

- **R1. D2: memory rewrite of a superseded row.** Proposal: refuse the rewrite and name the successor ID in the refusal. A silent rewrite of a hidden row loses the operator's correction.
- **R2. D3: the archived states.** Proposal: keep the columns, and define `archived` as a terminal state that excludes `superseded_by`. Add an archive command only when an operator asks for one. The readers already filter on `archived_at` (`0015:65`).
- **R3. D4: delete policy.** Proposal: use `ON DELETE RESTRICT` on all three FKs. A delete then never changes the state of another row.
- **R4. D6: presence reopen.** Proposal: only the open command reopens a closed row. Compile and settle on a closed row report `Degraded` and leave the row closed.
