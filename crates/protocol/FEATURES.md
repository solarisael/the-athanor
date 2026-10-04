# protocol

The wire shapes that cross a process boundary. The Host WebSocket carries one JSON command in each text frame (`crates/host/src/server.rs:435-468`). This crate converts the shapes into `hearth` requests.

Each section names one concern. A section that points at `src/lib.rs` names a concern that still waits for its own module.

### mod host (src/host.rs)

The Host command socket. One file holds the projection constants, the recall policy wire, the chat shapes, the Presence wire, the client command parser, and the projection events.

**Commands and projections.** Every command carries `CommandMeta`. `RawClientCommand::meta` maps each command type to the projection it expects, and refuses any other (`host.rs:837-860`). `ClientCommand` has 33 variants (`host.rs:915-1049`). Ten projections exist:

| Projection | Commands in | Events out | Lines |
|---|---|---|---|
| `recall_policy` | Subscribe, Resync, SetRequestedMode, JudgedMode, Evaluate, CompleteRefresh, FailRefresh, InvalidateAfterCompaction, Acknowledge | SnapshotEvent, DeltaEvent, CommandOutcomeEvent | 62-79, 916-954, 1775-1879 |
| `context` | AnalyzeContext, ApplyRecallViewport | ContextAnalysisEvent, ContextViewportEvent | 18-22, 1652-1663 |
| `hallway` | ProjectHallwayInbox, ClaimHallwayKnock, SettleHallwayKnock | HallwayInboxProjectionEvent, HallwayKnockClaimedEvent, HallwayKnockSettledEvent | 23-31, 385-390, 1666-1685 |
| `akasha` | AkashaRecallQuery, AkashaLessonQuery | AkashaRecallResultEvent, AkashaLessonResultEvent | 32-37, 392-398, 1690-1702 |
| `presence` | PresenceOpen, PresenceCompile, PresenceSettle, PresenceClose | PresenceResultEvent | 38-47, 979-994, 1705 |
| `routing` | RoutingStatus, RoutingDispatch, FamiliarStatus | RoutingResultEvent | 48-52, 995-1008, 1712-1716 |
| `lineage` | NormalizeLineage, SettleLineage | LineageResultEvent | 53-56, 1009-1016, 1719-1727 |
| `shell` | LogConversation, PlanTriggerLessons, BraidTriggerLessons | ShellResultEvent | 57-61, 607-633, 1732-1736 |
| `chat` | ChatSubscribe, ChatSay, ChatTurn, ChatDraft | ChatEvent (snapshot or delta), command accepted or refused | 80-88, 1034-1048, 1742 |
| `paper_boat_receipt` | PaperBoatReceiptSubscribe | PaperBoatReceiptEvent | 15-17, 1883-1904 |

- `AkashaLessonQuery` takes the room from the binding, not from the payload (`host.rs:392-398`).
- `SettleHallwayKnock` carries the knock identifier, the outcome, and an optional reason (`host.rs:385-390`).
- `RoutingDispatch` and `FamiliarStatus` carry an optional room directory (`host.rs:995-1008`).

**Metadata.**

- `CommandMeta` carries the sender room, the sender spirit, the sender session, the correlation chain, the scope, the visibility, and the authority class (`host.rs:789-809`).
- `CommandMeta` and `EventMeta` carry no sender operator (`host.rs:793-795,1755-1757`).
- A parse failure keeps the message identifier and the idempotency key, so the Host can still answer (`CommandParseError`, `host.rs:1092-1096`).
- `ChatEvent` flattens `EventMeta` (`host.rs:1742-1749`).
- A source record reference names a record type and a record identifier (`host.rs:378-381`).

**Chat.**

