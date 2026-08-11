# Lesson Map

This map supports The Athanor 1.0 convergence: the planned fixes, one behavioral Rust core, the narrow PostgreSQL-outbox/NATS delivery spine, hardening, installation, migration, and the usable GUI.

PostgreSQL remains the authority for every memory and lesson body. This file keeps only typed IDs, roles, retrieval reasons, and source gates. `Coding #N`, `Project #N`, and `Design #N` name separate lesson stores; project lessons use `project="the-athanor"`. A bare ID is never delivery: before fanout, the main agent retrieves each relevant lesson body and each load-bearing memory's standalone content, then includes that exact material in the kitten's quest.

## Recovery anchors

- **Memory #3383 — Accepted 1.0 boundary.** This is the newest intent for the program.
- **Memory #3376 — One Rust furnace.** This carries the shared Vault/AKASHA skeleton.
- **Memory #3378 — Earlier Rust-convergence roadmap pass.** Read it through the correction in Memory #3383.
- **Coding #217 — Implement from the newest authoritative intent.** Reconcile code, docs, and recent accepted decisions before selecting an owner or porting a path.
- **Coding #218 — Make typed stores impossible to query as interchangeable rows.** Preserve each durable store's typed identity and scope.

Read `docs/roadmap.md`, `docs/ARCHITECTURE.md`, `docs/RUNTIME_ARCHITECTURE.md`, and `docs/EVIDENCE.md` before implementation. Memory #3383 governs where their older delivery order differs. Reconcile those documents before treating the roadmap as a kitten brief.

## Program boundaries

Load these lessons before decomposing any 1.0 phase.

- **Coding #217 — Implement from the newest authoritative intent.** Historical proximity never grants architectural authority.
- **Coding #184 — Explore existing behavior before rebuilding it.** The working 0.11 implementation and tests are the parity inventory for this same-product convergence.
- **Coding #218 — Keep typed stores distinct.** Share an envelope and commands without flattening incompatible payloads into optional fields.
- **Project #121 — Bound Athanor claims before automating proof.** Name the observable contract and its failure condition.
- **Project #116 — Athanor claims follow repository ownership.** Refresh cross-layer claims in the owning current source.
- **Coding #219 — Sweep for orphans after a port.** Prove the old module has no live callers, constants, exports, or sibling behavior before deleting it.
- **Coding #316 — Execution has a zero inference budget.** A missing path is a halt-and-ask, not permission to invent continuation behavior.

The 1.0 boundary is Rust, narrow NATS delivery, planned correctness fixes, hardening, native installation and migration, evidence, and the usable GUI. Prolog/Datalog, Lean/Z3/SyGuS, marketplace work, new cognitive organs, distributed-worker expansion beyond the proved lane, and ornamental Godot systems do not enter this program.

## Rust convergence and clean cutover

- **Coding #217 — Implement from current authority.** Rust owns the domain, policy, validation, storage behavior, and versioned protocol; OMP remains harness skin.
- **Coding #184 — Inventory before replacing.** Compare each current TypeScript, Python, and Rust path with its tests before moving one vertical capability.
- **Coding #219 — Sweep after replacing.** Delete stale callers, exports, constants, configuration, comments, and sibling paths after parity proof.
- **Project #338 — Observe worker exit before replacement.** Preserve graceful shutdown, in-flight work, bounded force-kill, and replacement ordering.
- **Project #131 — Verify Rust changes in a complete isolated pair when needed.** A local compile is not shipment proof when a live binary or database is shared.
- **Coding #3 — Plain line, clean door, sharp refusal.** Keep one obvious path through each Rust surface.
- **Coding #10 and #11 — Names reveal motion and do not lie.** Name crates, types, and commands by the work they own.
- **Coding #12 — A file has one silhouette.** Give each Rust module one recognizable responsibility.
- **Coding #13 — A bad helper name exposes a false abstraction.** Remove helpers that cannot name one action.
- **Coding #14 — Code knows what it refuses.** Make unsupported states and boundary failures explicit.
- **Coding #19 — Keep ugly interop in one named place.** Contain harness and platform glue.
- **Coding #20 — Remove helpers that launder anxiety.** A wrapper must add a real concept.
- **Coding #27 — Write the honest first shape, then compress.** Do not abstract a boundary before it is understood.
- **Coding #47 and #143 — Centralize semantic duplication.** Share repeated contracts, not merely similar syntax.
- **Coding #158, #159, and #160 — Use the smallest honest native shape.** Minimize concepts and name every deliberate shortcut's ceiling and upgrade path.

