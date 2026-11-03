# Orihon for OMP

This module translates persistent OMP entries into Orihon provenance records.
The Host owns mode, generations, authenticated attribution, and storage selection.
Rust owns redaction, echo truncation, storage validation, and record acceptance.

## Mode and consent

Use `/orihon status` to compare `thisSession` with `defaultForNewSessions`.
The status also shows the generation, source session, baseline, checkpoint, and `liveStatus`.
Use `/orihon off` to stop this session's book capture.
Use `/orihon log` to enable this session's book capture.
A mode command changes this session immediately and saves its spirit's default for new sessions.
Other open sessions retain their effective settings.
The Host snapshots settings during attachment. Reattachment preserves the snapshot for the same spirit and session.
New spirits default to log. Legacy room settings migrate once and preserve explicit off.
Use `/orihon fold MODEL:TAG` to go live with a local compactor model.
Use `/orihon fold hosted` to go live with the native Sonnet default.
Use `/orihon fold hosted PROVIDER/MODEL` to go live with an Anthropic or Codex model.
Fold is live mode. The Host folds the room chat into a message tree in the background.
Every new turn in a live session starts from the chat view instead of the old conversation. See "Live turns".
The latest explicit fold configuration becomes the room compactor configuration.
Status reads do not start jobs. Invalid configuration starts no job.
Fold mode keeps the same capture and checkpoint rules as log mode.
Hosted mode binds the actual OMP executable and optional Bun entrypoint.
The binding includes the operator's agent directory, profile, working directory, and ordered configuration paths.
Configuration values and credentials stay inside OMP.
Ambiguous launch flags or relative overlay paths cause a visible refusal.
Use absolute overlay paths, or an explicit absolute startup `--cwd`, when needed.

Rust owns the compactor. The compactor never executes tools.
The final scoped overlay disables controlled model fallbacks without changing global settings.
The operator's configured OMP route remains authoritative. The echoed model selector does not attest upstream execution.
Configuration refusals remain separate from capture failures and do not promise a retry.

Each source session stores a baseline for each spirit and generation through `pi.appendEntry`.
A new generation starts at the next user page after that baseline.
The adapter shows a notice when it creates the baseline.
It does not import earlier pages or records from an off interval.
An unchanged generation resumes from its existing checkpoint.

## Replay source

The adapter uses `sessionManager.getSessionId()` and the complete `getEntries()` result.
It follows entry `parentId` links to the source user page.
It does not use compacted context, branch-only replay, or a room-wide latest user.
Forked sessions establish their own baselines.

| Source | Orihon kind |
|---|---|
| Native user message with explicit user attribution | user |
| Native user steering or visible `collab-prompt` with user attribution | note |
| Assistant text blocks | talk |
| Persisted assistant tool calls | tool |
| Original attempted input from the `tool_call` hook | tool |
| Tool result text | echo |
| User bash or Python execution | tool and echo |
| Visible `irc:incoming` with a source sender and body | work |
| Visible `async-result` containing only task jobs | work |
| Visible mixed or non-task `async-result` | echo |
| Visible door message (`athanor-restart-continuation`, `athanor-chat-say`, `athanor-hallway-knock`) | work |

A combined report names its exact source job IDs.
It does not assign portions of combined text to individual workers.
A combined task report also carries `children`: one `{name, session}` for each job whose transcript header proves its child session.
The transcript is `<parent session stem>/<jobId>.jsonl`. A job without that proof has no child reference.
A door record keeps the speaker `OMP door <customType>` and starts its text with `[<customType>]`.
Records before the generation's first user page carry no `parentUserId`. The Host allocates one page for that prelude.
The Host supplies operator and spirit names for user, note, and talk records.
Other speaker labels identify harness sources, not authenticated people.
Replay retains the operator and spirit attribution from first acceptance.

A `task` tool record carries `foldText` beside its exact `text`.
The fold text keeps each task's own text and names the expanded `context` by byte size and SHA-256.
An attempted input that equals its persisted tool call carries a fold text that names that call instead of repeating it.
Non-echo text above 1 MiB splits into consecutive records `<sourceId>#part-<k>-of-<n>` of the same kind and page.
The split is stable: the same text always splits at the same character boundaries. Split records carry no fold text.

The adapter excludes thinking blocks, images, system messages, developer messages, generated user messages, and unrelated custom context.
It does not read artifact contents to expand a report.
Task children do not capture. Their transcripts stay private; `zoom` opens them on request.
`ATHANOR_REPLAY_MODE=1` disables book capture, baselines, and attempted-input entries.

## Original attempted inputs

The adapter installs its `tool_call` observer before its own input-revision hooks.
It stores a bounded custom entry before those hooks can revise the attempted input.
That entry has its own persistent ID and an observed timestamp flag.
Persisted assistant tool arguments remain separate source facts; they can contain execution summaries.

OMP can refuse a call before extension dispatch, including through TTSR or an existing approval denial.
Those paths do not provide this observer with the original attempted input.
An attempted input above 1 MiB is stored as several entries, each marked `part` and `parts`.
Replay turns each part into its own exact `tool` record. No text is cut.
Gap entries written before this split stay readable as explicit gap records.

## Delivery and failures

Each request carries `harness`, `sourceSession`, `generation`, and normalized `records`.
Normal batches contain at most 64 records and target 4 MiB of raw JSON.
A larger individual echo travels alone without adapter truncation.
Rust redacts and truncates the echo before it enforces the prepared batch limit.
Rust refuses a prepared batch above 4 MiB before it writes any record.
The adapter splits that refused batch at record boundaries and preserves acceptance order.

