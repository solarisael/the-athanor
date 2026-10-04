# Ontology contract (A0) — DRAFT

Status: draft for three-chair review and Sol's shaping.
Quest: A0 `b3292fcd`, goal A `0848b2d0` ("Foundation before floors").
Evidence: House memories #3908 and #3912, guild-hall #192/#194/#196.

## 1. The problem

The column `memories.type` carries four different meanings in one free-text field.
The live census found 52 distinct values across 1,884 memories.
Machines route on exact strings with no vocabulary fence.
Boat #3414 (typed `boat`, not `paper-boat`) received no crane delivery.

The four meanings:

1. **Mechanical kind** — what machines do with the record. Example: `paper-boat`.
2. **Provenance** — how the record entered the substrate. Examples: `session`, `legacy-unindexed`.
3. **Semantic class** — what the record is about. Examples: `architecture`, `lore`.
4. **Facets** — stacked qualities. Example: `session+crisis+lesson+scene`.

## 2. The rulings (Sol, 2026-08-23)

1. **Automatic decisions route through code.** Consumers 1–3 (crane trigger, claim guard, wake path) are agent-facing and automatic. No LLM sits in any automatic lane. Deterministic code evaluates declared attributes.
2. **The database evolves constantly.** Vocabularies live in rows, not in DDL. Evolution is an insert plus a supersede, never a migration.
3. **Origami is the message-shape family.** It runs on PostgreSQL-authoritative storage with NATS-only delivery. Boats are stasis in the Sea with a return point. Cranes are movement in the Sea with a destination point. The shapes touch at one seam: `boat.ready`. Hallways and project routing belong to the same family.
4. **Modularity rule** (coding lesson #446). Every concern is a module in its own folder. Shared vocabulary earns its own crate. Founding example: `crates/origami`.

## 3. The design

### 3.1 Mechanical kind — axis 1

- A registry table `memory_kinds` holds one row per kind.
- Each row carries behavior flags. First flags: `stasis_return` (wake-eligible), `retry_posture` (the DO-NOTHING conflict path boats use today).
- Code routes on flags, never on string literals. The crane trigger, the claim guard, and the wake path read the flag.
- `memories.kind` gets a foreign key to the registry. An unknown word cannot enter the column. A new word enters by declaration first, with authorship.
- Registry rows use the canon authority lifecycle: active, superseded, never overwritten, lineage kept.
- Starting rows: `record` (the default) and `paper-boat`. The closed list stays small. Expressive meaning moves to axes 3 and 4.

### 3.2 Provenance — axis 2

- Provenance names how the row entered: `session`, `legacy-import`, `organ` (which organ), `giga-promotion`.
- Open question for Sol: own column with a small registry, or a declared key inside `meta`. The census must stay cheap to run.

### 3.3 Semantic class and facets — axes 3 and 4

- One governed tag vocabulary serves both. A class is a tag with a `class` role; a facet is a tag with a `facet` role.
- The vocabulary table uses the same authority lifecycle as canon.
- Recall and BM25F read active vocabulary only. Superseded words fade from ranking on refresh (the 0015 pattern).
- The GUI taxonomy panel shows the whole vocabulary. It stops truncating to 12.

### 3.4 `named_entities.kind`

- Nothing routes on it. It stays a human word.
- Governance is advisory: a registry that warns on unknown words and never refuses.
- Refusal is reserved for words machines obey.

### 3.5 Thread keys

- Normalize at write: trim, collapse whitespace, lowercase the key for identity while keeping a display form.
- A merge path folds duplicate stems and preserves events and links.
- Decided in A4, stated here for contract completeness.

## 4. Enforcement placement

- Database: foreign keys to registry tables. Sharp refusal at the storage boundary.
- Rust: organs validate against the registry and read behavior flags. Both layers see the same rows.
- SQL triggers: read flags via join to the registry, never string literals. Until A1 lands, the 0016 trigger literal is the one sanctioned duplicate.

## 5. The 52 → N mapping (appendix, draft)

Legend: **M** = mechanical fold, migration-owned. **J** = judgment row, Sol's hand.
Target shape: `kind` + `provenance` + `tags` (class/facet roles).

| current value | count | fold | proposal |
|---|---|---|---|
| memory | 818 | M | kind=record |
| session | 539 | M | kind=record, provenance=session |
| paper-boat | 330 | M | kind=paper-boat |
| legacy-unindexed | 60 | M | kind=record, provenance=legacy-import |
| reference | 28 | M | kind=record, class=reference |
| infrastructure | 10 | J | class=infrastructure or class=architecture? |
| project | 10 | M | kind=record, class=project |
| architecture | 9 | M | kind=record, class=architecture |
| session+lesson | 8 | M | provenance=session, facet=lesson |
| core | 8 | J | class=identity? weighty flag? |
| divination | 5 | M | class=divination |
| canon | 5 | J | overlaps canon organ — class=canon or migrate to named_entities? |
| session+project+self-note | 2 | M | provenance=session, class=project, facet=self-note |
| session+self-note | 2 | M | provenance=session, facet=self-note |
| coding-lessons | 2 | J | overlaps lesson store — class=lesson or migrate rows? |
| note | 2 | J | class=note or fold into record with no class? |
| state | 2 | J | class=state-shift? |
| session+architecture | 2 | M | provenance=session, class=architecture |
| lore | 2 | M | class=lore |
| personal | 2 | J | class=personal or facet? |
| reading | 2 | M | class=reading |
| state-shift | 2 | M | class=state-shift |
| project-memory | 2 | M | class=project |
| engineering-memory | 2 | J | class=architecture or class=project? |
| doctrine | 2 | M | class=doctrine |
| boat | 1 | M | kind=paper-boat (memory #3414; manual crane enqueue or tombstone in A1) |
| handoff | 1 | J | facet=handoff? |
| skill | 1 | J | class=skill or fold? |
| session+canon+architecture | 1 | M | provenance=session, class=architecture, facet=canon |
| orientation | 1 | J | — |
| discovery | 1 | J | — |
| session+lesson+architecture | 1 | M | provenance=session, class=architecture, facet=lesson |
| shared-work-convention | 1 | J | class=doctrine? |
| session+architectural-build | 1 | M | provenance=session, class=architecture |
| operator-handling | 1 | J | class=doctrine? sensitive — Sol reads |
| wishlist | 1 | M | class=wishlist |
| lesson | 1 | J | same question as coding-lessons |
| creative-direction | 1 | J | class=design? |
| identity | 1 | J | class=identity |
| planning | 1 | M | class=planning |
| session+crisis+lesson+scene | 1 | M | provenance=session, facets=crisis,lesson,scene |
| session+disclosure+lesson+canon | 1 | M | provenance=session, facets=disclosure,lesson,canon |
| feedback | 1 | J | — |
| session+self-note+project-memory | 1 | M | provenance=session, class=project, facet=self-note |
| reference+session | 1 | M | provenance=session, class=reference |
| operator-doctrine | 1 | M | class=doctrine (fold with doctrine) |
| divination-record | 1 | M | class=divination |
| session+rule | 1 | M | provenance=session, facet=rule |
| session+architectural-decision+autonomy-correction | 1 | J | facets need naming |
| session+lesson+design | 1 | M | provenance=session, class=design, facet=lesson |
| family | 1 | J | class=family or class=personal? |
| orientation, discovery, feedback | — | J | candidates to fold into record + facet |

Original strings stay in `meta.original_type` on every folded row.

## 6. Acceptance

- The three chairs review this contract before A1 starts.
- Sol resolves every J row.
- Every following quest names its reviewer-runnable census query.