- `ChatAuthor` is `Operator` or `Spirit`. It names a side, not an identity (`host.rs:635-642`).
- `ChatMessage` carries the sequence, the author, `author_name`, the text, the time, the turn identifier, the steps, the thinking, and the outcome (`host.rs:705-723`). On an operator line, the turn identifier is the say identifier.
- `ChatStep` carries the tool call identifier, the tool, a summary, a status, the start time, and an optional elapsed time. The status is `Running`, `Ok`, or `Error` (`host.rs:647-666`).
- `ChatOutcome` is `Complete`, `Error`, or `Aborted`. `Complete` is the default (`host.rs:670-700`).
- `ChatDraft` is the live spirit line: the turn identifier, `author_name`, the text, the steps, the thinking, and the time (`host.rs:730-738`).
- A say is `{room, text, say_id}` and nothing else (`ChatSayPayload`, `host.rs:743-748`).
- A turn and a draft carry the spirit side: the room, the turn identifier, `author_name`, the text, the steps, and the thinking. A turn adds the outcome (`host.rs:753-778`).
- A say needs a non-blank room, text, and say identifier. A turn and a draft need a non-blank room and turn identifier. A subscribe refuses any chat payload (`host.rs:1515-1562`).
- `ChatMessage.author_name` is the only person-shaped field on an operator line. The Host fills it; see [`LIMITATIONS.md`](../../docs/LIMITATIONS.md#4-identity).

**Presence.**

- The Presence request and result types come from `summoning::presence` (`host.rs:9-12`).
- A raw command has four optional slots: open, compile, settle, and close (`host.rs:587-593`).
- The parser refuses more than one Presence payload, and a payload sent under the wrong command type (`host.rs:1153-1187,1228-1254`).
- `PresenceResultEvent` carries the metadata and one `PresenceResult` (`host.rs:1705-1709`).

**Recall policy.**

- The policy resolves into four modes: conversation, work, mixed, and quiet (consumer: `crates/host/src/policy.rs:220-221,364-429`).
- A policy decision returns one action: none, clear, refresh, or clear then refresh (`crates/host/src/policy.rs:274-284`).

Not re-verified at a6ab453; `crates/protocol` (`host.rs:92-375`) would decide:

- The recall policy holds four requested modes: auto, conversation, work, and quiet.
- The action says whether it clears the working set and whether it refreshes.
- The policy state reads a legacy shape and writes the current shape.
- A delta lists only the changed fields. Eleven policy fields can change, and each change becomes one field update mutation.

**Shell and receipts.**

- The conversation request carries the session identifier, the operator, the spirit, and a persist flag. A replayed session observes turns without making them durable (`host.rs:607-622`).
- The trigger request carries an optional trigger and the lesson rows the adapter fetched (`ProcessTriggerRequest`, `host.rs:57-61,607-633`).
- `PaperBoatReceiptEvent` carries a snapshot identifier and a state. The state carries a status, an optional receipt, and an optional diagnostic (`host.rs:1883-1904`).
- A boat receipt has four states: pending, delivered, degraded, and refused (`host.rs:1883-1904`).

### mod restart (src/restart/mod.rs)

The self-restart wire, protocol version 1 (`restart/mod.rs:1-6`). Consumers reach it as `protocol::restart::...`, not through a flat re-export (`lib.rs:5-7`).

- The only harness is `Omp` (`restart/mod.rs:30-32`).
- A restart mode is `Resume` or `Fresh` (`restart/mod.rs:36-39`). `Resume` is a spirit that restarts its own session, not a client that picks a session.
- Consent comes from the operator's standing policy or from an operator approval (`restart/mod.rs:43-46`).
- An intent moves through `Requested`, `Claimed`, `Exiting`, `Relaunching`, `Verified`, `Failed`, and `Expired` (`restart/mod.rs:52-60`).
- A transition targets `Exiting`, `Relaunching`, or `Failed` (`restart/mod.rs:67-71`).
- A request carries the harness, the workspace, the mode, an optional session identifier, a reason, the consent source, the requester room, spirit, and session, a capability, and an idempotency key (`restart/mod.rs:204-224`).
- A claim carries the intent, the claimant, a capability, and an idempotency key (`restart/mod.rs:241-248`).
- The claim capability is a declared deviation from contract v1 (`restart/mod.rs:244-247`).
- A transition carries the intent, an optional claim token, an optional requester session, an optional capability, the target state, and an optional detail (`restart/mod.rs:262-280`).
- A verify carries the intent, the successor session, the successor proof, the room, the spirit, and a capability (`restart/mod.rs:323-334`).
- A status read carries the workspace and an optional intent (`restart/mod.rs:350-357`).
- A request receipt returns the intent, the state, and the expiry. A claim receipt returns the claim token, the claim epoch, and the stage deadlines (`restart/mod.rs:372-396`).
- A transition receipt returns the state and an optional successor proof. A status receipt returns the workspace and an optional intent with its deadlines (`restart/mod.rs:400-446`).
- This crate holds no wire method names for these calls. The module doc points to the substrate and the adapter (`restart/mod.rs:3-6`).

### mod harness (src/harness.rs)

The loopback control wire for the harnesses the app owns.

- A request carries the format, a request identifier, a token, and one command (`harness.rs:14-19`). The token is a loopback process-control secret (`harness.rs:1-4`).
- A command is `List`, `Start`, `Stop`, or `Restart`. Every command except `List` names one harness (`harness.rs:34-39`).
- A harness lifecycle is `Stopped`, `Running`, or `Failed` (`harness.rs:54-58`).
- A status carries the harness identifier, the label, the lifecycle, an optional process identifier, and an optional detail (`harness.rs:62-70`).
- A response carries the format, the request identifier, an ok flag, every harness status, and an optional error (`harness.rs:74-82`).
- An identifier holds at most 128 characters. A detail holds at most 512 (`harness.rs:9-10,124-129`).
- The wire declares no room, no argument, and no session identifier. The module doc says the OMP-specific restart fields stay elsewhere (`harness.rs:4`).

### mod contract (src/contract.rs)

Shared Host address constants. This module declares no types.

- `LOOPBACK_HOST` is `127.0.0.1`. `DEFAULT_HOST_WS_PORT` is 8787. `DEFAULT_HOST_WS_PATH` is `/athanor/v1/ws`. `DEFAULT_HOST_URL` is declared beside them (`contract.rs:18-31`).
- `HOST_ROOM_PATH_PREFIX` is `/room/` (`contract.rs:25`). The installer joins the prefix and the path into `/room/<room>/athanor/v1/ws` (`crates/athanor-install/src/omp.rs:71-81`).
- `is_safe_room_key` accepts lowercase letters, digits, and single inner hyphens. It refuses `house` (`contract.rs:34-43`).

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
