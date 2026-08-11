# Changelog

All notable changes to The Athanor are recorded here.

Entries through `0.11.0` were reconstructed on 2026-08-10 from package-version
diffs, commit history, and the dated records under
[`docs/history/`](./docs/history/). Their dates are package-version milestones,
not claims that every milestone had a published release tag. Git history remains
the exact implementation record.

## Maintenance rule

- Every operator-visible behavior, public contract, storage or authority change,
  migration, supported-platform change, and accepted product direction updates
  `Unreleased` in the same commit.
- Entries describe observable impact rather than copying commit subjects.
- `Current`, `Specified`, `Planned`, and `Research` remain distinct claims.
- At release, move the accumulated entries under the released version and date,
  then open a new empty `Unreleased` section.

## [Unreleased]

No changes yet.

## [1.0.0-rc.2] - 2026-08-11

### Fixed

- Added one stable Program Files OMP loader that follows `current.json`, loads
  the active adapter and hygiene extension exactly once, and replaces obsolete
  source/version entries without rewriting unrelated OMP configuration.
- Added an ACL-restricted per-user Host client projection. Tokens remain outside
  `config.yml`; room endpoints are selected from the exact OMP room binding.
- Replaced the invented single `local/home` Host with one supervised Host child
  per configured House room, each using an exact vault directory, identity,
  loopback port, and isolated durable state directory.
- Made external-database first installation back up the existing PostgreSQL
  authority before any migration, and restored it on first-install activation
  failure.
- Made the Windows service follow runtime-plan dependency order, reverse it for
  shutdown, and refuse a child that exits before readiness instead of accepting
  an unrelated listener on the same port.
- Routed the Start-menu Godot client through the native manager so Host token,
  room, spirit, and endpoint travel only in child-process environment.

## [1.0.0-rc.1] - 2026-08-10

### Added

- Established this canonical changelog and linked it from the public and grouped
  documentation indexes.
- Implemented the OMP Session Recall Policy slice with persisted `Auto`,
  `Conversation`, `Work`, and `Quiet` modes, visible requested/resolved state,
  work-immediate/conversation-hysteresis resolution, compaction recovery, and
  localhost administrative controls.
- Added `recall_policy` and `kitten_lineage_status` tools.
- Added typed PostgreSQL canon authority with correction history and active-row
  recall semantics.
- Added Rust-owned Vault retrieval, AKASHA lesson/design/entity/document
  behavior, health, migrations, backup/restore, and sole GIGA claim ownership.
- Added transaction-coupled Paper Boat sleep/wake and `boat.ready` outbox
  migration 16.
- Added NATS 2.14.4 JetStream delivery with bounded pointer payloads, durable
  receipts, dead letters, duplicate windows, and restart replay.
- Added the authenticated Athanor Host with persisted snapshot/delta/resync,
  Recall Policy, idempotency, and sanitized receipt projections.
- Added the Godot 4.7.1 Recall Policy and Paper Boat receipt screens.
- Added the native Rust Windows service supervisor, installer, doctor,
  update/rollback, uninstall, explicit purge, pinned dependency assembly, and
  Inno Setup package.

### Changed

- Added Recall Policy resolution, explicit override, isolation, bounded-context,
  and no-op self-contained-turn proofs to the accepted `1.0.0` evidence path.
- Replaced per-turn accumulation of room, routing, Striatum, context-nudge, and
  automatic Recall additions with stable singletons or replaceable working sets.
- Automatic Recall now runs only on eligible lookup/work/recovery boundaries,
  queries bounded stopword-stripped terms, caches same-topic evidence, refreshes
  after bounded new-conversation growth, injects at most two records, and omits
  raw prompts, missing terms, thread neighbors, taxonomy, and cluster resonance.
  Quiet performs no proactive retrieval.
- Automatic evidence identities are now hard-suppressed after their first
  exposure within a compaction epoch instead of merely receiving a ranking
  penalty. Compaction resets the exposure set for legitimate recovery.
- Consolidated behavioral authority into the Rust workspace; the OMP TypeScript
  surface now performs harness registration, bounded presentation, and Rust
  transport rather than owning domain behavior.
- Replaced the two portable Vault/AKASHA package topology with one native Windows
  x64 installer and runtime-selected authority profiles.
- Raised the database schema contract from 14 to 16.

### Fixed

- Repaired real top-level kitten lineage by listening to the aggregated
  `task:subagent:progress` channel and joining progress to lifecycle through
  `parentToolCallId:index`. Live `LineageGreenFour` completed with zero unmatched
  lifecycles, one committed write, and PostgreSQL memory `#3359`.
- Refused zero-row publish-failure updates so a lost outbox lease cannot be
  dead-lettered by the wrong owner.
- Made Host receipt replay use explicit bounded JetStream pull batches and ack
  every accepted, stale, duplicate, foreign, or refused retained message.
- Preserved the Host-configured session binding on bootstrap snapshots so the
  Godot client can authenticate an unbound initial subscription.
- Made delivery integration proof repeat-safe against shared test-broker traffic
  and extension schemas while checking retained JetStream state rather than raw
  core-subscription publish attempts.

## [0.11.0] - 2026-08-10

### Added

- Unified the core, Rust substrate, OMP adapter, installer, updater, release
  workflows, tests, and canonical documentation into one Athanor repository and
  release boundary (`a051640`).
