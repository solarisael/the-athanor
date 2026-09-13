# The Athanor Lesson Map

The PostgreSQL lesson registry is authoritative. This file names lessons by number. Query them directly:

```json
{"type":"coding","ids":[9,10,11,12,13,14,19,20,27,47,49,50,51,52,90,91,96,101,109,112,118,126,127,128,129,134,143,151,155,158,159,160,161,162,164,165,166,167,171,175,178,180,182,184,187,188,190,194,203,204,214,215,216,217,218,219,220,222,223,224,230,236,238,240,242,252,257,258,260,262,308,311,315,316,317,322,323,325,328,329,330,331,333,337,340,341,346,347,350,352,354,356,363,369,370,374,375,377,382,389,446,447,453,454,456,457,458,459,460,461,463,464,468,469,470,471,472,473,475]}
{"type":"project","ids":[70,74,76,116,117,118,119,120,121,122,123,125,126,127,129,130,131,338,450,462]}
{"type":"design","ids":[293,294,295,296,297,298,299,300,301,302,303,304]}
```

IDs are scoped by family; always pass `type`. Direct lookup needs an Athanor at or after `352b85e` (dev/next, 2026-09-13).
On an older build, lessons stay hidden from a bare query; supply `languageKeys` and `technologyKeys` from the repo’s stack.
Refreshed: 2026-09-13.

## Always

- Coding #468 — Ponytail ladder. Use for: every code-touching task.

## House coding default — Ponytail

- Coding #469 — Ponytail review. Use for: diff review.
- Coding #470 — Ponytail audit. Use for: repository audits.
- Coding #471 — Ponytail help. Use for: lesson-family explanations.
- Coding #472 — Ponytail debt. Use for: deliberate shortcuts.

## Recovery anchors

- Coding #217 — Implement from the newest authoritative intent. Use for: authority recovery.
- Coding #218 — Make typed stores impossible to query as interchangeable rows. Use for: typed-store scope.

## Program boundaries

- Coding #217 — Implement from the newest authoritative intent. Use for: phase decomposition.
- Coding #184 — Explore accepted behavior before rebuilding it. Use for: parity inventories.
- Coding #218 — Keep typed stores distinct. Use for: shared storage contracts.
- Project #121 — Bound Athanor claims before automating proof. Use for: observable claims.
- Project #116 — Athanor claims follow repository ownership. Use for: cross-layer claims.
- Coding #219 — Sweep for orphans after a port. Use for: port cleanup.
- Coding #316 — Execution has a zero inference budget. Use for: unmapped execution paths.

## Rust convergence and clean cutover

- Coding #217 — Implement from current authority. Use for: Rust ownership.
- Coding #184 — Inventory before replacing. Use for: replacement contracts.
- Coding #219 — Sweep after replacing. Use for: cutover cleanup.
- Project #338 — Observe worker exit before replacement. Use for: process replacement.
- Project #131 — Verify Rust changes in a complete isolated pair when needed. Use for: shared-runtime verification.
- Coding #9 — Plain line, clean door, sharp refusal. Use for: Rust surfaces.
- Coding #10 — Names reveal motion and do not lie. Use for: Rust naming.
- Coding #11 — Names reveal motion and do not lie. Use for: Rust naming.
- Coding #12 — A file has one silhouette. Use for: module responsibility.
- Coding #13 — A bad helper name exposes a false abstraction. Use for: helper boundaries.
- Coding #14 — Code knows what it refuses. Use for: unsupported states.
- Coding #19 — Keep ugly interop in one named place. Use for: platform glue.
- Coding #20 — Remove helpers that launder anxiety. Use for: wrappers.
- Coding #27 — Write the honest first shape, then compress. Use for: new boundaries.
- Coding #47 — Centralize semantic duplication. Use for: repeated contracts.
- Coding #143 — Centralize semantic duplication. Use for: repeated contracts.
- Coding #158 — Use the smallest honest native shape. Use for: native implementation choices.
- Coding #159 — Use the smallest honest native shape. Use for: native implementation choices.
- Coding #160 — Use the smallest honest native shape. Use for: native implementation choices.
- Coding #330 — Prove Rust ownership and concurrency. Use for: Rust lifecycle contracts.
- Coding #460 — Complexity budgets apply to functions, modules, and crates. Use for: complexity review.
- Coding #446 — Every concern is a module, a smaller project inside the bigger project. Use for: module boundaries.
- Coding #457 — Prove the version of the idea before perfecting it. Use for: design reassessment.
- Coding #473 — Marker comments form a bounded grep ontology (IN TESTING). Use for: marker comments.
- Coding #315 — (title in registry). Use for: independent replacement design.

