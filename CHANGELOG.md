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

### Added

- Added a durable Godot effects laboratory: four source-labelled alchemical
  treatments, pinned Juicee `1.4.2` orchestration, overlap/cancel/restore
  behavior, accessibility controls, responsive layout, and live renderer
  instrumentation. Decorative output carries no Host authority.

### Changed

- Reframed the Godot shell around the product contract: left room/session
  navigation, center active chat, and right contextual inspection. S01 now
  declares the absent Host chat contract instead of presenting its resume card
  as the final center.
- Added wide, compact, and narrow layout classes without changing the `0.9.5`
  version. Sidebars and nonessential chrome collapse around the center, Recall
  columns and state grids adapt, and action rows wrap.
- Replaced grayscale 2× font rasterization with Godot LCD antialiasing, normal
  hinting, automatic subpixel placement, and native/default oversampling for
  small desktop UI text.
- Consolidated the complete Git repository under `C:\Projects\the-athanor` and
  placed the Godot client under `gui/`. The obsolete Obsidian copy and nested
  `clients/godot` project no longer exist.
- Disabled VSync for every Godot game window. Frame rate remains uncapped unless
  an explicit measurement or operator-selected limiter is applied.
- Replaced the flat global screen strips with reusable nested reliquary
  navigation. Back restores trigger focus, Escape returns one pane before
  closing drawers, inactive panes are hidden, and settings uses the same stack.
- Restored the desktop right context column, collapsed the bottom dock to a
  42 px status rail, and made context/navigation exclusive overlay drawers
  below their docked breakpoints.
- Recovered the exact S01–S14/R01 inventory from `The Athanor Design Restart.zip`.
  Screen headers now use only the archive’s enabled `OrnamentFrame` rule:
  `corners={false}`, `sigils={false}`, exact divider flourish, bar, and mirror.
- Included the new `gui/navigation` runtime resources in native release staging.

### Fixed
- Removed invented box-drawing corners and the inferred four-corner side-panel
  treatment. The GUI no longer fills gaps in the extracted design with new
  ornament vocabulary.

- Moved S01, S02, and S03 under one center vertical `ScrollContainer`, removed
  the nested routing scroller, enabled focus-follow scrolling, and allowed long
  Paper Boat receipts to wrap at narrow widths.
- Validate Context Host analyses where they enter the OMP adapter. A missing
  query route now reports one precise Context Host degradation and skips
  automatic Recall instead of emitting a second misleading Recall Policy error.

## [0.9.5] - 2026-08-13

### Added

- Established the Chargebook as a canonical House practice and shipped the OMP starter room with an always-loaded default covering positive credits, zero-cost presence, behavioral charges, and operation costs.
- Added the third functional Godot operator screen: authenticated, read-only
  worker-lane and advisor status from the existing Host routing contract, with
  exact event/sequence receipts and no dispatch capability.

### Changed
- Sharpened native Godot text by disabling viewport stretch scaling, enabling
  high-DPI rendering, and importing bundled fonts with normal hinting,
  automatic subpixel placement, and 2× oversampling.
- Cut all operator-visible Godot copy and client-side diagnostics to English as
  the single source language until a deliberate localization pipeline exists.

- Kept the product on the `0.9.x` late-beta line after the premature
  `1.0.0-rc.3` label. The proven RC1-RC3 build identities remain below as
  historical artifact evidence; they do not claim that the operator product
  reached 1.0.
- Restored the 1.0 gate around a usable operator control surface, healthy
  continuity organs, clean installation and upgrade/rollback proof, signing,
  and public release evidence.
- Completed the first clean Rust ownership cut for the OMP runtime: prompt
  classification, keyword/process triggers, context-pressure nudges, Recall
  viewport selection and saturation, worker/familiar dispatch construction,
  spellbook validation, and subagent lineage normalization now execute in typed
  Rust behind Athanor Host commands.
- Deleted the root TypeScript behavioral core and stopped packaging `src/*.ts`.
  The OMP extension now retains harness registration, event normalization,
  transport, room-local file access, and bounded presentation rather than a
  second policy implementation.
- Reduced OMP hygiene to two harness-local guards: scratch-shaped writes in
  tracked trees and post-exec nudges for bulk staging or forceful deletion.
- Made the native release builder use the operating-system temporary directory
  when CI-only `RUNNER_TEMP` is absent, allowing local release builds from a
  standard PowerShell session.
- Replaced per-screen Godot WebSockets with one root-owned authenticated Host
  session shared by Recall Policy, Paper Boat receipts, and worker-lane status.

### Fixed

- Made Rust Host conversation capture persist the exact private JSONL source
  ledger before GIGA queues an event. GIGA workers now resolve source pointers
  from the room-local runtime instead of the retired legacy home-directory
  ledger, preventing fresh `GigaSourceMissingError` failures after the Rust
  ownership cut.
- Namespaced OMP Host retry identities by command type and the harness session
  manager's stable session ID. Concurrent subagents can now reuse inherited
  message IDs without colliding on different Recall Policy command bodies.
- Made delivery readiness process-owned and deterministic. The long-running
  worker now publishes a fresh atomic ready marker after PostgreSQL and NATS
  connect; the supervisor removes stale markers before spawn instead of
  launching repeated one-shot NATS health clients.
- Drained one-shot delivery NATS clients before process exit, preventing native
  update readiness probes from remaining alive indefinitely.
- Kept durable Recall Policy receipts readable across the snake_case to
  camelCase wire cutover while continuing to serialize the current camelCase
  contract.

## [1.0.0-rc.3] - 2026-08-11

### Fixed

- Staged the root TypeScript core entry and every runtime TypeScript dependency
  required by the installed OMP adapter.
- Added a release-build smoke that executes `loadHouseCore()` through the
  packaged adapter bridge, preventing a loader that registers successfully but
  fails when OMP first resolves the core module.

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