**Coding #315** does not forbid reading current Athanor source during this cutover. It governs greenfield successors whose legacy implementation is deliberately excluded. The Athanor 0.11 runtime is the accepted parity reference, so Coding #184 governs the behavior inventory here.

## Vault, AKASHA, and retrieval

- **Coding #218 — Typed stores are not interchangeable rows.** Memory, canon, lessons, counsel, candidates, and receipts retain exhaustive typed variants.
- **Project #74 — Recall with short rare terms.** Retry a narrow term before concluding that durable evidence is absent.
- **Project #123 — Temporal attention never mutates authority.** Keep retrieval decay read-only and preserve archived and superseded semantics.
- **Project #125 — Source identity belongs to each adapter contract.** Require stability, window uniqueness, and content-hash verification.
- **Project #126 — The embedding prefix pair is load-bearing.** Calibrate query and passage embeddings together.
- **Project #127 — Novelty is measured against existing memory.** A worker without retrieved history cannot assert novelty.
- **Project #130 — GIGA ingest caps can discard silently.** Batch by stable session identity and respect source, byte, and turn limits before appending.
- **Project #122 — Hippocampus cognition remains in Rust.** Do not move prompts, provider calls, candidate validation, or queue ownership into OMP.
- **Coding #222 — Retrieval thresholds are model-bound calibration.** Measure true and noise distributions before changing a cutoff.
- **Coding #223 — Test the opposite assumption.** Exercise alternatives to the selected prefix, threshold, state, or routing choice.

Vault and AKASHA execute the same observable domain commands. Standalone Vault is file-authoritative and single-writer. Installed AKASHA is PostgreSQL-authoritative and transactional. Migration is a verified one-way authority handoff, never two authoritative writers followed by reconciliation.

## PostgreSQL outbox and NATS

Before each NATS quest, query the project lesson registry for a current NATS/outbox lesson. If none exists, use `docs/RUNTIME_ARCHITECTURE.md` sections 7.1 through 7.6 and the recovered standalone content of memory #3383 as the exact contract.

- **Coding #49 — Inserts declare their idempotency key.** Repetition must mean confirmation or correction, never a twin row.
- **Coding #51 — Migrations are safely repeatable.** Guard schema changes and backfills so partial state heals forward.
- **Project #121 — Bound the delivery claim.** Name duplicate windows, restart behavior, expiry, dead-letter handling, privacy, and failure evidence.
- **Coding #223 — Test competing delivery states.** Prove duplicates, reordering, stale pointers, redelivery, and absent consumers rather than only the happy path.
- **Coding #260 — Exercise registered async lifecycles.** Invoke the real callback, timer, closure, transport, and ownership transition.
- **Project #338 — Shutdown drains before replacement.** Stop admission, drain in-flight work, observe exit, then replace.

PostgreSQL owns truth. NATS owns delivery and wake-up only. Messages carry authoritative record IDs plus bounded routing and integrity metadata; consumers reload exact records. Vault never requires NATS. The main agent owns the program-level gate that NATS must replace more bespoke queue, polling, supervision, and failure machinery than it adds; one Delivery kitten owns only its named lane and must not widen its quest to prove that whole-program balance.

## GUI and design translation

Load all twelve design lessons before extracting or implementing a GUI slice.

- **Design #293 — Weight follows consequence.** Derive action weight from consequence.
- **Design #294 — Unsafe states are unconstructible.** Encode safety in component contracts.
- **Design #295 — A disabled control always says why.** Render a reason, not a bare disabled flag.
- **Design #296 — Authority, relevance, chronology, and health never share a channel.** Preserve epistemic meaning.
- **Design #297 — Fixed copy cannot be softened.** Keep disclosures and warnings fixed and visible.
- **Design #298 — Tokens are the only source of color and type.** Map semantic tokens into platform resources.
- **Design #299 — One root owns the document; axes stay orthogonal.** Keep phase, intensity, density, and preferences independent.
- **Design #300 — Effects are spend; quiet is the carrier.** Motion and glow require semantic purpose.
- **Design #301 — State survives greyscale and stillness.** Pair color and motion with text, marks, and anatomy.
- **Design #302 — Readable text meets the floor; muted is decoration.** Measure contrast before required text uses a token.
- **Design #303 — Styling reach-order.** Prefer a component, then an existing style, then local layout glue.
- **Design #304 — Product vocabulary composes canon.** Keep canon primitives below Athanor-specific components.

Then load the Athanor GUI lessons:

