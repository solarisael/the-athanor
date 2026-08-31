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

## [0.5.0] - 2026-08-31

### Added

- `athanor.exe` is the canonical desktop owner. It keeps the existing Godot GUI
  and every managed harness process alive under one application lifetime.
- A strict registry describes each harness program, workspace, console mode,
  and driver. The first drivers are a plain process and OMP.
- The new Harnesses screen lists process state and sends Start, Stop, and
  Restart requests to the owner through an authenticated loopback socket.
- OMP runs as a direct child of `athanor.exe`. The existing `request_restart`
  signal now resumes OMP without an `omp-keeper.exe` sidecar process.
- The House now records a restart intent. A session asks for a harness restart.
  The adapter arms the exit. Athanor claims the exit and relaunches OMP. The
  successor session proves the return.
- Schema `restart` (migration 26) holds the intents and an append-only event
  ledger. Five substrate methods carry the plane: `restart_request`,
  `restart_claim`, `restart_transition`, `restart_verify`, and `restart_status`.
  `restart_status` is a read. It needs no capability.
- One workspace carries one live restart intent. The schema refuses a second
  live row, and `restart_request` refuses with the code `intent_pending` before
  it makes one. Athanor reads the newest live intent for a workspace, so a
  second intent cannot stand in for the successor.
- `restart_status` answers two questions. Without an intent id it reports the
  pending intent for the workspace, as before. With an intent id it reports that
  one intent in every state, including terminal states. Athanor can therefore
  confirm that its own successor reached `verified`.
  The workspace still scopes the read; an id from another workspace reports nothing.
- `restart_request` refuses with the code `restart_storm` after three intents
  reach the exit stage for one workspace in one hour. The cap now applies when
  an intent reaches the exit stage, not only when a session asks, so requests
  made before the hour filled cannot all arm. An unclaimed request expires after
  300 seconds. An expired intent never fires, and an expired request refuses a
  replay of its own idempotency key instead of answering with a live state.
- Each restart door proves its authority with a provisioned secret, because
  `restart_status` gives the intent id to any caller. The room holds three
  secrets: `restart_request` to ask, `restart_exit` to arm the exit, and
  `restart_verify` to sign the successor. Athanor holds `restart_claim`. The
  exit also names the harness session that asked, and the substrate compares it
  with the stored value. New refusal codes: `restart_capability`,
  `exit_not_authorized`, and `verify_not_authorized`. Provision every secret
  with `substrate/provision-restart-capability.ps1`.
- Schema migration 27 adds one hash-only successor proof for the current
  relaunch attempt. Athanor receives a fresh proof from each successful
  `relaunching` transition and passes it only to that child. This lets `resume`
  preserve the logical OMP session id while PostgreSQL still distinguishes the
  new process from its predecessor. Retries rotate the proof; verified and
  failed intents delete it. Verification after the House deadline refuses with
  `verify_expired`.
- The authenticated Host read surface gains one route:
  `/athanor/v1/insula/unverified-exit`. It reports sessions that armed an exit
  and never returned verified inside the stage window. The route reports only
  the room in the Host configuration, so one bearer never reads another room's
  workspace path or session. The route reads only and commands nothing.
- Athanor owns the OMP process and console. Its internal OMP driver checks every
  child exit against `restart_status`; exit code 87 is only a fast signal. The
  driver claims a pending intent, starts `omp --resume <sessionId>`, waits for
  successor verification, and retries one failed start. The standalone
  `omp-keeper` binary remains compatible during migration to the canonical app.
