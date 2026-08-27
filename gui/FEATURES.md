# athanor-godot

The GDExtension crate for the Athanor client. Eleven Rust modules under `src/`. The Godot scenes and the GDScript files are outside this map.

### protocol

`src/protocol.rs`, 1431 lines. The whole wire contract of the client. Every other module depends on it.

- **One home for wire literals.** Every command name, event name, projection identifier, and default address lives here. The names are imported from `house-protocol`, the same crate the Host uses, so the client cannot drift onto a guessed value.
- **Display text stays separate.** Display text never reaches the wire. A wire value is never shown as a label without a mark.
- **Strict readers.** The dictionary helpers require the exact type. A missing field, a wrong type, or an unknown field refuses the envelope. Nothing defaults.
- **Schema negotiation.** `parse_inbound` checks the schema version first. A version the client cannot speak is refused, never partly read.
- **`HostBinding`.** The house, room, spirit, and session come only from the Host envelope. The client never infers identity from its window, its configuration, or its working directory.
- **Recall Policy projection.** Every field is required in a snapshot. A missing value refuses the snapshot instead of inventing a plausible state.
- **Modes.** `RequestedMode` is the one operator-writable axis. `ResolvedMode` is read only and comes from the Host.
- **`apply_delta`.** It applies one field update onto the cursor and the projection, and it returns the reason a replay is needed when the order breaks.
- **Paper Boat parser.** `parse_paper_boat_receipt` reads the status, the receipt, and the diagnostic out of a receipt snapshot.
- **Routing parsers.** `parse_routing_status`, `parse_familiar_status`, and `parse_dispatch_result` read the lanes, the advisor, the familiars, and the spawn packet.
- **Outbound builders.** `base_envelope` builds the envelope. Identity and authority fields stay empty until the Host publishes a binding.
- **Commands.** Subscribe, resync, acknowledge, set the requested mode, subscribe to receipts, request the routing status, request the familiar status, and request a bounded dispatch packet.
- **Bounds.** One hop only. A command expires after 30 seconds.

### host_session

`src/host_session.rs`. One authenticated session shared by every screen.

- This node alone owns the transport, the credential, the reconnect controls, and the Host binding. Screens own projections and presentation only.
- Without `ATHANOR_HOST_TOKEN` the node stops and reports that no connection was invented.
- The address comes from `ATHANOR_HOST_URL`, or from the exported fallback.
- On open it sends one subscribe command with no binding, then reports the open state.
- It stores the binding out of every envelope that carries one, and it clears the binding on close.
- Five signals report the real transport facts: opened, closed, malformed, unavailable, and message.
- It exposes the phase, the address, the binding, a new identifier, a send, an open, and a close to the screens.
- It stops processing when the link closes.

### host_link

`src/host_link.rs`. The only outbound network surface in the client.

- It refuses any scheme other than `ws://` and `wss://`, so no direct database, broker, provider, or harness path can be configured.
- It refuses an empty address and an address with no host or port.
- It refuses an empty token. The client will not connect without authentication.
- It sends the bearer token as a handshake header.
- `poll` drains the transport state and every queued packet. It never interprets an envelope; that stays in `protocol`.
- An empty packet, a non-UTF-8 packet, or a packet that is not a JSON object becomes a malformed event.
- A close event carries the code and the reason.
- `send` refuses while the link is not open.
- `new_identifier` returns a 128-bit hexadecimal identifier for the message and the idempotency key.

### shell

`src/shell.rs`. The root of the operator shell.

- The center owns the active instrument. The two navigators are separate reliquaries, never hidden authorities.
- One root owns the docking, the drawer state, the screen routing, and the local presentation choices.
- Six screens: resume, Recall Policy, routing, familiars, dispatch, and health. `ATHANOR_INITIAL_SCREEN` picks the first one, and an unknown value warns and falls back.
- Three layout classes by width: wide above 1200, compact above 800, narrow below. The operator can override the class.
- The layout sets the margins, the navigator visibility, the scrim, the menu button, the identity label, and the column count.
- Every instrument shares one vertical scroll owner. The resume screen scrolls its own transcript, so the shared scroll is disabled there.
- The status rail mirrors the real link state and the real binding. Without a binding it shows the absent marks.
- The composer never pretends. A submit repeats the exact reason the Host serves no conversation contract yet.
- It requests the latest Paper Boat receipt when the link opens, then pushes the card fields into the center page.
- Escape closes the open drawer, after the navigator has had its chance to consume it.

### recall_policy

`src/recall_policy.rs`, 1303 lines. The first Host-backed instrument and the only write the client can author.