- Added verbatim braided lesson bodies to bounded worker dispatch packets and
  introduced the kitten-lineage adapter surface (`a051640`).
- Added schema 14 lesson thread keys so any matched lesson can retrieve its
  authority-eligible thread mates (`a051640`).

### Changed

- Expanded root scripts to verify the core, Python, Rust substrate, OMP adapter,
  portable bundle, installer, updater, and clean installation from the unified
  workspace (`a051640`).
- Hardened edit-capable dispatch packets around exact targets, bounded quest
  rendering, sanitized objectives, and lineage metadata (`6640807`).

### Historical correction

- `68fcc2d` changed the listener to raw `task:subagent:event`; that channel exists
  but carries `{id,event}`, not the aggregated assignment required by lineage.
  The progress channel was not nonexistent. The Unreleased fix above restores
  the emitted aggregate channel and uses the shared composite task key.

## [0.10.1] - 2026-08-05

### Added

- Added writing-lesson create/update support and clarified Striatum stage
  transitions (`123029c`, `e31ddbd`).
- Added the design-lesson family, governed design-document organs, and the
  accepted design catalogue (`2be39d5`, `4d5bd55`).
- Made Vault retrieval a first-class, database-service-free profile with native
  attributed local-file retrieval (`6413c54`).

### Changed

- Completed the product and repository naming cutover from Solarisael House to
  The Athanor and moved the package from `0.9.0` to `0.10.1` (`0324156`).
- Exposed the complete operational architecture and its ownership boundaries in
  the public documentation (`4df33ff`).
- Extended the accepted runtime direction across incremental safeguards,
  synthesis, the Godot client, companion ecosystems, Origami delivery,
  slop-resistance, and bounded proof/fabrication work (`1fe9b20`, `8bc1b64`,
  `558fe0c`, `f8d42ad`).
- Moved public comparative benchmarks after `1.0.0`; they remain announcement
  evidence rather than a release blocker (`4d5bd55`).

### Fixed

- Preserved writing registers through `remember` instead of dropping the typed
  writing context (`d8fe0ba`).

## [0.9.0] - 2026-08-05

### Added

- Activated Striatum lesson selection from persistent work state (`44cea9d`).

### Changed

- Brought the public retrieval and evidence map up to the `0.9.0` architecture,
  including the verified Rust-cutover boundary (`ae8165c`).

## [0.8.0] - 2026-07-22

### Added

- Added the typed Rust domain and protocol foundation, followed by typed recall,
  lesson, Anamnesis, and cluster-maintenance protocols (`657b692`, `ec1ab78`,
  `c8d9ebe`, `64966f9`, `a622ee6`).
- Added machine-readable protocol diagnostics and hardened the Rust protocol
  contracts (`ccd6818`, `06dffe2`).
- Added GIGA runtime contracts, safe queue maintenance, and event-ID-only
  processing so workers reload exact trusted sources instead of receiving
  unbounded bodies (`29a7c33`, `a34fc98`, `e37af3a`).
- Added durability-aware recall, ordered memory continuity, authority-aware
  thread retrieval, House-scoped durable remembers, familiar dispatch
  contracts, and canonical typed lessons (`7c6bef3`, `949ab80`, `488066e`,
  `277df46`, `fc6a6ed`).

### Changed

- Promoted Rust-first installation and defined the product direction around the
  Vault and AKASHA profiles (`41ae9eb`, `aa9b507`, `1469b02`).
- Reworked the public explanation around agent continuity, attributed evidence,
  current capability, and explicit limits instead of presenting a generic
  memory plugin (`c358031`, `cbf927e`, `1f7813e`).
- Closed ten of eleven GIGA Hippocampus Stage 1 design decisions while keeping
  candidates non-authoritative until review (`4c9a4ac`).

## [0.1.0] - 2026-06-10

### Added

- Preserved the original Solarisael House plugin as the pre-refactor baseline
  (`84f84cb`).
- Added session rites for wake, Anamnesis, durable writing, sleep, and Paper
  Boats, followed by fail-open automatic wake on the first turn
  (`24ed1a3`, `cad2419`).
- Added shared-core trigger routing, per-room context configuration,
  proprioception nudges, Recall taxonomy, memory-as-navigable-space routing,
  cross-room handles, entity-aware Recall, lesson tooling, guarded lesson
  updates, erasure/archival authority, and a retrieval evaluation harness
  (`3fbf1bf`, `217c37d`, `5215a47`, `6dccd74`, `becf237`, `53b20bd`,
  `7c778f4`).
- Added the public House front door, House guides, AI-led adaptive installation,
  Full-profile starter lessons, and the public substrate contract (`9a51395`,
  `9d994a1`, `50a082c`, `18e8645`, `587e321`).

### Changed

- Split the original plugin into canonical core, OpenCode adapter, and OMP
  adapter repositories with shared ownership boundaries (`3996fe4`).
- Centralized Python substrate configuration and exposed a verified core API
  contract (`463ee44`, `d80d78e`).

### Fixed

- Hardened generic-room keys, runtime validation, room isolation, and substrate
  setup while removing personal assumptions from the public core
  (`ce27865`, `b2ab9ca`, `4660f6e`, `b7cd1e7`, `ae8e45b`).
- Redacted private retrieval evidence and stale private verification references
  from public documentation (`442950b`, `4f9d564`).