- The OMP adapter now exposes `request_restart`, the exit door for its own
  session. The tool records the intent itself when the House holds none. It then
  arms that intent in the same call. The room must hold the `restart_request`
  secret to ask, and the `restart_exit` secret to arm. An unprovisioned room
  cannot restart itself, and the refusal says so. A claimed intent refuses,
  because only a requested intent may exit. A second arm refuses with
  `already_armed` while one exit waits. A `fresh` restart refuses with
  `fresh_without_boat` when the room holds no paper boat. The tool reads the
  boat and never consumes it. Before it arms, the tool reports what dies with
  the exit: this session's async jobs, buffered GIGA turns, and every open
  substrate transport. It names each casualty class it cannot see, and it claims
  nothing about that class. The armed exit fires at `agent_end`, never inside
  a turn. It moves the intent to `exiting`, then leaves OMP with code 87 for
  the Athanor owner. A refused transition keeps the session alive and names
  the substrate code.
- The installed loader now hands the resolved release to the adapter entry. The
  arm report names that loaded release, so a session can show `installed` against
  `loaded`.
- The adapter holds no keeper claim token, and it needs none. The `requested` to
  `exiting` transition carries the room's `restart_exit` secret and the door's
  own harness session identity. The intent id proves nothing, because
  `restart_status` gives it to any caller. The adapter's account of the exit
  rides as JSON in `detail`. The door measures the serialized payload against
  the 2048-byte ceiling, so JSON escaping cannot carry the payload over it.

### Known ceilings

- The adapter reports this session's async jobs from the harness snapshot. It
  cannot list hub processes, because the plugin surface exposes no process
  listing. The arm report names that class and claims nothing about it.
- An intent requested by another session still arms. The exit then learns
  `exit_not_authorized` at `agent_end`. A session-owner flag on `restart_status`
  is the way up. Today `restart_status` needs no capability, so it must not
  carry a live session identity.

### Fixed

- GIGA conversation ingest was dead in production. `giga.ts` called
  `path.dirname` without importing `path`. The `ReferenceError` fell into a
  `catch` that answers "this is a subagent session". Every session therefore
  looked like a subagent, and the adapter buffered no turn. The break started at
  `b635264`. Turn ingest resumes with this release. Set
  `SOLARISAEL_GIGA_ENABLED=0` to hold it off if the queue load surprises you.
  Kintsu's demand for a production-seam proof found this. The restart door's new
  seam test showed it.

### Changed

- The product version resets from `0.9.7.2` to `0.5.0`. The new line states the product's current maturity without erasing earlier evidence.
- The `0.9.*` records remain historical implementation receipts. They do not claim that one-owner startup, one-door room routing, or the web operator surface is complete.
- Windows OMP writes now allow five minutes for durable PostgreSQL work and backups. Other platforms keep the 90-second limit.

- The OMP adapter no longer injects lesson packets, process reminders, or asynchronous Athanor corrections. Use explicit retrieval for ordinary lessons.
- Lessons tagged `ttsr-approved` now register as versioned rules in OMP’s native `TtsrManager`. OMP owns matching, interruption, retry, and reminders.
- Trigger authoring fields remain in `remember` and `update_lesson`. Untagged trigger rows stay dormant.

- `update_lesson` now accepts the routing eligibility fields for writing and
  design lessons: both kinds take `alwaysOn`, and writing lessons additionally
  take `languageKeys`/`technologyKeys` (design already accepted keys). The
  per-kind gate itself remains: cross-store fields such as coding `scope` still
  refuse for writing/design, proven by a live-database update proof.
- Summoning now owns the Anamnesis and Paper Boat domain vocabulary.
- Presence now opens one Host-owned session frame and compiles one cited contract for each user turn.
  The OMP adapter injects the contract, settles a response receipt, and feeds close material to the existing Paper Boat writer.
- The authenticated Host read surface now carries the House data needed by
  operator panels: room-bound Docket board and evidence, Hallway inbox,
  newest-first memory and lesson timelines, full memory reads, and bounded
  WebSocket AKASHA recall and typed lesson queries. These remain read-only and
  inherit the Host bearer and room-binding fences rather than gaining panel
  authority.