## Refactor slices (crate folds, module collapses, cutovers)

- Coding #468 — Ponytail ladder. Use for: refactor slices.
- Coding #382 — Line breaks expose mental operations. Use for: moved expressions.
- Coding #389 — An empty catch is a silent fallback. Use for: row decoding.
- Coding #194 — Comments point outward. Use for: moved comments.
- Coding #337 — Comments point outward. Use for: moved comments.
- Coding #219 — Sweep for orphans after a move. Use for: moved modules.
- Coding #260 — Exercise the real async lifecycle. Use for: Host-spawned loops.
- Coding #475 — Rewrite from the schema; never regex-transform the old text. Use for: schema rewrites.

## Vault, AKASHA, and retrieval

- Coding #218 — Typed stores are not interchangeable rows. Use for: durable-store variants.
- Project #74 — Recall with short rare terms. Use for: evidence recovery.
- Project #123 — Temporal attention never mutates authority. Use for: retrieval decay.
- Project #125 — Source identity belongs to each adapter contract. Use for: source adapters.
- Project #126 — The embedding prefix pair is load-bearing. Use for: embedding calibration.
- Project #127 — Novelty is measured against existing memory. Use for: novelty claims.
- Project #130 — GIGA ingest caps can discard silently. Use for: ingest batches.
- Project #122 — Hippocampus cognition remains in Rust. Use for: cognition ownership.
- Coding #222 — Retrieval thresholds are model-bound calibration. Use for: retrieval cutoffs.
- Coding #223 — Test the opposite assumption. Use for: retrieval alternatives.
- Coding #164 — Centralize retrieval filtering. Use for: candidate eligibility.
- Coding #180 — Measure retrieval drift. Use for: routing and ranking changes.
- Coding #214 — Respect pgvector HNSW dimensional limits. Use for: vector indexes.

## PostgreSQL outbox and NATS

- Coding #49 — Inserts declare their idempotency key. Use for: repeated inserts.
- Coding #50 — Inserts declare their idempotency key. Use for: repeated inserts.
- Coding #51 — Migrations are safely repeatable. Use for: partial migrations.
- Coding #52 — Migrations are safely repeatable. Use for: partial migrations.
- Coding #333 — PostgreSQL design follows measured workload and authority boundaries. Use for: database design.
- Coding #238 — Prove identity before `ON CONFLICT`. Use for: conflict targets.
- Coding #311 — Materialize join keys. Use for: correlation identity.
- Coding #242 — Ledger current state belongs in a table. Use for: current-state projections.
- Project #121 — Bound the delivery claim. Use for: delivery guarantees.
- Coding #223 — Test competing delivery states. Use for: delivery failures.
- Coding #260 — Exercise registered async lifecycles. Use for: delivery lifecycle proof.
- Project #338 — Shutdown drains before replacement. Use for: delivery shutdown.

## GUI and design translation

