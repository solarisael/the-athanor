# Origami census — 2026-08-23

Six read-only census agents walked the `ontology` branch: three Faro
(Claude family) and three Cisma (GPT family), one pair per territory.
Redundancy was deliberate. Full tables live in the agent transcripts;
this file keeps the consolidated findings. The skeleton functions in
`crates/origami` carry the absorb map as doc comments (`file:line`).

## Coverage

- Flight side: house-delivery (broker, model, store, lib, main, tests),
  house-host NATS surfaces, installer NATS packaging. 241 rows (Faro),
  55 rows (Cisma).
- Write/wake side: house-substrate src (23 files), migrations (25
  files). 246 rows (Faro), 29 rows (Cisma).
- Contracts and edges: house-core, house-protocol, adapters/omp, plus
  a repo-wide sweep. 361 rows total with 93 doors (FaroEdges).

## Findings that shape the extraction

1. Hallways do not ride NATS. Confirmed by a full walk of
   hallway.rs (1,510 lines): posts, Bells, and Knocks write PostgreSQL
   rows only. Readers reach them through Host projections over
   WebSocket. The family doc claim was aspirational; lib.rs now states
   the truth. CismaSubstrate hedged the opposite; FaroSubstrate's
   zero-match walk is the grounded reading and lib.rs follows it.
2. 32 bare boat-vocabulary literals remain outside crates/origami:
   `"paper-boat"` x22 (Rust 18, SQL 4), `"boat.ready"` x8,
   `"boat.ready.v1"` x2. recall.rs excludes boats through the raw
   literal in about ten places (:404, :430, :470, :963, :1012, :1041,
   :1092, :1124, :1374, :1389), plus cluster.rs and timeline.rs.
3. The hallway family has zero SQL triggers and zero SQL functions.
   Every invariant beyond constraints lives in Rust. The crane family
   has the opposite posture: its enqueue is a database trigger
   (0016:95, recreated 0017:214).
4. health.rs requires the crane trio (crane_outbox, crane_receipts,
   crane_dead_letters) and never checks any hallway table. Substrate
   health reports green with the whole hallway family missing.
5. The substrate stdio protocol exposes 7 of 9 hallway methods; knock
   claim and settle are reachable only through house-host (WebSocket).
6. adapters/omp/house-proof/host.ts declares
   `registerEmbodiedSession` twice (:76, :101), byte-identical.
   Cleanup candidate, verdict pending.
7. The one call site honoring the extracted boat vocabulary at census
   time: paper_boat.rs:46 and remember.rs:448 (this branch's own
   commit 71d41b4). Everything else predates it.
8. Empty mouths (searched, zero hits): state/, repo tests/, .github/,
   house-vault; house-host/src/main.rs has no delivery wiring; the
   installer never names lanes — binary packaging only.
9. Unwalked, named honestly: athanor-install supervisor/layout
   internals (outside all three territories); adapters/omp non-Origami
   modules; docs diagrams. Whatever launches nats-server.exe at
   runtime lives in athanor-install — walk it before moving broker
   lifecycle code.

## Standing law for the extraction

JetStream dedup ends at the stream boundary (coding#368). The
commit-before-ack receipt ledger in store.rs already follows the law;
the extraction must preserve it, never simplify it away.