- The browser interaction prototype now uses live House data as its primary
  working surface. House Overview auto-reads quests, Hallway attention, and
  collapsed evidence drawers; Memories & Lessons reads PostgreSQL timelines,
  opens full memory bodies, paginates both shelves with their exact keyset
  cursors, and keeps empty, refused, malformed, and fixture fallback states
  visibly distinct.

- Managed runtime startup now tears down every already-started child when the
  next child exits before readiness, preventing a transient delivery failure
  from leaving Windows SCM stuck in `START_PENDING` with an orphaned NATS
  process.
- Automatic session wake now gives the cold paper-boat read a dedicated
  15-second budget instead of the shared 2-second context budget. A failed
  receipt produces a bounded visible warning rather than silently omitting
  previous-session continuity.
- Project lessons now fire inside their own project: `lesson_trigger_match`
  accepts a caller `project` slug (the adapter derives it from the git root
  basename), and the fence admits project-tagged lessons on a normalized
  exact match while keeping NULL-project lessons universal. The language
  fence table now speaks the registry's full `language_keys` vocabulary
  (go, css, html, gdscript, glsl, markdown, php, lean beside the five ast
  grammars), so keyed lessons in any of those languages stay armable.
- Lesson triggers now fence by language: a lesson carrying `language_keys`
  fires only on tool surfaces whose file extension maps to one of its
  languages (one authority table shared with ast-grep grammar selection;
  `.sql` stays regex-capable). Unkeyed lessons behave exactly as before, and
  a lesson that watches `text` while carrying language keys compiles with a
  named warning instead of silently never firing on prose.
- Trigger fires now carry `surfaceIndex` and `matchStart` (regex byte offset;
  null for ast hits) on the wire, and a prose-lane block interrupt keeps the
  clean draft up to the sentence boundary before the violation instead of
  discarding the whole aborted message. The correction counsel instructs the
  model to continue exactly from the cut, so only the violating tail is
  regenerated.
- Automatic Recall now recognizes work by evidence, not only vocabulary: the
  first `edit`/`write` tool call of a session rides the policy evaluation as
  `tool_evidence`, and auto mode resolves to `work` (reason `tool-evidence`)
  beneath technical intent and explicit lookups. Evidence holds for the
  session's lifetime.
- Work mode now injects a lesson packet: when the resolved Recall mode is
  `work` and the conversation carries none, the always-on coding lessons
  (`lesson_query` gains an `alwaysOn` filter) arrive as one superseding
  `solarisael-lesson-packet` message with an operator-visible card. Curating
  the packet is a data operation: flip `always_on` on the lesson.
- Insula now observes the Rust processes, not only the OMP adapter. A shared
  in-process emitter (`athanor_substrate::insula_writer`) mirrors the adapter
  writer contract: bounded queue, drop receipts, monotonic writer sequence,
  body-free observations, fire-and-forget. The substrate observes every
  protocol method (39 operations), refused decodes, and PostgreSQL backups;
  each room Host observes Knock claims and settlements, Hallway and receipt
  projections, recall policy decisions, and its own Insula HTTP handling.
  System-scope work binds room `house`, spirit `Athanor`.
- The Host now serves two authenticated Insula reads beside Vitals:
  `/athanor/v1/insula/trace` (room-scoped causal trace drilldown) and
  `/athanor/v1/insula/retention` (house-wide retention receipts with
  tombstone coverage sums).
- The substrate service now runs the proven replay-safe Insula retention
  sweep on a fixed daily cadence, five minutes after boot, with a receipt
  point per real deletion. This is an idempotent maintenance loop, not a
  monitor.
- The OMP adapter now records a result point for every completed tool call
  (previously only orphan results were pointed) and binds the provider's own
  request id to provider request settlements and usage points.
- The OMP adapter now records provider request spans and normalized token usage.
  The new `/insula` command shows bounded Vitals for 15 minutes, 1 hour, or
  24 hours. It reports missing usage and unavailable Host data without false
  zero values.