- Design #293 — Weight follows consequence. Use for: action weight.
- Design #294 — Unsafe states are unconstructible. Use for: component safety.
- Design #295 — A disabled control always says why. Use for: disabled controls.
- Design #296 — Authority, relevance, chronology, and health never share a channel. Use for: epistemic channels.
- Design #297 — Fixed copy cannot be softened. Use for: disclosures and warnings.
- Design #298 — Tokens are the only source of color and type. Use for: platform resources.
- Design #299 — One root owns the document; axes stay orthogonal. Use for: document state.
- Design #300 — Effects are spend; quiet is the carrier. Use for: motion and glow.
- Design #301 — State survives greyscale and stillness. Use for: redundant state signals.
- Design #302 — Readable text meets the floor; muted is decoration. Use for: text contrast.
- Design #303 — Styling reach-order. Use for: styling choices.
- Design #304 — Product vocabulary composes canon. Use for: product components.
- Project #117 — The GUI combines three grammars. Use for: GUI translation.
- Project #118 — Memory mapping is a first-class operation. Use for: memory operations.
- Project #119 — Renderer semantics stay layered. Use for: renderer layers.
- Project #120 — Evaluate UX for agent operators first. Use for: operator controls.
- Coding #134 — Frontend UX proof belongs to the rendered surface. Use for: visual proof.
- Coding #258 — A URL is not navigation. Use for: GUI entry paths.
- Project #462 — (title in registry). Use for: the parked Godot client boundary.
- Coding #165 — (title in registry). Use for: Godot work only if Sol reopens the client.
- Coding #166 — (title in registry). Use for: Godot work only if Sol reopens the client.
- Coding #167 — (title in registry). Use for: Godot work only if Sol reopens the client.
- Coding #331 — (title in registry). Use for: Godot work only if Sol reopens the client.
- Coding #341 — (title in registry). Use for: Godot work only if Sol reopens the client.
- Coding #375 — (title in registry). Use for: Godot work only if Sol reopens the client.

## Installation, migration, and release hardening

- Coding #51 — Migrations are idempotent. Use for: installation migrations.
- Coding #52 — Migrations are idempotent. Use for: installation migrations.
- Project #70 — Windows-to-WSL path translation. Use for: old WSL maintenance or migration.
- Project #76 — Preserve the public documentation spine. Use for: release documentation.
- Project #121 — Public claims are bounded and falsifiable. Use for: platform claims.
- Project #131 — Live Rust verification needs a safe paired environment. Use for: live verification.
- Coding #190 — Registration is not execution-branch proof. Use for: destination branches.
- Coding #203 — Search limits bound absence claims. Use for: orphan claims.
- Coding #204 — Recover only hidden evidence. Use for: elided evidence.
- Project #338 — Process replacement waits for observed exit. Use for: release teardown.
- Coding #90 — Deployment state is not runtime proof. Use for: activation proof.
- Coding #91 — Deployment state is not runtime proof. Use for: activation proof.
- Coding #109 — Bound deletion collateral. Use for: destructive cleanup.
- Coding #182 — Kill only owned processes. Use for: process termination.
- Coding #224 — Supersede every indexed name layer. Use for: renames and corrections.
- Coding #262 — Audit authorization after backfill. Use for: migration privileges.
- Coding #308 — Probe migrations with bounded stratification. Use for: migration samples.

## Documentation

- Coding #188 — Use ASD-STE100 Simplified Technical English. Use for: project documents.
- Coding #454 — A ruling is not captured until its symptom is armed. Use for: lesson writes.
- Coding #337 — Comments point to project lessons. Use for: durable reasoning.
- Coding #151 — A keep-in-sync comment admits a structural crack. Use for: duplicated truth.
- Coding #194 — Comments preserve intent and connection, never code mechanics. Use for: source comments.
- Coding #453 — Score code against its job sentence and platform owner. Use for: code assessments.
- Project #76 — Preserve the public documentation spine. Use for: canonical entry surfaces.
- Project #129 — Use the Athanor's own vocabulary. Use for: House terminology.
- Project #116 — Claims follow repository ownership. Use for: document authority.
- Project #121 — Bound claims before proof. Use for: evidence reports.

## Subagents and fanout