- **Runtime invariants.** Nothing appears without a real snapshot or delta. Every unavailable control carries a specific visible reason. The disclosure is re-asserted on every render and the scene cannot empty it.
- **Separate channels.** Transport phase, projection readiness, command lifecycle, and subsystem health never merge into one verdict.
- **Link states.** Idle, connecting, connected without a snapshot, snapshot applied, disconnected, and protocol refused. Each carries a mark and a word, so the state survives greyscale.
- **Command phases.** Idle, pending, acknowledged, refused, and failed. A refused command names the Host reason.
- **Two operator acts.** Proposing a mode and applying it stay separate. A new proposal clears a stale outcome.
- **Named refusals.** The proposal controls report an invalid address, no connection, a waiting connection, a closed connection, a refused protocol, a missing snapshot, or a pending command. The apply control adds a missing selection, a Host refusal for the same mode, and a mode that is already active.
- **Idempotent retry.** A retry after a transport failure replays the same idempotency key, so the Host returns the existing result or a stable conflict.
- **Pending bound.** A pending command times out after 15 seconds and becomes a failure with the reason.
- **Snapshot flow.** A snapshot before a binding uses subscribe; afterwards it uses resync. Every applied snapshot and delta is acknowledged.
- **Replay.** A delta that will not apply asks the Host for a replay instead of showing a guessed state.
- **Loss of trust.** A closed link, an unavailable session, or a refused envelope drops the projection, the binding, and the cursor.
- **All-or-nothing binding.** Either every scene binding resolves or none is used, so a half-wired scene cannot present half a state.

### health

`src/health.rs`. The disclosure screen for client-side health.

- Five channels stay separate and never collapse into one verdict: transport, binding, recall health, Paper Boat delivery, and the last protocol refusal.
- Every channel reports its own real Host event. An absent value is written as the absent mark.
- The refresh action asks for the two existing snapshots. It never invents a state.
- The action names its own refusal: no connection, no authenticated binding, or a request already pending.
- It routes each envelope by projection identifier to the recall reader, the receipt reader, or the routing reader.
- The routing reader refuses only when all three routing parsers fail, and it reports all three reasons.
- A pending request clears when the matching correlation identifier returns.
- A missing scene binding is reported by name and the screen renders nothing.

### routing

`src/routing.rs`. The read-only view of the worker lanes of the House.

- The disclosure states that this screen does not dispatch, start agents, or infer availability from the shell.
- It renders each lane with the model role, the agent, the executor or read-only mark, the description, the tools, the context modes, and the acceptance rule.
- It renders the advisor with its dispatchable flag. The advisor is a review channel.
- It requests the status when the link opens and when the operator presses refresh.
- One query runs at a time. A second press is refused with the reason.
- An unsolicited result is refused. Only the pending correlation identifier is accepted.
- The detail line carries the event identifier and the sequence of the applied result.

### dispatch

`src/dispatch.rs`. Build-only authoring of one bounded task packet.

- The disclosure states that this screen builds a packet and never spawns or executes an agent. The Host validates the request.
- The form takes the lane, the familiar, the task, an optional target, the acceptance criteria, and a risk level of low, medium, or high.
- The send action names its own refusal: no connection, no authenticated binding, a request already pending, or missing task text.
- It refuses an unknown risk selection locally and says so.
- The result shows the status, the lane, the model role, the agent, the event identifier, the sequence, the errors, the warnings, and the dispatcher outcome with its reason.
- The packet field renders each task with its name, its agent, and its text, plus the tool and the context.
- An applied receipt states plainly that nothing was executed.

### familiar_status

`src/familiar_status.rs`. The read-only view of the room spellbook.

- The disclosure states that the client does not read a spellbook path, infer a room, dispatch, spawn, or execute an agent.
- It renders the collective, the collective aliases, the spellbook aliases, and each familiar with its identifier, its lane, its aliases, and its description.
- It renders the source and whether the source is an alias.
- It reports every error the Host returned with the status.
- The refresh action names its own refusal, the same three reasons as the health screen.
- It accepts only the envelope that matches the pending correlation identifier, and it asks once when it holds nothing yet.

### paper_boat_receipt

`src/paper_boat_receipt.rs`. The state of the latest verified delivery.

- This module owns no node, no transport, no credential, and no body loading. PostgreSQL stays the authority and NATS stays delivery only.
- `apply` accepts one Host-ordered snapshot. A repeat returns false. A regressive order is an error, never a silent overwrite.
- `refuse` clears the receipt and records the reason. `degrade` only downgrades while no receipt exists, so a real delivery is never lost to a link problem.
- `card_fields` returns exactly the keys the center card accepts.
- A delivered receipt shows the time, the room, the record, the event, the stream sequence, and the integrity digest.
- Any other status writes the absent mark in every evidence field and states the phase with its diagnostic.

### lib

`src/lib.rs`, 17 lines. The extension entry point.

- It declares the ten modules.
- It registers `AthanorExtension` as the GDExtension library. The crate builds as a single dynamic library.
