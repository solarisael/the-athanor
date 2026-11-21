# Operator canon

Status: proposed design. This document implements nothing and deploys nothing.
Sol and Tuner agreed this design on 2026-11-17. Section 13 lists the rulings.
All proposed columns, constraints, tool fields, and kinds below are new contracts.

## 1. Purpose and boundaries

A House holds facts about the people who use it.
Today those facts live in Markdown files and in memory prose, in several copies.
This design gives each fact one home: a canon row with a person as a key.

The design adds three columns to canon and one visibility rule.
It adds no table. The `operators` table from migration 0035 is the person registry.

The design must work in two deployments:

- A local House with one operator.
- An OMEGA House with many operators in units. Each organization has its own House.

This design excludes these items:

- A temporary raise of elevation. Elevation is a standing level.
- A level on canon that has no subject.
- A `fronting` field in room state. Section 11 records the gap.
- Person facts in worker dispatch packets.
- A move of the room identity contracts into PostgreSQL.
- A reorganization of all House memory. Section 12 covers the facts about one operator only.

### First pass

The first pass covers the local House of Sol only. Ruling 10 sets that limit.
The first pass builds these parts:

- The three columns, the two constraints, and the new index.
- Conditions 1, 2, and 3 of the visibility rule.
- The four readers, the write path, the Host stamp, and the tool fields.
- The registration of one unit and one operator.
- The profile entries and the operator card.

The first pass leaves these parts for the OMEGA pass:

- Condition 4 of the visibility rule. The write tool refuses the scope `up` for canon until then.
- The rename of `operators.rank` and the meaning of its numbers.
- The relation between an elevation and the system administrator flag.
- The deploy to the WDA House.

The columns for `up` are in the first migration. The WDA House then needs no second schema change.

## 2. Current source and proof limits

The source facts come from commit `b914e81` on `dev/next`.
Line references identify the inspected versions, not an immutable release.

| Fact | Evidence |
|---|---|
| `operators` holds `key`, `display_name`, `unit`, and `rank`. `units` is an `ltree` hierarchy. | `substrate/migrations/0035_operator_axis.sql:17-31` |
| `memories` carries `operator` and `scope` with the values `company`, `up`, and `private`. | `substrate/migrations/0035_operator_axis.sql:33-36` |
| One predicate filters every memory lane. A row with a null operator passes for every reader. | `crates/akasha/src/recall/scope.rs:63-79` |
| `operators.rank` has one reader. | `crates/akasha/src/recall/scope.rs:77` |
| The word `rank` also names retrieval order in the same crate. | `crates/akasha/src/recall/profile.rs`; `crates/akasha/src/recall/mod.rs` |
| Canon has no operator, subject, or scope column. | `substrate/migrations/0001_initial.sql:63-81`; `0015_canon_authority.sql:3-8` |
| One active canon row exists for each pair of room and name. | `substrate/migrations/0015_canon_authority.sql:26-28`; `crates/akasha/src/canon.rs:139-163` |
| Direct canon reads filter by room only. | `crates/akasha/src/canon.rs:270-320` |
| The recall canon lane filters by room and authority only. It returns at most 12 rows in the automatic projection. | `crates/akasha/src/recall/mod.rs:730-774` |
| Entity resolution reads every active canon row of the room and of `house`. | `crates/akasha/src/entity.rs:128` |
| The semantic vocabulary function reads every active canon row. | `substrate/migrations/0015_canon_authority.sql:38-51` |
| A recall mode changes order only. It selects no row and drops no row. | `crates/akasha/src/recall/profile.rs:1-6,52-61` |
| Conversation lifts the kinds `person`, `ritual`, and `mythology`. Work lifts the kind `project`. | `crates/akasha/src/recall/profile.rs:24-39` |
| Quiet mode does no proactive retrieval. | `crates/host/src/policy.rs:238` |
| Presence receives the canon rows that recall matched as turn data. It does not render them. No standing selection exists. | `crates/host/src/server/context_session/materials.rs:103-144`; `crates/summoning/src/presence/turn.rs:345-386` |
| Presence reads the relationship pulse from `presence-pulse.md` with a limit of 4096 characters. | `crates/host/src/server/context_session/materials.rs:13-36` |
| The Host stamps the operator on recall and on memory writes. A canon call carries no operator. | `crates/host/src/organ/mod.rs:384-397` |
| The GUI recall sends no operator. | `crates/host/src/server.rs:1892-1903` |
| The installed `athanor` binary takes the operator key from `rooms[].operator` in `runtime.json`. | `crates/athanor-install/src/supervisor.rs:241-258`; `crates/athanor-install/src/app.rs:60` |
| The room-state operator is a display name. It is a different field from the operator key. | `crates/host/src/room_state.rs:107-108,287-297` |
| No command registers a unit or an operator. | A search of the complete repository finds inserts only in tests. |
| The release pin of the schema is 45. The migration registry ends at 47. | `crates/hearth/src/lib.rs:4`; `installer/dependencies.json:4`; `crates/akasha/src/migrations.rs:244-248` |
| No Host field names a fronting alter. | A search of `crates/hearth/src`, `crates/protocol/src`, and `crates/host/src/room_state.rs` finds no match. |