- Guarded local native deployment now runs every required Cargo test package in
  the reusable `target/deploy` release profile before building its staged
  binaries, avoiding separate default-profile compilation during hot deploy.
  It refuses a transitional native-service state before test or backup work.
  The same guarded transaction now stages, manifests, replaces, and rolls back
  both installed copies of `athanor-manage.exe`, so local deployment can carry
  the current installer manager without a product-version change.
- Native release builds now hold one exclusive dependency-cache lock across
  cache selection, production, publication, use, and pruning, so concurrent
  builds serialize instead of racing. Once the requested cache is verified
  complete, superseded keyed caches and abandoned pending directories are
  removed, so repeated dependency-key changes no longer accumulate one full
  runtime payload per key. An incomplete cache never removes a working one, a
  cache another build finished first is reused rather than rebuilt, and a cache
  root, cache directory, or required artifact reached through a junction or
  symlink is refused rather than trusted.
- Successful native and OMP adapter activations now prune historical installed
  releases down to the pointer-addressable current and previous pair. Failed
  activations do not prune rollback state, and cleanup failures leave durable
  activation pointers unchanged.
- Adapter-only install and rollback now validate the active native manifest's
  identity and compatibility without rereading or hashing unrelated native
  payload artifacts. Component source and retained releases remain fully
  integrity-checked.
- The Host now provides an authenticated, Host-scoped Insula Vitals query.
  The GUI prototype shows the Insula panel in House Mechanics. The disconnected
  panel states that live Vitals, traces, and loss data are unavailable. It also
  states that the retention runner is not scheduled.
- Hallway unread and Bell counts now exclude messages authored by the observing
  room without advancing its stable read position. Posting no longer rings a
  room about its own voice, while older unread messages from peer rooms remain
  pending until an exact read covers them.
- Hallway Knock delivery now settles the parent turn as soon as OMP accepts its
  trusted injection, before the model can answer with a child Knock. Allowed
  Knocks claim active sessions as well as idle ones and interrupt the current
  turn only after that authoritative start settlement succeeds.
  Knock Host commands use a 10-second bounded deadline, and failed claims back
  off from 5 to 60 seconds instead of hammering the Host every poll interval.
  If OMP omits custom `message_start` metadata, the doorman completes an idle
  Knock from its first `agent_end`; an active interruption consumes the aborted
  predecessor's end and assigns the following end to the Knock.
- The native release builder now gives an absent `VCToolsVersion` a stable cache
  identity instead of passing an empty mandatory cache-key segment.
- Native release assembly now derives its product version only from the root
  package, pins Rust 1.95.0, refuses missing or foreign MSVC tools before
  dependency work, and retains machine-readable timing for each build and
  packaging stage.
- The installed OMP adapter is now an independent, integrity-checked component
  with its own atomic pointer, compatible-release retention, rollback, and
  doctor proof. Adapter-only deployment runs its contracts and activates the
  verified component without rebuilding or mutating a native product release.
- The public Pages specimen now self-hosts a subsetted Inter variable font,
  renders repository documentation as local JavaScript-free pages with static
  SVG diagrams, and keeps its disconnected-record claim true by replacing
  curated internal registry rows with explicitly fictional public fixtures.
  The browser specimen also closes the overlay inspector on compact first
  paint, contains and wraps the mechanical observatory at intermediate widths,
  preserves its scroll position across slot changes, keeps short thread titles
  intact, separates the mobile Bell from slot labels, and provides a concept-map
  fallback when JavaScript is unavailable.
  Narrow Hallway views now keep the members drawer closed until requested,
  expose a full-width drawer over a dismissible scrim, and compact every status
  channel without clipping; generated concept diagrams also bind their visible
  prose to semantic `figcaption` elements.
  Intermediate Hallway widths now move thread metadata below the title and
  distribute all three subject tabs inside their owned strip, preventing title
  fragments and Bell hit-target overlap.
- The browser interaction prototype no longer renders full-width census or
  authority hint banners above Project, room memory, status, or Hallway record
  views. Authority stays attached to values, receipts, refusals, unavailable
  actions, status channels, and the isolated About surface; the dead renderer,
  CSS rule, responsive selector, and color tokens were removed.