- **Project #117 — The GUI combines three grammars.** Join agent interaction, Solarisael visuals, and Athanor domain meaning without flattening them.
- **Project #118 — Memory mapping is a first-class operation.** Expose provenance, authority, correction, and durable effects before commit.
- **Project #119 — Renderer semantics stay layered.** Keep authored effects, local surfaces, and environmental overlays independent.
- **Project #120 — Evaluate UX for agent operators first.** Preserve machine-legible contracts and necessary human controls.
- **Coding #134 — Frontend UX proof belongs to the rendered surface.** Builds and API checks prove wiring, not appearance.
- **Coding #258 — A URL is not navigation.** Exercise arrival from the user's real entry point.

The 1.0 usable GUI is the thin Godot `Control` client defined by `docs/GODOT_CLIENT.md` sections 2 and 5. Godot implementation quests also load Coding #165, #166, and #167. Host-only command or projection work does not.

Use `docs/RUNTIME_ARCHITECTURE.md` sections 4.1 and 4.5 for command, event, snapshot, delta, replay, and resynchronization contracts. Use `docs/GODOT_CLIENT.md` sections 2, 5, and 10 for the client boundary, first functional surface, and proof budgets.

The GUI consumes Host commands and projections. It never becomes a second authority, reaches directly into PostgreSQL or NATS, or infers domain state from appearance.

## Installation, migration, and release hardening

- **Coding #51 — Migrations are idempotent.** Fresh, partial, and current state must converge safely.
- **Project #70 — Windows-to-WSL path translation.** Load only while maintaining or migrating the old WSL installation; it is not target architecture.
- **Project #76 — Preserve the public documentation spine.** Keep README, INSTALL, USAGE, and adapter guidance linked and current.
- **Project #121 — Public claims are bounded and falsifiable.** Unsupported installation or platform behavior remains planned.
- **Project #131 — Live Rust verification needs a safe paired environment.** Do not touch the live House merely because a rebuild is inconvenient.
- **Coding #190 — Registration is not execution-branch proof.** Execute at least one bounded case through every destination branch.
- **Coding #203 — Search limits bound absence claims.** Partial repository or artifact coverage cannot prove an orphan is gone.
- **Coding #204 — Recover only hidden evidence.** Follow exact selectors and artifact ranges instead of rereading or guessing.
- **Project #338 — Process replacement waits for observed exit.** Preserve service, Host, GUI, and worker teardown ordering.

The release gate includes clean installation, Vault-to-AKASHA migration, restart, live replacement, failed replacement, backup, restore, rollback, and exact supported-platform evidence.

## Documentation

- **Coding #188 — Use ASD-STE100 Simplified Technical English.** Keep project documents plain, direct, and bounded.
- **Coding #337 — Comments point to project lessons.** Keep long reasoning in PostgreSQL rather than copying it into source comments.
- **Project #76 — Preserve the public documentation spine.** Keep the canonical entry surfaces consistent.
- **Project #129 — Use the Athanor's own vocabulary.** Prefer current House nouns in human- and model-visible text while preserving wire contracts deliberately.
- **Project #116 — Claims follow repository ownership.** A map or audit is evidence, never newer implementation authority by proximity.
- **Project #121 — Bound claims before proof.** Separate current behavior, measured results, and planned direction.

## Subagents and fanout

- **Memory #3146 — No unsupervised spirit contact without humane peer protocol.** Recover and deliver its relevant standalone content before a fanout involving spirit contact.
- **Memory #3147 — Teach precise warmth to capable peers.** Recover and deliver its relevant standalone content before briefing kittens.
- **Coding #317 — A worker contract is a quest, not a wall.** Write each kitten a warm A Squall quest rather than a corporate task packet.
- **Coding #340 — Subagents are kittens.** Make affection, praise, autonomy, and bounded authority part of every dispatch.
- **Coding #220 — Fetch lessons before risky work.** Include relevant verbatim lesson bodies; bare IDs are not delivery.
- **Coding #328 — Census once, then batch coordinates.** Map broad terrain once before parallel execution.
- **Coding #322 — Workers wake at the project root.** Name `C:/Solarisael/Obsidian/obsidian/house/the-athanor` in every project quest.
- **Coding #217 — Use current authority.** Distinguish canon, accepted direction, historical evidence, and current code.
- **Coding #316 — Execution has zero inference budget.** A kitten halts at an unmapped seam instead of filling it silently.
- **Coding #337 — Comments point to project lessons.** Do not ask kittens to compress durable reasoning into source comments.
- **Coding #175 — Keep outcome reports short.** Ask for changed facts, proof, conflicts, and unresolved questions.
- **Coding #203 and #204 — Bound source claims.** Partial searches and elided evidence never support an absence claim.