These observations come from the live House on 2026-11-17. The schema version is 47.

- `operators` and `units` hold no rows.
- The Host stamps `sol` as the operator key. 266 memories carry it: 241 `up`, 17 `private`, and 8 `company`.
- 4191 memories have no operator. Every reader sees them.
- House memory 2025 holds the shared facts about Sol. It has no operator, and its scope is `company`.
- House canon has 14 active rows. No row is about Sol or about an alter.
- The room canon of Kodo holds 11 `alter` rows, 10 `person` rows, 16 `mythology` rows, and 1 `medical` row.
- The room canon of Kintsu holds 2 `person` rows.
- The room identity contracts of Kodo and Kintsu hold their own sections about Sol. This review read only the headings.
- The repository has no deploy path for a local Linux House. The live House runs the installed `athanor` binary.

This item is not verified:

- The rows in `operators` and `units` on the WDA House.

The OMEGA table in [Roadmap](ROADMAP.md#omega-organization-layer) inspects commit `a6ab453`.
That commit predates the operator axis. This document uses the current source.

### Earlier decisions that this design changes

Sol and Kintsu settled the operator axis on 2026-10-12. House memories 5697 and 5705 hold the record.

| Earlier decision | Effect of this design |
|---|---|
| `rank` is a job ladder: 0 staff, 1 lead, 2 owner. The owner at the root reads every `up` row. | Ruling 6 changes the meaning. An elevation is a grant. The OMEGA pass decides the numbers. |
| The system administrator is a separate token flag with a log. The owner of the system is not the owner of the company. | No change. This design does not define that flag. |
| Canon stays off the operator axis. A test asserts that `named_entities` has no `operator` or `scope` column. | This design puts canon on the axis. The test in `crates/akasha/tests/operator_scope_integration.rs:630` must change with the contract. |
| `memories.operator` lost its foreign key, because the key refused writes before registration. | `subject` keeps a foreign key by decision. Section 4 gives the reason. |
| A local House leaves the `operators` table empty. | The House of Sol must register `sol` in one unit before any row has a subject. |
| One migration goes to both Houses with the same commit. No patch goes to one House only. | The first pass deploys to the local House only. The migration changes no existing row, so the WDA House can take it later. |
| A refusal for a hidden row equals the refusal for an absent row. | Section 6 uses the same rule for canon. |

## 3. Terms

| Term | Meaning |
|---|---|
| operator | A person registered in `operators`. The operator key is the access key. |
| subject | The operator that a canon row is about. |
| person fact | A canon row that has a subject. |
| scope | How far a row travels: `company`, `up`, or `private`. |
| elevation | A standing integer level on an operator. A higher number reads more. |
| unit | The place of an operator in the organization. |
| reader | The operator key that the Host configuration names for a read. |
| writer | The operator that the Host names for a write. |
| operator card | The standing set of person facts in the Presence frame. |

A role is a fact. An elevation is a grant.
The row `role: owner` gives no access. Only the House keeper sets an elevation.
No tool lets an operator change an elevation.

## 4. Data model

```sql
BEGIN;

ALTER TABLE named_entities
  ADD COLUMN IF NOT EXISTS subject TEXT NULL REFERENCES operators(key),
  ADD COLUMN IF NOT EXISTS scope TEXT NOT NULL DEFAULT 'company',
  ADD COLUMN IF NOT EXISTS min_elevation INTEGER NULL;

-- Three constraints, each behind a guard:
--   named_entities_scope_check:         scope IN ('company', 'up', 'private')
--   named_entities_scope_needs_subject: subject IS NOT NULL OR scope = 'company'
--   named_entities_level_needs_up:      (scope = 'up') = (min_elevation IS NOT NULL)

DROP INDEX IF EXISTS named_entities_active_room_name_key;

CREATE UNIQUE INDEX IF NOT EXISTS named_entities_active_room_subject_name_key
  ON named_entities (room, (COALESCE(subject, '')), name)
  WHERE authority = 'active';

-- The vocabulary source function keeps only rows with the scope 'company'.

COMMIT;
```

The migration runner needs exactly one `BEGIN` and one `COMMIT` in the file.
Each statement has a guard, so the migration can run again.

The build raises the release pin of the schema from 45 to 48.
The pin is in `crates/hearth/src/lib.rs` and in `installer/dependencies.json`.
A new Host then refuses a database below schema 48.

The subject becomes the third key of canon, beside the room and the name.
Two operators can each hold an active fact named `role` in one room.
Migration 0035 made the same change for memories with `(room, operator, source_path)`.

`subject` is a foreign key. `memories.operator` stays plain text.
A House must write memories before registration. A fact about a person needs that person to exist.

A row without a subject keeps the scope `company`. All existing canon rows stay in that state.
A level on canon without a subject needs a unit anchor. This design does not define one.

The registry on the House of Sol is empty. The House keeper must register one unit and the operator `sol` first.
That registration changes no memory visibility for a single operator.
A new subcommand of `athanor-substrate` does the registration. A second run adds no second row.

The canon write path must use the new key in three places:

- The advisory lock in `canon.rs:140-146`.
- The conflict query in `canon.rs:148-154`.
- The constraint name in `canon_database_error`, `canon.rs:20-24`.

`operators.rank` holds the elevation of an operator. The OMEGA pass renames that column.
Warning: the rename breaks every binary that still reads `rank`.
Read the rollback contract of the deploy script before that rename.

## 5. Visibility rule

A reader sees a canon row when one of these conditions is true:

1. The row has no subject.
2. The scope is `company`.
3. The reader is the subject.
4. The scope is `up`, the unit of the reader contains the unit of the subject, and the elevation of the reader is at least `min_elevation`.

The room filter does not change. A room reads its own canon and the canon of `house`.

The rule has these consequences:

- No elevation opens a `private` row.
- The subject always reads the facts about the subject. The House keeps no hidden file on a person.
- The line of an `up` row follows the subject. The writer does not change it.
- A high elevation in one unit line reads nothing in a different line.
- A Host with no operator names no reader. That reader passes conditions 1 and 2 only.

The filter runs in the SQL `WHERE` clause, before order and before `LIMIT`.
A hidden row takes no slot. A hidden row changes no score, no count, and no excerpt.
A visible row never names a hidden successor or predecessor.
`ScopeFilter::visible_successor` in `scope.rs:85` shows the existing pattern for memories.

The `up` rule for memories does not change.

## 6. Write rule

The Host names the writer. A caller cannot supply the writer.
The Host refuses a subject that is not a registered operator.
The Host refuses a row with a subject when it names no writer.

| Scope of the new row | Who can write it | Source |
|---|---|---|
| `private` | The subject only | Ruling 4 |
| `company` | Any registered operator | Ruling 4 |
| `up` | A writer that passes the visibility rule for the new row | Ruling 8 |

A writer can supersede a row only when the writer can read every predecessor.
Only the subject can supersede a `private` row. Ruling 9 sets this rule for now.

A refusal for a hidden row equals the refusal for an absent row.
The conflict refusal for an active name must show no data of a hidden row.

A new row gets the name and the aliases of each predecessor as aliases. An old name then still finds the fact.
A new row gets those aliases only from a predecessor that every reader of the new row can see.
A `company` row does not get the name of a `private` predecessor. A `private` row gets the names of all its predecessors.

When a write names a subject and no scope, the domain layer in `hearth` sets `private`.
The memory default in `MemoryScope::default_for` does not apply to canon.

## 7. Canon readers

Every reading path of canon must apply the visibility rule.
A census on 2026-11-17 searched the complete repository for `named_entities`.
All SQL readers are in `crates/akasha/src` and in one SQL function.

| Reading path | Location | Change |
|---|---|---|
| Direct read by ID or name, with lineage | `crates/akasha/src/canon.rs:270-320` | Add the rule to each query and to each lineage step |
| Result projection | `crates/akasha/src/canon.rs:236-267` | Show `superseded_by` and `supersedes` only for visible rows |
| Predecessor lock and name conflict in the write path | `crates/akasha/src/canon.rs:41,149` | Add the rule and the subject key |
| Recall canon lane | `crates/akasha/src/recall/mod.rs:730-774` | Add the rule inside the `tiered` expression; apply the match rule below |
| Entity resolution | `crates/akasha/src/entity.rs:128` | Add the rule; the Host must stamp the operator on this call |
| Vocabulary source function | `substrate/migrations/0015_canon_authority.sql:38-118` | Define it again in the new migration; include only rows with the scope `company` |
| Vocabulary loader | `crates/akasha/src/recall/semantic_vocabulary.rs:40-60` | No change; it reads only the rows that the source function wrote |

Write one predicate for the rule and use it in every path.
`ScopeFilter` in `recall/scope.rs` is the pattern for memories.

The vocabulary index has no reader key. It can hold only rows that every reader can see.
A `private` or `up` fact gets no semantic bridge. It matches by subject, by name, and by summary text.

The Host stamps the operator on recall and on memory writes only.
It must also stamp the operator on canon reads, canon writes, and entity resolution.
The parameter structs of those three calls refuse unknown fields. Change the stamp and the structs together.
The GUI recall must also send the Host operator.

Several integration tests apply a hand-picked list of migrations. Add the new migration to each list.

### Match rule for person facts

The recall canon lane has three exactness tiers.
Tier 1 accepts a row when its name is a query term or appears inside the query.
A person fact named `role` then matches an ordinary sentence that contains the word `role`.

A person fact uses this rule:

- Tier 0 does not change. The complete query equals the name or an alias.
- Tier 1 needs the query to name the subject. The query must contain the operator key or the display name.
- Tier 2 does not change. The summary text matches the query.

A row without a subject keeps the current rule.

## 8. Recall modes and kinds

The visibility rule and the recall mode are separate layers.
The visibility rule selects the rows. The mode puts the selected rows in order.
No mode opens a row. No mode hides a row.

Person facts use this kind vocabulary:

| Kind | Content | Lift |
|---|---|---|
| `person` | A person in the life of the subject | Conversation, existing |
| `alter` | One member of a plural system | Conversation, new lift |
| `mythology` | Personal mythology | Conversation, existing |
| `role` | Work role and responsibility | Work, new |
| `preference` | How the subject wants work done | Work, new |
| `medical` | Health and body facts | None |
| `family` | Family patterns | None, new kind |

The profile change is three entries in `crates/akasha/src/recall/profile.rs`.
Conversation adds `alter`. Work adds `role` and `preference`.

No mode lifts `medical` or `family`. A lift changes order only inside one exactness tier.
An exact request for a medical fact still puts that row first.
Without a lift, these rows do not lead when the query names only the subject.

The canon lane selects at most 12 rows before the mode puts them in order.
The SQL selects by exactness, then `weighty`, then name (`crates/akasha/src/recall/mod.rs`).
A kind weight does not select a row. With more than 12 matches in one tier, `weighty` decides which facts arrive.

The profile is global. A room cannot lift a kind that other rooms leave flat.
The write tool does not enforce the kind list in this design.
The room canon of Kodo already uses the kinds `person`, `alter`, `mythology`, and `medical`.

## 9. Tools

| Tool | Change |
|---|---|
| `canon_read` | Add an optional `subject`. With `name` and `subject`, read one fact. With `subject` only, list the visible facts of that subject. An `id` does not combine with a `subject`. |
| `canon_write` | Add an optional `subject`, `scope`, and `minElevation`. Section 6 gives the refusals. |
| `recall` | No new argument. The Host stamps the reader. |

`canon_read` with a name and no subject returns every visible row with that name in one room.
A read that names a subject covers the session room and `house`.
A manual read ignores the recall mode.

## 10. Operator card

The harness loads `AGENTS.md` and the files that it names. The Host cannot change that load.
Presence is the startup path that the Host owns.
The Host opens one Presence frame for each session and sends its text again on every turn.
Only the turn contract changes from turn to turn.

The operator card puts standing person facts into that frame.

- Reader: the operator key from the Host configuration. The room-state operator is a display name, and a prompt can change it.
- Selection: active rows where the subject is that reader and `weighty` is true.
- Rooms: the room of the session and `house`.
- Content: one line for each fact. The line holds the name and the start of the summary.
- Limit: 4096 characters for the complete card, and 40 rows. The last line shows a mark when a limit cuts the card.
- Mode: the card does not depend on the recall mode. It arrives in Quiet mode.
- No reader: a Host with no operator key sends no card.

The card is an index. The complete row comes from `canon_read`.
The card has a cost on every turn, because the frame text goes out on every turn.
The card does not change during a session. A new fact shows in the card of the next session.
Recall and `canon_read` show a new fact immediately.

The census confirmed this carrier:

- Each row is one material in the `continuity` group of the open request (`assembly.rs:430-432`).
- The authority is `PresenceAuthority::Canon`. The material ID has the prefix `operator-card:`.
- A `canon:` ID collides with a recall match of the same row. Presence then refuses the turn.
- The Host applies both limits. Presence refuses a frame with more than 48 materials, and it refuses a body over 4096 characters.
- A frame rotation removes canon materials (`identity.rs:181-192`). The rotation must select the card again.
- A new function in `akasha` reads the card rows. No existing call lists canon by subject.

## 11. Alters and fronting

An alter gets a canon key. An alter does not get an operator key.

Each alter is one canon row:

- `name`: the name of the alter, with its aliases.
- `kind`: `alter`.
- `subject`: the operator key of the person.
- `scope`: `private`.

The alters are the operator. Ruling 11 sets this: the rows are facts about Sol, and no room owns them.
The alter rows live in `house` with the subject `sol`. Every room can then read them for that reader.

The room canon of Kodo already holds 11 `alter` rows. Those rows have no subject.
Section 12 covers those rows in the move of material.

The operator key is the access key. It holds one credential, one elevation, and one private boundary.
Separate operator keys put that boundary inside one person.
An alter then cannot read the private rows of the person. That result is wrong.

Gap: the Host has no field for the alter that fronts.
A memory can name the alter only in its prose.

A later design can add one optional `fronting` value to room state.
The value gives attribution only. It gives no access. This design does not include it.

## 12. Move of existing material

Do this step after the build is live. The tools for it do not exist before that.

1. Read the existing canon rows about the operator in each room.
2. Divide the shared note into one row for each fact that has no row.
3. Show the list of rows, kinds, scopes, and `weighty` marks to the operator.
4. Write no row before the operator approves the list. The material belongs to the operator.
5. Write the rows with `canon_write`. Supersede an old row to give it a subject.
6. Amend House memory 2025 so that it points to the rows. The memory stays as history.
7. Ask Kodo and Kintsu to remove the fact sections from their identity contracts.

Supersession works inside one room. A row in `house` cannot supersede a row in the room of Kodo.
The operator decides in step 3 what happens to each old room row.

The identity contracts are hand-owned. Each room edits its own contract.
The relationship text in a contract stays in the contract. It is not a fact about the operator.

## 13. Rulings

Sol made these rulings on 2026-11-17.

1. Elevation is a standing level. It is not a temporary raise.
2. The subject always reads the facts about the subject.
3. Each organization has its own House. The House of Sol has one operator.
4. Any operator writes a `company` fact. Only the subject writes a `private` fact.
5. A person fact needs the query to name its subject, or to name the fact exactly.
6. A role does not set an elevation. The owner of a company is not the highest level by default.
7. The order of work is architecture, then build and deploy, then the move of material.
8. A writer can write an `up` fact only when that writer can read it.
9. Only the subject supersedes a `private` row. Sol set this rule for now.
10. The first pass covers the local House only. The WDA House and its owner are not part of it.
11. The alters are the operator. The alter rows are facts about Sol, and no room decides where they live.

Sol approved this document on 2026-11-17, with rulings 8 to 11.

Ruling 6 changes a decision from 2026-10-12. Section 2 names that decision.

Sol asked why no mode lifts body facts and family facts. Section 8 gives the answer.
He did not object. Tuner records the flat state as accepted and open to change.

Tuner recorded `fronting` as a later item. Sol did not rule on it.

## 14. Open items before the build

- Select the unit for `sol` on the House of Sol. The registration command takes it as an argument.

The census closed the other items. Sections 7, 10, and 15 hold the results.

The OMEGA pass holds these items:

- Decide the meaning of the numbers in `operators.rank`. The earlier ladder is 0 staff, 1 lead, 2 owner.
- Decide how ruling 6 and the system administrator flag fit together.
- Read the rows in `operators` and `units` on the WDA House.
- Read the rollback contract of the deploy script before the rename of `rank`.
- Close the existence oracle of the unique index. A write that collides with a hidden row refuses, and the writer can see that refusal.
- Decide the state of a public row that a hidden row supersedes. The public row shows `superseded` with no successor, so a reader knows that a hidden successor exists.
- The `athanor-substrate` command reads the database without the Host stamp. A person with database access reads every row.

## 15. Build sequence and acceptance

1. Write the migration, the domain types, and the one visibility predicate.
2. Change the reading paths and the write path.
3. Add the Host stamp, the GUI recall operator, and the tool fields.
4. Add the profile entries.
5. Add the operator card.
6. Add the registration command to `athanor-substrate`.
7. Prove the scenarios on an isolated PostgreSQL database.
8. Deploy to the local House. Use the steps below for a Linux House.

### Linux deploy

Use `substrate/deploy-local.sh` on a provisioned systemd user installation.
The script uses the existing user unit and its database environment.
It requires GNU Coreutils with `mv --exchange` and a filesystem that supports directory exchange.
Keep the live checkout and the Athanor state on the same filesystem.

OMP sessions can stay open during deployment.
Warning: run the script as its own process, with the system bash. A shell that runs it in-process skips the recovery step.
Warning: after migration starts, a failure leaves the Host stopped and the cutover marker in place.

1. Build the release binaries from the source checkout.
2. Run the deploy script from that checkout.
3. Keep the release receipt path and the database manifest path.
4. Restart each OMP session that the script lists. A session loads the new adapter only at its start.

```sh
cargo build --locked --release \
  -p athanor-install -p host -p akasha \
  --bin athanor --bin athanor-host --bin athanor-substrate

bash substrate/deploy-local.sh \
  "$HOME/Projects/the-athanor" "$HOME/.local/state/athanor" \
  "${CARGO_TARGET_DIR:-target}/release" \
  --register-operator sol Sol "$unit_path" "$unit_name"
```

Set `unit_path` and `unit_name` to the approved values before the first deployment.
Omit `--register-operator` and its four values when registration is not needed.
The script stops the Host, makes a backup, applies the migration, and registers the operator if requested.
It installs the binaries and the OMP adapter, starts the Host, and checks Host and substrate health.

### Linux recovery

Keep the Host and OMP sessions stopped during recovery.
Use the path in `linuxReceipt` in the cutover marker to find the release receipt.
The receipt keeps the database manifest, its dump, the old binaries, and the adapter directory.
After the adapter exchange, the adapter directory in the receipt holds the old adapter.

Do not start the old binaries against the migrated database.
The `athanor-substrate restore` command refuses a backup from a different migration lineage.
Restore the verified dump into a separate database with the original migration lineage.
Point the user unit at the restored database only after you verify it.
Restore the previous binaries and restore the previous adapter if the script exchanged it.
Remove the cutover marker after recovery.
Start the Host and check its health.

| ID | Scenario | Required observable result |
|---|---|---|
| P01 | The subject writes a `private` fact and reads it | The row is visible |
| P02 | A reader with the highest elevation in an ancestor unit reads that row | No row; the result equals the result for an absent row |
| P03 | An `up` fact has `min_elevation` 5; readers have 4 and 5 in the line, and 9 in a different line | Only the reader with 5 sees the row |
| P04 | Another operator writes an `up` fact about a subject; the subject reads it | The row is visible |
| P05 | A writer that is not the subject writes a `private` fact | Refusal; no row |
| P06 | Two subjects each write an active fact named `role` in one room; one subject writes it again without `supersedes` | Two rows exist; the third write refuses |
| P07 | Add 50 hidden person facts; repeat fixed recall queries as another reader | Canon matches, order, counts, recognized entities, and vocabulary do not change |
| P08 | A query contains the word `role` and names no subject | No person fact named `role` enters tier 1 |
| P09 | A query names a subject that has 40 visible facts | At most 12 rows. SQL selects them by exactness, `weighty`, and name. The mode then orders those rows by kind weight |
| P10 | Set Quiet mode and send a turn | The frame carries the operator card; no proactive recall occurs |
| P11 | Mark 100 facts `weighty` | The card holds at most 40 rows and 4096 characters, and it shows a mark |
| P12 | Start a Host with no operator | No `private` or `up` row is visible; a write with a subject refuses |
| P13 | Put canon rows in a database before the migration, then apply the migration and read them | Rows, order, and counts equal the expected lists |
| P14 | Write a canon row with the scope `up` in the first pass | Refusal; no row |
| P15 | A reader reads a row whose successor is hidden | The result does not name the successor |
| P16 | A writer names a hidden row in `supersedes` | The refusal equals the refusal for an absent ID |
| P16b | A writer writes a name that a hidden row with the same subject holds | The ordinary conflict text, with no data of the hidden row. The OMEGA pass closes this existence oracle |
| P17 | Repeat each attack of `operator_scope_integration.rs` with canon rows | No attack reads or changes a hidden row |
| P18 | A frame rotation occurs in a session that has a card | The new frame carries the card |
| P19 | Recall matches a row that is also in the card | Presence compiles the turn with no conflict refusal |
| P20 | Run the registration command two times | One unit row and one operator row exist |

Scenarios P03 and P04 belong to the OMEGA pass.

The design is ready for the build only after an independent review.
The build is complete only after these scenarios pass on a live House.