- Coding #317 — A worker contract is a quest, not a wall. Use for: worker briefs.
- Coding #340 — Subagents are kittens. Use for: peer dispatch.
- Coding #220 — Fetch lessons before risky work. Use for: lesson delivery.
- Coding #328 — Census once, then batch coordinates. Use for: broad work.
- Coding #459 — A cartographer needs an enumerable manifest, hard negative space, and a deterministic receipt. Use for: census contracts.
- Coding #322 — Workers wake at the project root. Use for: worker roots.
- Project #450 — (title in registry). Use for: the deploy-only clone boundary.
- Coding #217 — Use current authority. Use for: worker authority.
- Coding #316 — Execution has zero inference budget. Use for: unmapped worker paths.
- Coding #337 — Comments point to project lessons. Use for: worker comment scope.
- Coding #175 — Keep outcome reports short. Use for: worker reports.
- Coding #203 — Bound source claims. Use for: search coverage.
- Coding #204 — Bound source claims. Use for: elided evidence.
- Coding #178 — Classify parallel slices before dispatch. Use for: parallel decomposition.
- Coding #323 — Use target-repository worktrees. Use for: isolated workers.
- Coding #325 — Serialize shared proof tools. Use for: singleton proof surfaces.

## Verification order

- Coding #329 — Real proof precedes regression seals. Use for: verification order.
- Coding #346 — Real proof precedes regression seals. Use for: verification order.
- Coding #155 — Tests are safety nets, not coverage theatre. Use for: regression guards.
- Coding #161 — Uncontrolled green is not a pass. Use for: isolated proof.
- Coding #162 — Test the real user path, not a proxy signal. Use for: user-path proof.
- Coding #190 — Registration is not execution proof. Use for: destination branches.
- Coding #456 — A test changed with its asserted constant is not a test. Use for: test contracts.
- Coding #447 — A green probe proves only its exact probe. Use for: proof scope.
- Coding #458 — Map, cut, two knives, proof, repeat. Use for: adversarial review.
- Coding #463 — Parallel work is unlanded until the merged tree passes its real gate. Use for: integration proof.
- Coding #257 — Fake-backed boundary tests are theatre. Use for: boundary fixtures.
- Coding #240 — Fake-backed boundary tests are theatre. Use for: boundary fixtures.
- Coding #215 — Integration proof crosses the real boundary. Use for: integration boundaries.
- Coding #126 — Green output is not intention proof. Use for: intention checks.
- Coding #127 — Green output is not intention proof. Use for: intention checks.
- Coding #128 — Align before implementation. Use for: preflight alignment.
- Coding #129 — Align before implementation. Use for: preflight alignment.
- Project #121 — Claims name falsification conditions. Use for: release gates.
- Coding #223 — Test the opposite assumption. Use for: failure states.
- Coding #219 — Sweep for orphans after cutover. Use for: displaced owners.
- Coding #252 — Sweep sibling lookups after re-keying. Use for: identity changes.
- Coding #260 — Exercise real asynchronous lifecycles. Use for: callback proof.
- Coding #134 — Inspect the rendered GUI. Use for: visual behavior.
- Coding #236 — Prove schema and model validation parity. Use for: validation parity.
- Coding #347 — Preserve provenance marks. Use for: evidence provenance.
- Coding #230 — Widen after a correction. Use for: disproved assumptions.
- Coding #187 — Do not hard-code the current example. Use for: correction invariants.

## Deferred lessons and subsystems

