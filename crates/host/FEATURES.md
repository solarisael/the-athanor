# host

The Athanor Host. One process serves every room of a House over HTTP and WebSocket, on one loopback bind. `athanor.exe` with no arguments starts it; see [`athanor-install`](../athanor-install/FEATURES.md#app).

`src/lib.rs` exports `HostConfig`, `KnockAutonomy`, `HostRuntime`, and `start`.
The server mounts surface, context, and command handlers as child modules.

### Host API 2 ownership

The following source capabilities pass isolated verification. Deployment remains pending.

| Module | Responsibility |
|---|---|
| `room_state.rs` | Typed room mutations, prompt directives, and the derived spirit header |
| `organ/` | Bound native operations and room-owned GIGA workers |
| `judgment.rs` | Approved provider execution, privacy checks, deadlines, and result selection |
| `lifecycle.rs` | Chat selection, answer ownership, Knock deadlines, and the boat margin |
| `server/context_session/` | Lesson plans, context assembly, replay, identity changes, diagnostics, and cache ownership |

The Host API is `2`; the envelope schema remains `1`.
The native CLI and Host share domain execution in `akasha::native`.
Administrative migrations remain outside the service dispatcher.
The Host owns worker shutdown and waits for exit before replacement.

Context replay returns pending settlement information from the native contract.
An acknowledged contract does not become pending again.
Identity changes retire the old contract without reassigning its historical receipts.
The complete contracts and proof limits appear in [Architecture](../../docs/ARCHITECTURE.md) and [Evidence](../../docs/EVIDENCE.md).


### house

`src/house.rs`. One process, many rooms.

- `start(Vec<HostConfig>)` runs one thread, one Tokio runtime, and one listener (`house.rs:86-105`).
- Each config becomes its own `server::Host`, nested under its room path in one axum router (`house.rs:166-202`).
- `house_settings` refuses zero rooms and a room configured twice (`house.rs:205-234`).
- Every room must share the bind address, the bearer token, the House id, `DATABASE_URL`, and `ATHANOR_NATS_URL`. The spirit and the session may differ per room (`house.rs:205-234`).
- One bearer token serves every room (`house.rs:222`). It proves reach, not identity; see [`LIMITATIONS.md`](../../docs/LIMITATIONS.md#4-identity).
- The House owns the shared PostgreSQL pool, Insula emitter, delivery task, retention task, cancellation token, and task tracker.
- [`ARCHITECTURE.md`](../../docs/ARCHITECTURE.md#31-one-process-many-rooms) lists the source of every `HostConfig` field.

### server

`src/server.rs` owns the room's Host configuration and command boundary.

**Routes.**

- The file binds no listener and holds no TLS code. Its output is one axum `Router` (`server.rs:250-258`).
- `GET /health` reports the Host API separately from the envelope schema, plus scoped runtime health.
- The WebSocket upgrade path is `DEFAULT_HOST_WS_PATH` from `protocol` (`server.rs:253`).
- The router merges the Insula, panel, and surface routers (`server.rs:255-257`).

**Bearer.**

- `authorized` reads only the `Authorization` header. It requires the `Bearer ` prefix, then compares the token in constant time (`server.rs:282,301-312,314-323`).
- A failure returns 401 with `{error: "unauthenticated"}` before the upgrade (`server.rs:277-299`).
- No query parameter, subprotocol, or per-frame check carries the token. Authentication happens one time, at the upgrade.

**Socket.**

- The socket selects among the cancellation token, the Hallway deltas, the recall deltas, the receipts, the chat deltas, and the next client frame (`server.rs:369-432`).
- Cancellation sends a close frame and ends the socket (`server.rs:371-374`).
- Recall deltas flow only after `Subscribe` returns a snapshot. Receipts and chat follow the same rule with their own subscribe commands (`server.rs:440-453`).
- Hallway deltas reach only the socket whose session equals the target session (`server.rs:375-390,455-459`).
- When the socket forwards a Hallway inbox projection, it stores the event's state hash as the inbox fingerprint of that session (`server.rs:378-385`).
- A lagged recall stream sends `projection delta stream lagged; resync required`, then ends the socket (`server.rs:398-402`).
- A lagged chat stream sends `chat delta stream lagged; resubscribe required`, then ends the socket. A lagged receipt stream ends the socket without a message (`server.rs:413,424-428`).
- A binary frame gets `binary WebSocket messages are not accepted`. A ping gets a pong. A close or an error ends the socket (`server.rs:469-479`).

**Commands.**

- Each text frame passes JSON parse, a semantic hash, `parse_client_command`, then `validate_command` (`server.rs:525-574`).
- The command families are listed in [Architecture](../../docs/ARCHITECTURE.md#33-the-command-socket).
- `validate_command` checks configured House and room scope, current embodied spirit, recipient, and envelope constraints.
- It also requires visibility `operator`, authority class `room_state`, and a non-empty sender session of at most 256 bytes (`server.rs:2567-2581`).
- `sender_session` is chosen by the caller and is not authenticated.
- `Subscribe` and `PaperBoatReceiptSubscribe` pass with a blank binding. `Resync` does not (`server.rs:2551-2566`).
- `knock_authority` gates Hallway Knock claim and settle. It refuses a room or a spirit that is not this Host's own (`server.rs:1651-1665`).
- `PresenceOpen` passes `authenticate_presence`. The operator comes from the room state, not from the wire (`server.rs:1257-1285`).
- Chat lines take the author name from the room store identity, never from the caller (`server.rs:999-1013`).
- `resolve_room_dir` accepts an empty request or this Host's own room directory. Only `RoutingDispatch` and `FamiliarStatus` use it (`server.rs:2067-2093`).
- The Host reads the room spellbook and the quest report from disk (`server.rs:2097-2101,2110-2114`).
- `LogConversation` writes the transcript, the source ledger, and a debug provenance line (`server.rs:2131-2235`).
- `LogConversation` refuses a foreign `room_dir` before writing. The adapter reports the refusal instead of treating it as a capture.
- Routing, lineage, and shell answers are typed events: `RoutingResultEvent`, `LineageResultEvent`, and `ShellResultEvent` (`crates/protocol/src/host.rs:48-61`).
- `meta_for_projection` stamps the event metadata of each answer (`server.rs:2723-2727`).

**Bridges.**

- The Hallway bridge consumes `HallwayPostProjection` from JetStream through `Broker::hallway_consumer` for this Host's room (`server.rs:2855-2879`).
- A post passes only with schema version 1, a positive message id and sequence, this room's subject, another sender room, and empty or matching `to_rooms` (`server.rs:2885-2894`).
- A post that fails these checks is terminated. Insula records `invalid_projection`, never the payload (`server.rs:2895-2902`).
- Receipt ingest uses this Host's room (`server.rs:3102`).
- `ReceiptTracker` lives in memory only: the health state, the latest projection, and a refusal reason (`receipt.rs:5-12,75-98`).
- The tracker starts as `Disabled`, `MissingBroker`, or `Connecting`, by whether AKASHA is on and a broker is configured. `Disabled` and `MissingBroker` never change.

Not re-verified at a6ab453; `crates/host` (`server.rs`, `receipt.rs`) would decide:

- `knock_authority` runs before any database access.
- A Bell rings when an inbox entry carries unread messages or mentions.
- Knock claim and settle open an Insula span, refuse without a database, then publish a typed event.
- A command needs exactly one hop, a correlation id equal to the message id, and an unexpired RFC 3339 expiry.
- The same idempotency key with the same body hash replays the stored outcome.
- `commit_change` raises the version and the sequence, hashes the state, saves it, then emits a delta.
- The receipt bridge opens the boat receipt stream with a bounded ephemeral consumer, and retries after each failure.
- `ingest` classifies a receipt as accepted, duplicate, stale, foreign room, or conflict.

### chat

`src/chat.rs`. The chat ring of one room.

- Each room keeps a bounded `ChatLog` with messages, drafts, and a sequence.
- Atomic room-local checkpoints preserve this presentation state across restart. Streaming drafts have a separate checkpoint.
- A failed write leaves accepted state unchanged. A settled draft does not return after restart.
- The Host stamps the sequence itself and increments it after each append. The caller passes the time (`chat.rs:151-162`).
- Three callers append: `ChatSay` over the socket and `chat/say` over HTTP for operator lines, and `ChatTurn` for spirit lines (`server.rs:934,950`; `surface.rs:85`).
- `ChatDraft` replaces the live draft (`server.rs:969`).
- Three callers read: `ChatSubscribe`, `chat/snapshot`, and the `room/state` summary (`server.rs:919-920`; `surface.rs:57,123`).
- Operator lines get `identity.operator` as the author name. Spirit lines get `identity.spirit` (`server.rs:935,951,970,999-1012`; `store.rs:51-75`).
- A `turn` retires the draft of its turn id and appends one spirit line. A later draft for that turn changes nothing (`chat.rs:54-78,80-97,122-124`).
- Dedupe is by the pair of author and turn id. A repeated say id or turn id appends nothing (`chat.rs:144-150`).
- The ring keeps at most 256 entries and drops the oldest first. It keeps at most 8 drafts (`chat.rs:16-23,108-110`).
- Text is capped at 32768 characters. A line keeps 64 steps and 64 thinking blocks, with 32768 thinking characters in total (`chat.rs:164-166,171-207`).

### presence

`src/presence.rs`. The Presence lifecycle of the room.

- `PresenceRuntime` holds a map of sessions and a replay ledger (`presence.rs:46-50`).
- A session records the frame, the authoritative ledger, the active contract, and at most 16 receipts (`presence.rs:16-17,52-60,315-318`).
- Sessions are keyed by the authenticated binding session (`presence.rs:155,174-188`).
- Open refuses a request binding that differs from the authenticated binding, and a live frame that belongs to another binding (`presence.rs:204-212`).
- The replay ledger is keyed by session and idempotency key. Each entry records the operation, the request digest, and the outcome (`presence.rs:12-14,62-69`).
- The ledger keeps 64 entries and drops the oldest. A key reused for another operation or another body is a conflict (`presence.rs:373-419`).
- The server loads a live session from PostgreSQL and adopts it. The ledger and the close are written back to the store (`presence.rs:223-234`; `server.rs:1129-1136,1156-1162,1394`).
- Capabilities come from Host configuration: `RoomState` always, `Akasha` when enabled, and `Receipts` when enabled (`presence.rs:421-436`).

Not re-verified at a6ab453; `adapters/omp` would decide:

- The adapter owns no Presence state and authors no ledger.

### surface and panel

`src/surface.rs` and `src/panel.rs`. The read and chat doors for Pulse.

**Surface.**

- Three POST routes sit behind `panel.protect`: `/athanor/v1/chat/snapshot`, `/chat/say`, and `/room/state` (`surface.rs:13-15,29-37`; `server.rs:257`).
- `chat/snapshot` returns the room, the messages, and the drafts (`surface.rs:49-58`).
- `chat/say` returns the room, `accepted`, `repeated`, and the message. The author is `identity.operator` from the room store. The say publishes one delta (`surface.rs:60-95`).
- `room/state` returns the room, the operator, the spirit, this room's presences, and a chat summary of entries and last time (`surface.rs:118-135`).
- Each presence in `room/state` carries its session, spirit, and operator (`surface.rs:118-121`). Any bearer holder can read them; see [`LIMITATIONS.md`](../../docs/LIMITATIONS.md#8-known-defects-in-the-current-code).
- `routingMode`, `recallPolicy`, and `modelDefault` appear only when the room root carries them (`surface.rs:136-151`).
- A room root that names a foreign room is refused with 503 (`surface.rs:103-112`).

**Panel.**

- Seven POST routes read House state: `docket/board`, `hallway/inbox`, `hallway/messages`, `docket/evidence`, `memory/timeline`, `memory/read`, and `lesson/timeline` (`panel.rs:37-43,109-150`).
- No route writes. Response bodies are the substrate's own (`panel.rs:6-7,395-417`).
- Identity comes from `HostConfig`, never from the request. Board, inbox, and evidence inject the room, spirit, House id, and the session `host:<room>` (`panel.rs:247-254,270-325`).
- `hallway/messages` injects the room only (`panel.rs:293-298`).
- The three timeline routes carry no identity. They pass the substrate's own parameter types (`panel.rs:332-393`).
- A substrate refusal passes through. The panel adds and hides nothing (`panel.rs:395-417`).

**Shared gates.** Panel and surface routes need the bearer token and refuse a query string. They allow 4 concurrent operations, then return 429 `panel_busy`. The body limit is 64 KiB (`panel.rs:45-46,132-141,199-212`).

Not re-verified at a6ab453; `crates/host` (`panel.rs`) would decide:

- Without a database pool the panel returns 503 `panel_database_unavailable`.

### insula

`src/insula.rs`. The observability door of the Host.

- Six POST routes: `insula/events`, `vitals`, `trace`, `retention`, `spans`, and `unverified-exit` (`insula.rs:27-38,272-284`).
- The bearer guards all six routes. A query string is refused (`insula.rs:41-55`).
- The routes allow 4 concurrent operations, then return 429. The body limit is 512 KiB. A batch holds 1 to 128 events (`insula.rs:41-55`).
- A vitals window holds at most 366 days. A read limit stays below the substrate cap. A trace id must be a canonical UUID (`insula.rs:321-372,392-656`).
- The room and the House come from the binding, never from the caller (`insula.rs:392-656`).
- The only database write is `ingest_batch`. It inserts into `insula.log` and upserts `insula.vitals_minute` (`insula.rs:412`). The other five routes read.
- Trace rows and unverified-exit rows carry session ids (`insula.rs:491-500,615-656`).
- Each route opens a span and ends it with an outcome class (`insula.rs:306-318`).
- `health()` feeds the `insula` field of `GET /health` (`server.rs:273`).

Not re-verified at a6ab453; `crates/host` (`insula.rs`) would decide:

- The refusal codes `unexpected_query`, `insula_busy`, and 503 `insula_unavailable`.
- The health states and the success and failure counters.

### viewport

`src/viewport.rs`. The presentation filter for a recall result. The socket reaches it through `ApplyRecallViewport` and `AnalyzeContext`; it has no HTTP route (`server.rs:746-775,799-801`).

- `apply_viewport` turns one result into a bounded presentation plus diagnostics (`viewport.rs:448`).
- Automatic mode suppresses a candidate as `zero-terms`, `glue-only`, `insufficient-evidence`, or `saturated` (`viewport.rs:484-508`).
- A candidate needs two independent signals, unless an exact signal matches (`viewport.rs:484-508`).
- A candidate already exposed in this session is `saturated` (`viewport.rs:484-508`).
- Automatic mode keeps at most 5 candidates. Manual mode keeps `hearth::MANUAL_RECORD_CAP`. Overflow becomes a `record-cap` suppression (`viewport.rs:115,465-468,520-534`).
- Manual mode also warns with the dropped ids (`viewport.rs:579-587`).
- Candidate fields have ceilings: an excerpt of 900 characters, 6 neighbors, and 5 reasons (`viewport.rs:89-110,278-332`).
- Canon is kept only when exact or file-linked, at most 6 rows. A cut summary carries `full_read` (`viewport.rs:122-128,355-430`).
- Raw chunks, at most 5, appear only when no candidate is kept (`viewport.rs:432-446`).
- Manual mode adds the taxonomy, the cluster nudge, the cluster resonance, and the memory handle (`viewport.rs:588-675`).
- Automatic mode keeps at most 8 warnings of 300 characters each. Manual warnings are unbounded. Manual mode keeps the whole `body`; Automatic mode drops it (`viewport.rs:335-338,550-578`).

Not re-verified at a6ab453; `crates/host` (`viewport.rs`) would decide:

- The value of `hearth::MANUAL_RECORD_CAP`. The comment says 5 (`viewport.rs:111-115`).
- The list of exact signals, and whether Manual mode ever counts an exposure.
- The diagnostic counts for each reason.

### policy

`src/policy.rs`. The Recall Policy decision engine, held for each sender session. It gates the recall refresh and clear decision, not an HTTP door.

- A refresh is eligible only in a non-Quiet mode, and only with an explicit lookup, a technical project, or pending recovery (`policy.rs:220-221`).
- A decision returns one action: none, clear, refresh, or clear then refresh. Clear happens on Quiet, a mode change, or a project change (`policy.rs:274-284`).
- Auto mode resolves in order: a technical project gives work. A lookup gives mixed with a project, or conversation without one (`policy.rs:364-429`).
- Tool evidence gives work. A judged mode lives one turn. Conversation needs two turns of hysteresis (`policy.rs:210-212,364-429`).
- Refresh reasons rank: post-compaction recovery, requested mode change, active project change, resolved mode change, empty working set, explicit lookup, topic change, stale working set (`policy.rs:245-273`).
- A working set goes stale after 8 turns or 4096 tokens (`policy.rs:245-273`).
- A topic change needs at least 3 terms on both sides and an overlap below 0.25 (`policy.rs:245-273`).
- A degradation text holds at most 240 characters (`policy.rs:299,311`).
- `apply_requested_mode` writes `explicit-override`, or `awaiting-auto-resolution` for Auto (`policy.rs:347-361`).

Not re-verified at a6ab453; `crates/host` (`policy.rs`) would decide:

- Query terms stay unique and hold at most 16 entries.
- `complete_refresh` clears recovery. `invalidate_after_compaction` seeds recovery terms from the summary.
- The serialized form keeps the older field names.

### routing and dispatch

`src/routing.rs`, `src/routing/dispatch.rs`, and `src/routing/spellbook.rs`. The worker lanes, the room spellbook, and the dispatch packet.

- `house_dispatch` takes exactly one lane or one familiar (`routing.rs:471-535`).
- A constant table holds 4 lanes: `smol-scout`, `smol-executor`, `tester`, and `verifier` (`routing.rs:100-157`).
- Each lane fixes the OMP agent, the model role, the tools, editing, the allowed context modes, and whether acceptance is required (`routing.rs:100-157`).
- Advisor is listed with `dispatchable: false`. It is a review channel, not a lane (`routing.rs:189-199`).
- Dispatch only validates and packages. Every receipt carries `executed: false`, so a refused request still gets a receipt (`dispatch.rs:176-177`).
- An accepted packet is `spawnPacket { tool: "task", args }`. The Host never spawns a worker (`dispatch.rs:440-456`).
- The gates: a known lane, a task, one or more complete lesson bodies, acceptance where required, an exact target for editing lanes, and allowed context modes (`dispatch.rs:151-160,183-233`).
- The spellbook lives at `<room_dir>/familiars/spellbook.json`, then `litters.json`. The first found wins (`routing.rs:300-301`).
- The spellbook binds familiars and aliases. Ids are kebab-case, and `ompAgent` and `modelRole` come together (`spellbook.rs:52-192`).
- Spellbook loading asks the caller for each file read. The server supplies the read and the room directory (`routing.rs:354-360`; `server.rs:867-870,2097-2101`).
- `familiar_dispatch` sets the lane from the spellbook. Only `house_dispatch` reaches it; no socket command calls it directly (`routing.rs:230-279`).
- The socket serves `RoutingStatus`, `RoutingDispatch`, and `FamiliarStatus` (`server.rs:854-882`).

Not re-verified at a6ab453; `crates/host` (`routing.rs`, `dispatch.rs`) would decide:

- The shared context formats the hints, the acceptance lines, and the lesson bodies.
- A hint's risk level comes from a closed list.
- A dispatch cannot override the model role.
- The fields of the `familiar_status` report.

### config

`src/config.rs`. The settings of one room's Host.

- `HostConfig` holds the bind, the bearer token, the room directory, the state directory, the House id, the room, the spirit, the session, the database URL, the NATS URL, and the Knock autonomy (`config.rs:55-68`).
- The caller builds the struct. The production builder is `host_configs()` in `athanor-install` (`config.rs:4,21-41`; `crates/athanor-install/src/app.rs:17-58`).
- The only environment variable it reads is `ATHANOR_HOST_KNOCK_AUTONOMY` (`config.rs:4,21-41`).
- `KnockAutonomy` accepts exactly `off` or `claim`. An absent value means `claim` (`config.rs:21-41`).
- The bind must be loopback. The token, the House id, and the spirit must not be blank (`config.rs:87-89,92-120`).
- The room must be a safe room key and must equal the `room` field of the room-state file (`config.rs:87-89`; `store.rs:56-62,157-162`).
- A safe room key uses lowercase letters, digits, and single inner hyphens, and is never `house` (`crates/protocol/src/contract.rs:34-43`).
- The room-state path is `room_dir/.omp/runtime/athanor-house-state.json`. The scope is `room:<room>:recall_policy` (`config.rs:71-84`).
- AKASHA is on when `database_url` or `nats_url` is set. The production builder always sets both (`config.rs:71-84`; `app.rs:27-58`).
- The installer writes `runtime.json` `hostPort` as `DEFAULT_HOST_WS_PORT` (`crates/athanor-install/src/installer.rs:1229-1250`).

Not re-verified at a6ab453; `crates/host` (`config.rs`) would decide:

- The WebSocket path shape, the NATS URL rules, and `validate`.

### store

`src/store.rs`. Durable state on disk.

- `RoomStateStore` validates the configured room and current identity.
  An absent embodiment field can use the configured agent.
  A malformed identity receives a refusal.
- Native room mutations and policy writes share the same room mutex.
  Atomic writes preserve unrelated fields and the manual spirit body.
  Legacy filename migration runs before the initial store load.
- `HostDurableStore` keeps three files in `state_dir` (`store.rs:15,285-320,322-441`).
- `recall-policy-cursor.json` holds the projection id, the version, the sequence, and the state hash.
- `recall-policy-receipts.json` holds the idempotency key, the body hash, the outcome, and the store time. It keeps at most 512 receipts and drops the oldest.
- `recall-policy-sessions.json` holds the recall policy session of each sender session.

Not re-verified at a6ab453; `crates/host` (`store.rs`) would decide:

- The cursor refuses a foreign projection id.
- The recipes of `state_hash` and `body_hash`.