The adapter requires the matching generation and exact accepted source IDs before it stores a checkpoint.
It also checks accepted counts and the current source session before checkpoint persistence.
A failed or off response does not advance the checkpoint for that batch.
An earlier accepted batch can retain its checkpoint if a later batch fails.
A missing baseline, checkpoint source, or ancestor produces a visible gap.

Capture failures produce a UI warning and a console warning.
Capture failures do not abort ordinary turns in log mode.
Normal lifecycle boundaries retry from the last accepted checkpoint.
No new daemon or event spool runs.
Session attachment reads the mode before normal work starts.
OMP session persistence does not establish power-loss durability or an fsync guarantee.
Lifecycle callbacks await each capture batch and its checkpoint.
Portable shutdown invokes capture before it detaches the room.
Listener failures remain visible and do not prevent detachment.

## Live turns

A live session is a session in fold mode. `turn.ts` owns its requests.
Before each turn, `before_agent_start` appends the Host's `instructions` as the last system prompt part.
The same part serves the compactor, so turns and compactions share one stable prefix.

The first provider request of each agent loop opens the turn:

1. The adapter finds the turn's first message: the native user message, the dequeued batch's first message, or the door message.
2. It captures every record before that message. The current message and its turn stay uncaptured until the usual lifecycle boundaries.
3. It sends `orihon_turn` with the generation, `throughSourceId` (the last record the replay accepts before the turn, including a combined report's child records), and the prefix: the system prompt parts and the tool definitions.
   OMP hands tools over as ArkType callables; the adapter converts each through the schema's own `toJsonSchema` and refuses a non-object schema.
   After the first provider request, the prefix uses the tool set and order the model last received, as long as the provider API is unchanged; the registry order is the fallback.
4. The Host waits for every leaf before that frontier inside one deadline that starts at the context hook (25 seconds, under OMP's 30-second handler cap) and covers capture batches and the turn. A preparation that ends after its deadline authorizes nothing.
5. The adapter holds the view for the whole loop and replaces the request messages.

The replaced request is: the view, then OMP's standing notices, then the current turn from its first message.
Everything else before the turn goes: pages, steering, replies, tool calls, tool results, reports, doors, compaction and branch summaries, derived blocks, earlier views.
The kept notices are a fixed list in `turn.ts` (`HARNESS_NOTICES`): tool roster, xdev mount, session agent, eval prelude and state, plan, goal, vibe, context notes, checkpoint reminders.
Messages before the generation's first captured page were never recorded by consent. They are not sent; one notice names how many there are.
The current turn keeps every tool call, tool result, steering message, and anchored block.
A session seen live here that later loses its room identity refuses its requests until the room is reachable again.

The adapter refuses the request when the view is not ready: it notifies, aborts the request, and sends only a notice.
It never sends the old conversation instead of the view.
Refusals name the Host code, for example `orihon_view_not_ready` with its unbuilt and failed counts.
Send the message again when the compactor is ready. `/orihon status` shows `liveStatus`.

An existing log can need more catch-up time than one turn permits.
Ready merges get priority so the compactor can reduce its context during catch-up.
One leaf can run alongside merges when its context fits the 32,000-byte target.
Completed work updates and saves both views before the next jobs read their context.
The 16,000 to 32,000-byte range is a target, not a hard limit.
Missing parents and oversized model replies can keep the view above that range.

An ephemeral side turn (`/btw`) opens with the frontier at the end of the session.
An in-loop OMP compaction keeps the loop: the view stays first and stands for the turn's lost start.
In a fold session the adapter answers `session_before_compact` itself: a fixed note replaces the summary and no model call is made; the note is dropped from later requests.
The provider gate (`before_provider_request`) runs only inside an agent loop. Handoff, advisor, title, and compaction requests outside a loop pass unchanged.
Inside a loop the gate waits for the current preparation and refuses the request when the context hook was skipped or timed out.
An identity failure refuses only a session that was seen live here.
A loop ends at `agent_end`. Session start, switch, branch, and tree changes also end it.
The `prefixDigest` in the view details is the adapter-visible prefix identity; OMP can add provider blocks the extension cannot see.

## Task children

A task child receives the bounded chat view plus its task, never the parent's conversation.
The child's first request asks `orihon_turn` with purpose `subagent`. The Host returns the persisted view cut at its first unbuilt line and the instructions.
A combined task report names each child by its job id and resolves its transcript by the artifact id (`details.jobs[].agentId`), which OMP keeps when it suffixes a colliding job id.
The adapter prepends the instructions and the view to the child's first request and keeps them for the child's life.
A room that is not live returns `orihon_not_live`; the child then runs as before.
Any other failure refuses the child's request visibly.
Advisors, `/tan` clones, and eval agents receive no view.

## Read tools

The model tools are read-only `orihon`, `unfold`, `zoom`, and `date`.
`orihon` accepts page, sequence, and day bounds, or a separate search query.
`unfold` accepts an exact stored fold revision reference and optional source pagination.
`zoom` opens a view line: `id` and `n` give the two lines a line was made from; `n` 1 gives the message whole; `rawOffset` pages the audit text behind a fold text.
`zoom` with `name` opens a task child's own log. The Host returns the verified `{session, sourceSession}` references.
The adapter proves the transcript by its header under the reporting session's artifacts and pages it 30,000 bytes at a time.
A record above one page is clipped to its head and tail with an explicit `clipped` marker.
`date` returns the date and time of a message id.
Outside live mode the Host answers `orihon_not_live`.
The Host injects the room identity.
The response is provenance, not memory or canon authority.
Capture and mode operations remain internal adapter operations.