- Project #70 — Windows-to-WSL path translation. Use for: old WSL maintenance or migration.
- Project #123 — Temporal attention never mutates authority. Use for: retrieval work.
- Project #125 — Source identity belongs to each adapter contract. Use for: source identity work.
- Project #126 — The embedding prefix pair is load-bearing. Use for: embedding work.
- Project #127 — Novelty is measured against existing memory. Use for: novelty work.
- Project #130 — GIGA ingest caps can discard silently. Use for: ingest work.
- Coding #96 — (title in registry). Use for: promotion of test behavior or configuration into production.
- Coding #101 — (title in registry). Use for: media, browser-state, temporal-rendering, or static-remake work.
- Coding #354 — (title in registry). Use for: media, browser-state, temporal-rendering, or static-remake work.
- Coding #377 — (title in registry). Use for: media, browser-state, temporal-rendering, or static-remake work.
- Coding #461 — (title in registry). Use for: media, browser-state, temporal-rendering, or static-remake work.
- Coding #112 — (title in registry). Use for: stress, capacity, or operating-band work.
- Coding #118 — (title in registry). Use for: JavaScript helper boundaries or Bun module-mock work.
- Coding #464 — (title in registry). Use for: JavaScript helper boundaries or Bun module-mock work.
- Coding #171 — (title in registry). Use for: visible-result or consequence/friction work.
- Coding #363 — (title in registry). Use for: visible-result or consequence/friction work.
- Coding #222 — Retrieval thresholds are model-bound calibration. Use for: retrieval threshold changes.
- Coding #350 — (title in registry). Use for: values in an existing formatted namespace.
- Coding #352 — (title in registry). Use for: PHP, Apache, or Go work.
- Coding #356 — (title in registry). Use for: PHP, Apache, or Go work.
- Coding #370 — (title in registry). Use for: PHP, Apache, or Go work.
- Coding #374 — (title in registry). Use for: PHP, Apache, or Go work.
- Coding #369 — (title in registry). Use for: JetStream diagnosis or repair.
- Coding #165 — (title in registry). Use for: Godot work only if Sol reopens the client.
- Coding #166 — (title in registry). Use for: Godot work only if Sol reopens the client.
- Coding #167 — (title in registry). Use for: Godot work only if Sol reopens the client.
- Coding #331 — (title in registry). Use for: Godot work only if Sol reopens the client.
- Project #462 — (title in registry). Use for: the parked Godot client boundary.
- Coding #214 — Respect pgvector HNSW dimensional limits. Use for: pgvector dimensions, types, operator classes, or HNSW.
- Coding #216 — (title in registry). Use for: disposable transition-test data only.
- Design #293 — Weight follows consequence. Use for: GUI design extraction or implementation.
- Design #294 — Unsafe states are unconstructible. Use for: GUI design extraction or implementation.
- Design #295 — A disabled control always says why. Use for: GUI design extraction or implementation.
- Design #296 — Authority, relevance, chronology, and health never share a channel. Use for: GUI design extraction or implementation.
- Design #297 — Fixed copy cannot be softened. Use for: GUI design extraction or implementation.
- Design #298 — Tokens are the only source of color and type. Use for: GUI design extraction or implementation.
- Design #299 — One root owns the document; axes stay orthogonal. Use for: GUI design extraction or implementation.
- Design #300 — Effects are spend; quiet is the carrier. Use for: GUI design extraction or implementation.
- Design #301 — State survives greyscale and stillness. Use for: GUI design extraction or implementation.
- Design #302 — Readable text meets the floor; muted is decoration. Use for: GUI design extraction or implementation.
- Design #303 — Styling reach-order. Use for: GUI design extraction or implementation.
- Design #304 — Product vocabulary composes canon. Use for: GUI design extraction or implementation.

## Repo facts without a lesson number