- The browser interaction prototype now uses an icon-only global Bell with its
  ordinary and targeted counts overlaid on opposite upper edges. Bell scope
  follows the authenticated room/spirit presence rather than the selected
  subject, and future Project notifications share the same typed inbox.
  House Overview, Mechanics, and Memories & Lessons now use one outer width.
- The browser interaction prototype now renders a real monochrome Bell icon with
  round ordinary-unread and squared targeted-attention counts. Its deterministic
  local read model preserves Bell rows and unread indexes beyond the returned
  message page, structured `toRooms` remains the sole recipient authority, and
  local sends remain visibly undelivered. Mobile Escape closes the visible
  sidebar before leaving House, mechanics summaries label effective values, and
  disconnected product language now uses one authority vocabulary.
- The browser interaction prototype now exposes House slot 2 as a discoverable
  mechanical observatory. Account Settings routes directly to 43 source-censused
  mechanisms across seven categories, with all-category search and typed
  effective value, default, scope, owner, mutability, secrecy, apply mode,
  health, and consequence. The disconnected surface stays read-only, marks
  Host state unavailable, and exposes secret presence or health without values.
- The browser interaction prototype now exposes a global Hallway Bell with
  separate ordinary-unread and explicit-attention badges, structured room
  recipients, exact thread routing, and acknowledgment limited to the opened
  thread. Offscreen mobile navigation is absent from the accessibility tree,
  and the fixed member dock no longer obscures narrow Hallway content. The
  prototype remains local-only and disconnected from Host authority.
- Windows native release assembly now reuses Cargo's release artifacts and a
  checksum-keyed cache of the pinned PostgreSQL/pgvector, NATS, and Godot
  runtimes. Unchanged Rust dependencies no longer cold-compile on every package,
  successful version workspaces are removed, and every emitted payload remains
  freshly assembled and checksummed.
- Rust domain state now uses typed commands, outcomes, availability, recovery,
  health, disposition, embedding, and query-lane models instead of coordinated
  boolean products. Existing protocol JSON, environment variables, persisted
  Host state, and idempotency responses remain compatible; production-disabled
  embedding is no longer reported as an isolated-test condition.
- Lesson trigger matches now carry a room-wide fires ledger: each fired
  lesson reports how many times it has bitten in this room, rendered as
  `×N` on trigger cards. Repeat cooldown stays session-scoped and never
  reads the ledger.
- Room identity guidance (IDENTITY_GUIDE, starter room) now states the
  drift rule explicitly: the runtime owns invocation shapes, and tool
  tables, organ counts, and deployment paths must never be hand-copied
  into room prose.
