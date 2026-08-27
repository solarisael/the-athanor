# protocol

The newline-delimited JSON wire protocol, version 1. This crate carries the shapes that cross a process boundary, and converts them into `core` requests.

Each section names one concern. A section that points at `src/lib.rs` names a concern that still waits for its own module.

### mod host (src/host.rs)

- The Host surface has four concerns in one file: subject names, the recall policy, the client command parser, and the projection events. Each one can carve out.
- Constants name every websocket path, stream, subject, and projection identifier the Host publishes.
- Seven projections exist: context, hallway, routing, lineage, shell, recall policy, and boat receipts.
- The recall policy holds four requested modes: auto, conversation, work, and quiet.
- The policy resolves into four modes: conversation, work, mixed, and quiet.
- A policy decision returns one action: none, clear, refresh, or clear and then refresh.
- The action says whether it clears the working set and whether it refreshes.
- Policy facts carry the query route, the active project, the token count, and two flags for the working set and the tool evidence.
- The policy state reads a legacy shape and writes the current shape, so an older client keeps working.
- Recovery state is typed. A pending recovery carries its terms.
- A delta lists only the fields that changed between two states.
- Eleven policy fields can change. Each change becomes one field update mutation.
- `parse_client_command` turns one JSON value into one of 26 typed commands.
- A parse failure keeps the message identifier and the idempotency key, so the Host can still answer.
- Command metadata carries the sender room, the sender spirit, the sender session, the correlation chain, the scope, the visibility, and the authority class.
- Event metadata adds the sequence and the state hash.
- Snapshot, delta, and outcome events all flatten the same metadata.
- A boat receipt projection carries the record identifier, the stream sequence, and an integrity hash.
- A boat receipt has four states: pending, delivered, degraded, and refused.
- A source record reference names a record type and a record identifier.
- The conversation request carries the visible window, the attributions, and a persist flag. A replayed session observes turns without making them durable.
- The trigger request carries the matched trigger and the lesson rows the adapter fetched.

### envelope (src/lib.rs, lines 34-43, 760-814, 1449-1484, 2436-2438, 2467-2473, 2538-2546)

- The protocol version is 1. A request with any other version refuses.
- A request carries the version, an identifier, a method name, and raw parameters.
- `parse_line` decodes one line. A decode failure returns a malformed error.
- A response carries exactly one branch: a result or an error. Both branches or neither branch refuses.
- The reader rejects any extra key beside `result` and `error`.
- `success` and `error` build the response and stamp the version.
- The protocol error has four kinds: malformed, version mismatch, unknown method, and invalid parameters.
- Each kind maps to a stable error code, and all four are not retryable.
- The error body keeps `details` as raw JSON, so an older producer stays readable.

### request_parsing (src/lib.rs, lines 2153-2439)

- Twenty-five accessors turn one envelope into one domain request.
- Each accessor checks the version first, then the method name, then the parameters.
- A wrong method name returns an unknown method error and keeps the name.
- Parameter decoding failures become invalid parameter errors, and keep the decoder message.
- Every parameter shape denies an unknown field, so a typed field can never pass silently.
- The health accessor also checks that the backup age is finite and above zero.

### diagnostics (src/lib.rs, lines 816-1237)

- Typed diagnostics ride inside the version-1 `details` object, so an older reader keeps working.
- A category names the broad failure area. Twelve categories exist.
- A stage names the exact point of failure.
- An owner names the component, and may add a path and a symbol.
- Evidence carries a kind, a summary, and machine-readable data.
- A target names an exact thing to inspect. Seven target kinds exist, from a file to a service.
- A next check gives one ordered follow-up action, with an optional target and an expected value.
- An execution record answers three questions: did the request dispatch, what happened to the write, and is a retry safe.
- The write outcome has four values: not started, rolled back, committed, and unknown.
- The retry policy has four values: safe now, after a change, reconcile first, and never.
- Unknown detail keys survive a decode and a re-encode.
- Every builder redacts. A caller cannot leak a secret by accident.
- Text redaction catches a bearer token, a basic credential, an authorization header, a token parameter, a password parameter, and an authenticated URL.
- Key redaction catches 13 exact key names, and any key that ends with password, secret, or token.
- Redaction walks arrays and nested objects.

### giga_wire (src/lib.rs, lines 2548-4031)

- One macro guards each known string. A parameter decode fails when the domain rejects the value.
- Ten guarded strings exist: visibility, source type, event type, risk, candidate kind, authority, review state, finish outcome, queue state, and promotion authority.
- `RequiredNullable` states the difference between a missing field and an explicit null.
- Eight lifecycle parameter shapes match the eight domain event types.
- The lifecycle enum decodes without a tag, so the field set selects the shape.
- Twelve GIGA requests cross this wire: ingest, conversation ingest, claim, finish, replay, queue maintenance, review, tool review, promote, tool promote, candidate list, and health.
- Two promote paths exist: the strict one, and the tool one that reads a target by kind.
- An ingest result reports a disposition, and writes it as two booleans for an older reader.
- A candidate store result reports its disposition the same way.
- The health result reports whether GIGA is on, whether the store answers, the queue depth, counts by kind and state, and the classifier identity.
- Each result converts back from the matching domain receipt.