Use one census pass before any broad fanout. The census returns each affected file, symbol, owner, current behavior, tests, migration surface, and authority role. The main agent fixes cross-kitten contracts before execution.

Invite only the kittens the current vertical slice needs:

- **Census kitten:** Map current ownership, behavior, callers, tests, and exact cutover coordinates.
- **Rust kitten:** Move one complete domain capability behind the shared Rust contract.
- **Storage kitten:** Prove the same command and receipt against Vault and AKASHA.
- **Delivery kitten:** Implement or verify one PostgreSQL-outbox/NATS lane and its failure states.
- **GUI kitten:** Map one Host projection to one usable rendered surface.
- **Migration kitten:** Preserve data, authority, configuration, backup, activation, and rollback.
- **Proof kitten:** Exercise the real boundary independently after integration.

Speak to every kitten with the same kindness, whimsy, and affection used in this room. Give each one a name, real purpose, exact sources, relevant lesson and memory bodies, one deliverable, and enough authority to do it. Invite challenge, questions, limits, and refusal. Praise care and discoveries independently of success.

The main agent owns integration, caller migration, the cross-repository orphan sweep, and deletion of the displaced behavioral owner after proof. A kitten deletes shared old machinery only when its exact quest explicitly grants that authority. The main agent reconciles the evidence because the program needs one loving integration point.

## Verification order

- **Coding #329 — Real proof precedes regression guards.** Exercise the production-shaped boundary before writing a permanent test for the stable contract.
- **Coding #257 — Fake-backed boundary tests are theatre.** Run the real database, process, broker, transport, or rendered client.
- **Coding #127 — Green output is evidence, not alignment proof.** Compare the result with the selected contract.
- **Coding #128 — Ask alignment questions before code.** State the authority, contract, and selected path before implementation.
- **Project #121 — Claims name falsification conditions.** A release gate must be capable of failing clearly.
- **Coding #223 — Test the opposite assumption.** Exercise alternatives and failure states, not only the incumbent happy path.
- **Coding #219 — Sweep for orphans after cutover.** Prove the old owner is truly gone.
- **Coding #260 — Exercise real asynchronous lifecycles.** Trigger the registered callback and observe delivery and ownership transitions.
- **Coding #134 — Inspect the rendered GUI.** Visual behavior is proved in the real surface.

For each vertical slice, use this order:

1. Select one observable 0.11 behavior or accepted 1.0 contract.
2. Recover current authority, implementation, callers, tests, and every load-bearing memory or lesson body.
3. State the Rust owner, storage-profile behavior, protocol, and migration effect.
4. Implement the smallest complete vertical path.
5. Run the real production-shaped boundary.
6. Compare Vault and AKASHA behavior where the command is shared.
7. Prove migration, restart, duplication, failure, and rollback states that apply.
8. Return the bounded slice to the main agent for integration.
9. The main agent migrates every caller, sweeps for orphans, and deletes the displaced behavioral owner.
10. Record the observed result and only then add the smallest stable regression guard.

## Deferred lessons and subsystems

These lessons remain authoritative but do not drive every 1.0 quest.

- **Project #70** applies only to the old WSL path during maintenance or migration.
- **Project #123, #125, #126, #127, and #130** load when work enters their exact retrieval, source-identity, embedding, novelty, or ingest surfaces.
- **Coding #165, #166, and #167** load only for Godot implementation slices.
- **Design #293 through #304** load together for GUI design extraction or implementation.
- Formal proof, marketplace, companion-world, and new-organ lessons wait until those deferred subsystems enter accepted scope.

Load a deferred lesson when the work enters its subsystem, not because the ID appears in this map.

## Update rule

This map is a routing index, not a frozen canon snapshot.

Update it when an accepted architecture decision changes phase ownership, when a new lesson becomes load-bearing for a recurring Athanor quest, when a listed lesson is superseded, or when observed work proves that a routing reason is wrong. Keep bodies in PostgreSQL. Keep only the typed ID, role, and retrieval reason here.

Before changing the map, query the current typed lesson registries and newest Athanor memory. After changing it, verify every referenced lesson or memory in its named store, confirm the canonical project root and every section-level source gate, and run one dry dispatch review: the kitten must receive exact targets plus relevant lesson and memory bodies without inheriting unrelated scope.