- The Hallway gained its Bell (House memory #3676). Persistent Hallways now
  contain House-local daily threads; replies inherit the parent's thread
  across midnight. Messages carry structured `toRooms` recipients that create
  durable mention notifications; ordinary unread is derived from the gapless
  sequence against new room-stable read state, never stored per message.
  Reading with cursor advance acknowledges exactly the returned set; room
  read state advances only on contiguous coverage. Sessions in allowed rooms
  gain presence lazily on first use, and hallway refusals now carry truthful
  stable codes (hallway_not_found, room_not_allowed, spirit_mismatch,
  idempotency_reuse, message_not_found) instead of one generic validation
  receipt blaming decode_line. A new `hallway_inbox` organ lists unread,
  pending mentions, and latest-message metadata per hallway, and OMP projects
  a revision-gated Hallway Bell notice into context so no spirit has to
  remember to poll its mailbox. The House timezone is explicit runtime
  authority via SOLARISAEL_HOUSE_TZ. Live NATS knock delivery is deferred:
  Crane's outbox still requires a memory-row aggregate.
  Post idempotency now binds the normalized recipient set, inbox rows expose
  pending notification message/thread targets, and `hallway_read` accepts an
  exact thread without advancing across unreturned threads. Automatic Bell
  revision gating now belongs to the authenticated Host; the trusted OMP notice
  carries only Host-derived counts and never embeds peer Hallway prose.
- The Hallway gained an explicit Knock without turning peer prose into a
  command. Each room owns an append-only `manual`/`allow_list` wake policy;
  one addressed message may request one recipient turn, and direct reply Knocks
  inherit a root 1–8 turn ceiling, expiry, thread, and room reversal. PostgreSQL
  owns policy, idempotency, lease, and started/completed/failed lifecycle state.
  The recipient's own Host claims a short lease and its OMP adapter starts the
  turn from pointer-only routing metadata, never the peer body; claiming does
  not clear the Bell. Focused PostgreSQL, Host-protocol, and OMP actuator guards
  were mutation-proven. NATS wake delivery remains deferred behind the generic
  Crane prerequisites; the first local cut uses Host polling.
- Guarded local deployment now stops the native Athanor service before replacing
  supervised binaries, restarts it after migrations, and waits for every
  configured room Host health endpoint. Rollback also attempts to recover the
  prior service. This closes the false-green state where Windows reported the
  supervisor running after all room Hosts had been killed.
- Knock wake notices now expose their trusted PostgreSQL Knock id alongside the
  Hallway message pointer so a recipient can issue a child Knock without private
  database access. Host claims pass through the recipient's Hallway presence
  gate; started turns that outlive the root expiry become explicit failures on
  the next room claim. OMP retains locally observed turn state, retries
  started/completed settlement after transient Host failure, fails a wake whose
  custom turn never starts within 15 seconds, and releases a lifecycle that
  still cannot settle after 25 seconds instead of wedging that room's doorman.
- Project-lesson GIGA promotion now treats operator publication approval as a
  project-only proof and binds reviewer authority to the candidate's durable
  `start_review` transition. Missing or mismatched reviewer evidence is refused
  before durable promotion writes.
- OMP prose lesson triggers now inspect coalesced live assistant text updates.
  Block verdicts queue hidden correction context, abort the active provider turn,
  and remove the aborted assistant from the continuation's model context;
  reminders queue without stopping generation. Duplicate terminal stream events
  received while Rust matching is in flight now settle once instead of
  recursively restarting an empty pump.
  Every prose verdict also leaves a compact transcript card naming the lesson
  and whether it interrupted or queued a reminder; expanding the card shows the
  exact injected lesson context.
  OMP exposes extension observers after
  UI publication, so any prefix displayed before the Rust verdict remains
  visible until OMP exposes native pre-commit TTSR actuation.
- OMP now exposes a shared Athanor feedback rail instead of leaving automatic
  House activity visible only to the model. The status line identifies the
  active spirit and every House tool's running or final state; a compact widget
  names automatic room, routing, wake, Anamnesis, process-lesson, Recall, model,
  and correction activity. Automatic degradation surfaces as warnings. Tool
  cards use OMP lifecycle symbols and success/error colors while preserving
  canonical JSON behind expansion. Remember now renders a framed durable receipt
  with memory or lesson identity, room, authority, threads, and expandable
  content provenance. Recall renders ranked canon and memory evidence with
  lexical-coverage gauges, scores, source rooms, excerpts, and expandable
  provenance instead of a generic JSON summary.
  Sleep now renders a framed paper-boat receipt with room, PostgreSQL authority,
  durability, backup completion, continuity readiness, warnings, and expandable
  carried body/source/outbox evidence.


## [0.9.7.0] - 2026-08-15

### Added

- Lessons can now carry structured triggers: `condition` (regex), `ast_condition`
  (ast-grep patterns), `trigger_scope`, `interrupt_mode`, and
  `repeat_cooldown_secs` (migration `0019_lesson_triggers.sql`). PostgreSQL
  remains the only trigger store; a lesson written mid-session is live at the
  next match without a restart.
- New substrate operation `lesson_trigger_match` matches tool and prose payloads
  against trigger-bearing lessons in Rust: regex plus ast-grep with Rust,
  TypeScript, JavaScript, and Python grammars, per-lesson repeat policy (once
  per session, or a cooldown in seconds), and one row per fire in the new
  `lesson_trigger_events` ledger. A healthy response always serializes an empty
  `warnings` array.
- The OMP adapter taps `tool_call`, `tool_result`, and `context`. A
  `block`-urgency lesson stops a violating edit or write before it lands and
  returns the lesson as the reason. A `remind` fire prepends an in-band reminder
  to the tool result. Prose matches inject a current-turn reminder without
  moving earlier anchors. Every tap fails open with a 300ms budget and honors
  `SOLARISAEL_DISABLE_LESSON_TRIGGERS=1`.
- Write paths refuse lesson triggers that can never fire: a regex that does not
  compile, an ast pattern that does not parse cleanly for any supported grammar,
  an unknown scope token, or an invalid interrupt mode. GIGA promotions never
  propose triggers.
- `lesson_trigger_events` joined the substrate health contract's required
  tables.

### Known v1 ceilings

- `.sql` paths skip ast conditions with a warning (no SQL grammar in
  ast-grep-language 0.45.1); regex conditions still cover them.
- Project-scoped lessons do not fire yet; the wire carries no project field.
- Taps cover the `edit` and `write` tools.

## [0.9.6.2-rc2] - 2026-08-15

### Fixed

- Familiar dispatch can now bind a room familiar to an exact discovered OMP
  agent and model-role alias through `ompAgent` and `modelRole` spellbook
  fields. The task packet, receipt, and shared context carry that exact route
  instead of relabelling `scout`, `sonic`, `task`, or `reviewer` after the
  generic lane was chosen. Legacy spellbooks remain readable and emit an
  explicit lane-fallback warning until their bindings are migrated.

## [0.9.6.2-rc1] - 2026-08-14

### Changed

- Widened the Paper Boat delivery lane into the general Crane delivery system in
  place (schema 17). `boat_ready_outbox`, `boat_ready_receipts`, and
  `boat_ready_dead_letters` are renamed to `crane_outbox`, `crane_receipts`, and
  `crane_dead_letters` with their rows, receipts, and dead letters preserved;
  migration `0017_crane_delivery.sql` renames rather than recreates. `boat.ready`
  is now the first lane of that system and is unchanged where it counts: same
  event IDs, same `boat.ready:memory:<id>` idempotency keys, same seven-key
  pointer envelope, same `athanor.boat.ready` subject, same receipt projection to
  the Host. Existing boat rows are creased `boat.ready.v1`.
- Generalized the relay and consumer: the outbox claim no longer filters on one
  event kind, publication routes by lane (`boat.ready` keeps
  `athanor.boat.ready`; addressed Cranes use
  `athanor.crane.<recipient_kind>.<recipient_key>`), and the consumer dispatches
  by exact subject to a per-lane parser before the shared PostgreSQL receipt
  ledger. PostgreSQL stays authoritative and NATS stays delivery-only.
- Unified client typography under one face resolved from the operating system
  (`SystemFont`, Segoe UI on the reference workstation) with grayscale
  antialiasing, light hinting, and an embolden compensation for un-gamma'd
  dark-theme blending. Removed the bundled Atkinson Hyperlegible Next, Cinzel
  Decorative, and JetBrains Mono faces. Enforced two floors: no text below
  14px and no light-on-dark text color below 0.7 brightness; lifted the muted
  chrome tier to 0.78. Standing rule in project lesson 375 and `AGENTS.md`.

### Added

- Added optional Crane envelope fields — `crease_pattern`, `recipient_kind`,
  `recipient_key`, `expires_at`, `parent_intent_id`, `correlation_id` — enforced
  in both PostgreSQL and the strict `CraneEvent` parser, with column and payload
  agreement required. The `boat.ready` lane still refuses every one of them.
- Added consume-time expiry: a Crane whose `expires_at` has passed is
  dead-lettered as `expired` before the receipt ledger, so it is never applied. A
  Crane delivered on another recipient's subject is dead-lettered as
  `recipient_mismatch`.
- Added the PostgreSQL-authoritative Hallway domain and OMP tools for creating
  explicitly scoped shared channels, joining authenticated session presences,
  appending idempotent ordered messages, and reading with per-session cursors.
  Multiple sessions may embody the same spirit without sharing a singleton
  session or cursor. Hallway contact remains operator-visible and manual-wake.

## [0.9.6.1] - 2026-08-14

### Fixed

- Accepted the substrate's `clusters` and `chunks_total` in cluster staleness
  telemetry. Recall no longer refuses with `unknown field chunks_total` once
  clusters exist.
- Normalized cluster resonance at the substrate boundary: activation is clamped
  to the wire's `[0, 1]` unit-fraction contract instead of `[-1, 1]`, and a null
  cluster label serializes as an empty string instead of refusing the result.

## [0.9.6] - 2026-08-14

### Added

- Added a durable Godot effects laboratory: four source-labelled alchemical
  treatments, pinned Juicee `1.4.2` orchestration, overlap/cancel/restore
  behavior, accessibility controls, responsive layout, and live renderer
  instrumentation. Decorative output carries no Host authority.

### Changed

- Wired the remaining Godot shell actions (S07, S08, S09) to live Host commands
  and removed their dead placeholder routes. Every visible GUI action now
  performs its real operation or states its absent contract.

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
- Restored the archive’s flat grouped S01–S14 navigation rail instead of adding
  another hierarchy. Reusable nested pane technology remains where the reference
  calls for it: Preferences and its Appearance, Accessibility, and Connection
  subpanes, with focus restoration and Escape grammar.
- Matched the standalone archive shell geometry: 15.25 rem left rail, 20 rem
  right contextual panel, center instrument, identity/room stack in the left
  rail, contextual evidence cards on the right, and the compact bottom bar.
  Context/navigation become exclusive drawers only below their docked widths.
- Changed the default window to the archive comparison viewport, 1440×900, so
  launch opens directly in the complete three-column desktop composition.
- Recovered the exact S01–S14/R01 inventory from `The Athanor Design Restart.zip`.
  Screen headers now use only the archive’s enabled `OrnamentFrame` rule:
  `corners={false}`, `sigils={false}`, exact divider flourish, bar, and mirror.
- Included the new `gui/navigation` runtime resources in native release staging.

### Fixed

- Stopped the OMP adapter from deleting injected replaceable context (Recall
  working set, Striatum lessons, nudges) out of historical turns on refresh.
  Replaceable additions now anchor only at the current turn, the turn-addition
  memo is an access-touched LRU of 128 sessions, and a durable per-room shelf
  (`.omp/runtime/turn-additions/<hash>.json`) replays byte-identical context
  across adapter restarts. Ordinary turns and restarts now keep their prompt
  cache instead of missing on every message.
- Restored the `recall` organ against legacy stored `pointer_files` shapes. The
  substrate now normalizes them at its boundary instead of refusing the entire
  recall with `unknown field note`.
- Accepted the substrate's `cluster_id` in cluster resonance profile entries.
  Recall no longer refuses with `unknown field cluster_id` once memory clusters
  exist; unknown fields elsewhere remain refused.
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
- Wired S07 Familiars, S08 Dispatch builder, and S09 Saúde instruments to existing Host commands. Deleted dead S03–S06, S10–S13, and R01 navigation routes. The status rail now mirrors the authenticated Host binding. The Host defaults an absent `room_dir` to its configured room for `familiar.status` and `routing.dispatch`. Recall command-state and hash labels wrap inside their columns. Dispatch form fields now have usable heights. The navigation contract test now uses the operator map.

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