### remember_wire (src/lib.rs, lines 103-156, 1239-1330, 1486-1623, 2441-2465)

- One parameter shape covers a memory write and all five lesson writes.
- A lesson write and a memory write pick different room key constructors, so only a memory reaches the commons.
- A thread continuation carries the previous memory identifier as text, and converts to a number.
- Defaults exist for the backup flag and the similarity thresholds.
- The result hides the room and the source path for a lesson write.
- The result reports the lesson identifier only when it is not zero.
- The result always states that the authority is Postgres.

### recall_wire (src/lib.rs, lines 179-194, 276-540, 1335-1352)

- Parameters carry the room, the query, both lane counts, both similarity floors, and the decay flag.
- Every count and every floor has a default, so a caller may send only the room and the query.
- The result reports the query, whether anything was found, and the source.
- The result holds six evidence lists: candidates, canon matches, semantic chunks, content chunks, date matches, and query dates.
- A candidate carries its thread neighbours, so a reader can see the thread around a match.
- A canon match carries the entity, and a canon file carries the path.
- Cluster staleness and cluster resonance ride as telemetry beside the results.
- A memory handle lets the caller re-open one exact memory.
- Warnings ride with the result, so a partial answer is still readable.

### recall_presentation (src/lib.rs, lines 542-696)

- The presentation layer is a separate shape from the raw result.
- Every list has a presentation twin: candidate, canon match, raw chunk, date match, taxonomy, cluster profile, cluster resonance, memory handle, and vault.
- A presentation entry states the authority and the superseding identifier, so a reader never treats stale text as current.
- Empty fields drop out of the wire, so a presentation stays small.
- A cluster nudge is a single line of text for the reader.

### recall_viewport (src/lib.rs, lines 698-720)

- The viewport runs in two modes: automatic and manual.
- The result keeps the candidates it kept, and lists every suppression with its identity and its reason.
- Diagnostics count what was kept, what was suppressed, and how many times each reason fired.
- The result carries the full presentation beside the viewport decision.

### canon_wire (src/lib.rs, lines 1803-1979)

- Write parameters carry the attribution, the pointer files, the aliases, and the superseded identifiers.
- Read parameters carry an optional identifier and an optional name.
- Identifiers arrive as text and convert to unsigned numbers, so a large identifier survives JSON.
- The entity result carries the whole canon body, and the read result may carry its history.
- The write result reports the new identifier and every superseded identifier.

### anamnesis_wire (src/lib.rs, lines 1624-1801, 2474-2536)

- The read limit defaults to 10.
- Write parameters split into add and append. Each converts to its own domain request.
- A repetition parameter shape carries the number, the date, and the three text fields.
- One result shape serves a read, an add, and an append.
- The write result names the operation, so a reader can tell an add from an append.
- The write result always states that the authority is Postgres.

### boats_wire (src/lib.rs, lines 62-93, 1981-2151)

- Sleep parameters carry the room, the body, and a backup flag that defaults to on.
- Wake parameters carry only the room.
- The wake result builds a ready-to-read wake context as one hidden reminder.
- The wake body clips at 6000 characters, and says how many characters it dropped.
- An unboated memory raises a stale boat warning inside the wake context.
- The warning lists each unboated memory by identifier and title, and tells the reader to recall them first.
- The wake result says when the unboated list truncated.

### cluster_wire (src/lib.rs, lines 236-274, 722-758, 1353-1447)

- Parameters carry the room, the operation, a dry run flag, an if-stale flag, and a cluster count that defaults to 8.
- The operation text converts into a check request or a rebuild request.
- The result reports the operation, whether it was a dry run, and whether it rebuilt.
- The result carries the staleness numbers and every cluster summary.
- Telemetry shapes let a recall answer carry the same staleness numbers.
- A unit fraction reader refuses a value that is not finite and between 0 and 1.

### vault_recall_wire (src/lib.rs, lines 196-223)

- Parameters carry the room, the room directory, and the query.
- All three must carry text. A blank value returns an invalid parameter error that names the field.
- These parameters stay as they are. There is no domain request behind them.

### substrate_wire (src/lib.rs, lines 45-60, 2410-2435)

- Health parameters carry a skip flag for embedding and a maximum backup age that defaults to 24 hours.
- The backup age must be finite and above zero.
- Migration parameters are empty, and deny every field.
- Neither family has a result shape here. The caller builds its own answer.