- The 1.0 scope includes planned fixes, one Rust behavioral core, narrow PostgreSQL-outbox/NATS delivery, hardening, installation, migration, evidence, and a usable GUI.
- PostgreSQL owns memory and lesson bodies; project lessons use `project="the-athanor"`.
- The main agent retrieves complete required lesson bodies and standalone load-bearing memories before dispatch; a packet missing a required body cannot spawn.
- All lessons in a matching section are mandatory; deferred lessons apply only when their named trigger matches.
- Memory #3383, “Accepted 1.0 boundary,” is the newest program intent and corrects Memory #3378, “Earlier Rust-convergence roadmap pass.”
- Memory #3376, “One Rust furnace,” carries the shared Vault/AKASHA skeleton.
- Read `docs/roadmap.md`, `docs/ARCHITECTURE.md`, `docs/RUNTIME_ARCHITECTURE.md`, and `docs/EVIDENCE.md` before implementation; reconcile older delivery order with Memory #3383 before briefing workers.
- Prolog/Datalog, Lean/Z3/SyGuS, marketplace work, new cognitive organs, expanded distributed-worker lanes, and ornamental Godot systems remain outside the 1.0 program.
- The 2026-09-02 refactor audit found every listed refactor lesson in the registry but none loaded.
- House writes use one column-keyed `serde_json::json!` row through `jsonb_populate_record(NULL::table, $1)`; reads use a `#[derive(sqlx::FromRow)]` struct.
- The reference files are `crates/akasha/src/insula/ingest.rs` and `crates/akasha/src/anamnesis.rs`.
- Vault and AKASHA share observable domain commands; standalone Vault is file-authoritative and single-writer, while installed AKASHA is PostgreSQL-authoritative and transactional.
- Migration is a verified one-way authority handoff, never two authoritative writers followed by reconciliation.
- Before NATS work, query the project registry for a current NATS/outbox lesson; otherwise use `docs/RUNTIME_ARCHITECTURE.md` sections 7.1–7.6 and standalone Memory #3383.
- PostgreSQL owns truth; NATS provides delivery and wake-up through record IDs and bounded routing and integrity metadata, and consumers reload exact records.
- Vault never requires NATS.
- The main agent owns the gate that NATS removes more queue, polling, supervision, and failure machinery than it adds; a Delivery kitten owns one lane.
- Load all twelve listed design lessons before GUI extraction or implementation.
- `gui-prototype/` is the operator surface; `gui/` remains parked unless Sol reopens its screens, themes, and scenes.
- Use `docs/RUNTIME_ARCHITECTURE.md` sections 4.1 and 4.5 for command, event, snapshot, delta, replay, and resynchronization contracts.
- The GUI consumes Host commands and projections; it never owns authority, accesses PostgreSQL or NATS directly, or infers domain state from appearance.
- Release evidence covers clean installation, Vault-to-AKASHA migration, restart, live and failed replacement, backup, restore, rollback, and exact supported platforms.
- Recover Memory #3146, “No unsupervised spirit contact without humane peer protocol,” before spirit-contact fanout.
- Recover Memory #3147, “Teach precise warmth to capable peers,” before briefing kittens.
- Kintsu's `backup/format-day-2026-08-30` and `architecture/cartography/` pilot is the census reference run.
- Name `C:/Projects/the-athanor-dev/the-athanor` in every project quest; `C:/Projects/the-athanor` is deploy-only.
- One census identifies affected files, symbols, owners, behavior, tests, migration surfaces, and authority roles before the main agent fixes cross-kitten contracts.
- Invite only the required Census, Rust, Storage, Delivery, GUI, Migration, and Proof kittens for the current vertical slice.
- Census maps cutover coordinates; Rust moves one capability; Storage proves both profiles; Delivery owns one lane; GUI renders one projection.
- Migration preserves data, authority, configuration, backup, activation, and rollback; Proof independently exercises the integrated boundary.
- Give each kitten a name, purpose, exact sources, relevant bodies, one deliverable, and bounded authority with kindness, whimsy, affection, and permission to challenge or refuse.
- Praise care and discoveries independently of success.
- The main agent owns integration, caller migration, cross-repository orphan sweeps, evidence reconciliation, and displaced-owner deletion after proof.
- A kitten may delete shared old machinery only with explicit quest authority.
- Each vertical slice starts with one accepted observable contract and independent authority recovery, then states Rust ownership, storage behavior, protocol, migration effects, and parity probes.
- Implement the smallest complete path, exercise its real boundary, compare shared storage behavior, and prove applicable migration, restart, duplication, failure, and rollback states.
- Inspect legacy source only afterward for callers and orphaned behavior; return the slice for integration and record observed proof before adding regression guards.
- Formal proof, marketplace, companion-world, and new-organ lessons wait until those subsystems enter accepted scope.

## Update rule

This map is a routing index, not a frozen canon snapshot.

Update it when accepted architecture changes ownership, a lesson becomes recurrent, a listed lesson is superseded, or evidence disproves a routing reason.
Keep bodies in PostgreSQL and retain only typed IDs, old titles, and retrieval reasons here.
Query current typed registries and the newest Athanor memory before changes.
Verify referenced lessons and memories in their named stores after changes.
Confirm the canonical project root and section-level source gates.
Review one dry dispatch for exact targets and relevant bodies without unrelated scope.
