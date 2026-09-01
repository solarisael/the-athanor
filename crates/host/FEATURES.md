# host

The Athanor Host. One loopback server that serves one room over WebSocket and HTTP.

### server

`src/server.rs`, about 2560 lines. The largest module in the crate. It holds every concern below.

- **The `process_text` dispatcher.** It parses, authenticates, and routes 26 typed command variants.
- **Command vocabulary.** The Host serves Recall Policy, Context, Hallway, AKASHA, Routing, Lineage, Shell, Paper Boat receipt, and Presence commands.
- **Presence lifecycle.** `PresenceRuntime` owns the one live session frame, the one active turn contract, the bounded settlement receipts, the authoritative `PresenceLedger`, and the close material. The adapter owns no Presence state and authors no ledger.
- **Presence replay ledger.** Every Presence door is keyed by authenticated session plus idempotency key, and replay is checked before any lifecycle lookup. An exact retry returns the prior typed outcome; the same key with a changed body, or reused across two operations, refuses by name. Retained entries are bounded, so a successful close replays after its session is removed while a stale compile replay answers without reactivating an expired contract.
- **Presence authentication.** `authenticate_presence` derives the binding from the authenticated envelope and the room's own `embodiedSpirit` and `operator`. A caller's claimed binding is checked against it, never used in its place. Capabilities are projected from configuration the Host already resolved — room state, an AKASHA pool, a receipt bridge — and never read from the wire.
- **WebSocket transport.** The Host upgrades the configured path, splits the socket, then selects between the cancellation token, the delta broadcast, the receipt broadcast, and the next client frame.
- **Subscription gates.** The socket forwards deltas only after a subscribe command returns a snapshot. Receipts follow the same rule.
- **Transport refusals.** A lagged broadcast returns `projection delta stream lagged; resync required`. A binary frame returns `binary WebSocket messages are not accepted`. A ping returns a pong. A close ends the socket.
- **Bearer authentication.** `authorized` reads the `Authorization` header, requires the `Bearer ` prefix, then compares the token in constant time. A failure returns 401 with the diagnostic.
- **Health endpoint.** `GET /health` returns the status, the schema version, the WebSocket path, the projection identifier, the cursor version, the sequence, the state hash, the delivery health for AKASHA, and the Insula health.
- **Knock authority.** `knock_authority` runs before any database access. It refuses when the operator disables autonomy, and it refuses a room or a spirit that is not this Host's own. Authority comes from `HostConfig`, never from the caller envelope.
- **Hallway inbox projection.** The Host reads the inbox, hashes it into a fingerprint, and marks the projection as changed only when the fingerprint moves. A bell rings when an entry carries unread messages or mentions.
- **Hallway Knock claim and settlement.** Both operations open an Insula span, refuse without a database, call the substrate, then publish a typed claimed or settled event.
- **Routing and lineage responses.** `routing_response`, `lineage_response`, and `shell_response` wrap a result value in a typed event under the routing, lineage, or shell projection identifier.
- **Room directory authority.** `resolve_room_dir` accepts an empty request or a path that canonicalizes to this Host's own room directory. Anything else is rejected loudly.
- **Idempotency and receipts.** The same key with the same body hash replays the stored outcome. The same key with a different body is refused.
- **State commit.** `commit_change` increments the version and the sequence, hashes the next state, writes the room state, saves the cursor, the sessions, and the receipt, then emits an accepted outcome with a delta.
- **Command validation.** `validate_command` requires exactly one hop, a correlation identifier equal to the message identifier, an unexpired RFC 3339 expiry, and a binding that matches this Host. Only the two subscribe commands may carry a blank binding.
- **Conversation logging.** `log_conversation` writes the transcript, the source ledger, and a debug provenance line. It skips a turn that the transcript already holds.
- **Room file reads.** The Host reads the room spellbook and the quest report from disk. `house_core` owns which files exist and what they must contain.
- **Event metadata.** Two builders stamp every outgoing event with the schema version, the identity, the correlation chain, the scope, and the projection identifier.
- **Receipt bridge.** A background task connects to NATS, opens the boat receipt stream, and creates a bounded ephemeral consumer. It replays receipts, ingests each one, and acknowledges it. Every failure publishes a degradation and retries.
- **Insula spans.** Command paths open and close observation spans with an outcome class taken from the result.

### policy

`src/policy.rs`. The Recall Policy decision engine, held for each sender session.

- `evaluate` returns one action: none, clear, refresh, or clear then refresh.
- `resolve_auto_mode` picks the mode. A technical project gives work. An explicit lookup gives mixed with a project, or conversation without one. Tool evidence gives work and outranks the prompt words.
- Conversation hysteresis needs two turns. The first turn gives mixed, the second gives conversation.
- The refresh reason is chosen in a fixed order: recovery after compaction, requested mode change, project change, resolved mode change, empty working set, explicit lookup, topic change, then stale working set.
- A working set goes stale after 8 turns or after 4096 more observed tokens.
- A topic change needs at least 3 terms on both sides and an overlap below 0.25.
- Query terms merge the required terms, the recognized entities, the active project, the recovery terms, and the route terms. The list stays unique and holds at most 16 entries.
- `complete_refresh` clears the recovery state and records the new working set.
- `fail_refresh` records a bounded degradation of at most 240 characters.
- `invalidate_after_compaction` drops the working set and seeds recovery terms from the summary.
- `apply_requested_mode` writes an explicit override, or marks the projection as awaiting automatic resolution.
- The serialized form keeps the established field names, so an older room file still loads.

### insula

`src/insula.rs`. The observability door of the Host.

- Four routes accept POST only: event ingest, vitals, trace, and retention.
- One bearer layer guards all four routes. It uses the same Host token.
- A request with a query string is refused with 400 `unexpected_query`.
- A semaphore allows 4 concurrent operations. A fifth returns 429 `insula_busy`.
- The body limit is 512 KiB. One batch holds at most 128 events.
- Without a database pool the routes return 503 `insula_unavailable`.
- The health snapshot reports unavailable, unverified, ok, or degraded, plus the counters for successful and failed operations.
- Both read routes ask for one row beyond the caller's limit, so truncation is reported honestly.
- Each route opens a span and ends it with an outcome class taken from the HTTP status.

### viewport

`src/viewport.rs`. The presentation filter for a recall result.

- `apply_viewport` turns one result into a bounded presentation plus diagnostics.
- Automatic mode suppresses a candidate for one of four reasons: zero terms, glue only, insufficient evidence, or saturated.
- A candidate needs two independent signals, unless an exact signal matches.
- Exact signals cover the source path, a canon file, a canon term, a query date, and a distinctive query word.
- The session counts exposures for each candidate identity. A second exposure saturates the candidate. Manual mode never counts.
- The presentation keeps at most 5 candidates.
- Raw semantic chunks and content chunks appear only when no candidate survives.
- Manual mode adds the taxonomy, the cluster nudge, the cluster resonance, and the memory handle. Automatic mode omits all four.
- Every field is bounded. Text is truncated by character count and lists are truncated by entry count.
- Diagnostics report the kept count, the suppressed count, and a count for each reason.

### store

`src/store.rs`. Durable state on disk.

- `RoomStateStore` reads and writes the `recallPolicy` block inside the room state file. It validates the file before it writes, and it stamps `updatedAt` and `lastUpdatedAt`.
- `HostDurableStore` opens the state directory and holds three files: the cursor, the receipts, and the sessions.
- The cursor refuses a foreign projection identifier. When the stored hash differs, the version and the sequence both increment.
- Receipts are bounded to 512 entries. The oldest entries are dropped.
- `state_hash` is a SHA-256 digest over the serialized projection.
- `body_hash` is a SHA-256 digest taken after the volatile envelope fields are removed.
- Every write is atomic. The file is written, synced, then replaced.

### panel

`src/panel.rs`. The read door for the Pulse operator surface.

- Six routes accept POST only: the Docket board, the Hallway inbox, the Docket evidence, the memory timeline, one memory read, and the lesson timeline.
- No route writes. The panel renders House state and holds no private authority.
- Identity comes from `HostConfig`. The session is `host:<room>`. A bearer token proves reach, not the right to act as another room.
- The three timeline routes carry no identity. They deserialize the parameter types of the substrate directly.
- One bearer layer guards all six routes, refuses a query string, and allows 4 concurrent operations. A fifth returns 429 `panel_busy`.
- The body limit is 64 KiB.
- Without a database pool the routes return 503 `panel_database_unavailable`.
- A refusal from the substrate maps to a status code and a bounded error body.

### receipt

`src/receipt.rs`. The tracker for the latest Paper Boat receipt.

- `ingest` parses the exact sanitized schema, validates it, then classifies the result as accepted, duplicate, stale, foreign room, or a conflict.
- `validate_projection` checks the schema version, a UUID event identifier, a canonical decimal record identifier, the room length, an RFC 3339 timestamp, a positive stream sequence, and a lowercase SHA-256 digest.
- The bridge holds five states: disabled, missing broker, connecting, connected, and degraded.
- Disabled and missing broker never change. Only a configured broker can connect or degrade.
- `state` reports the visible status: delivered, refused, degraded, or pending. Every status except delivered carries a diagnostic.
- Every reason string is bounded to 256 characters.

### config

`src/config.rs`. Startup settings read from the environment.

- `from_env` reads every setting. A blank value counts as absent.
- The bind address must be loopback. The default is `127.0.0.1:8787`.
- The token, the room directory, the state directory, the house identifier, the room, the spirit, and the session are all required.
- The room key must use lowercase letters, digits, and single inner hyphens. The reserved key `house` is refused.
- The WebSocket path must be one absolute path with no query and no fragment.
- AKASHA turns on when `DATABASE_URL` or `ATHANOR_NATS_URL` exists.
- The NATS address must use the `nats://` scheme, stay under 2048 characters, and hold no whitespace.
- `KnockAutonomy` parses `off` and `claim` exactly. A typo stops startup instead of a silent downgrade. An absent value gives bounded claims.
- `room_state_path` names the room state file. `scope` names the authority scope for this room.
- `validate` repeats the checks against an already built configuration.

### lib

`src/lib.rs`, 28 lines. The launcher.

- It declares the eight modules and exports the configuration, the `Host` type, and the defaults.
- `run` builds the Host, binds the TCP listener, logs the address, then serves until Ctrl-C.

### main

`src/main.rs`, 27 lines. The binary entry point.

- It installs the tracing subscriber with the `house_host=info` default.
- It reads and validates the configuration. A refused startup exits with code 2.
- A failure during the run exits with code 1.
