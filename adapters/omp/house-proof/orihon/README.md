# Orihon for OMP

This module translates persistent OMP entries into Orihon provenance records.
The Host owns mode, generations, authenticated attribution, and storage selection.
Rust owns redaction, echo truncation, storage validation, and record acceptance.

## Mode and consent

Use `/orihon status` to show the mode, generation, source session, baseline, and checkpoint.
Use `/orihon off` to stop book capture.
Use `/orihon log` to enable book capture.
The Host defaults to log and preserves an explicit off setting.
Fold mode is not available.

Each source session stores a baseline for each Host generation through `pi.appendEntry`.
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

A combined report names its exact source job IDs.
It does not assign portions of combined text to individual workers.
The Host supplies operator and spirit names for user, note, and talk records.
Other speaker labels identify harness sources, not authenticated people.
Replay retains the operator and spirit attribution from first acceptance.

The adapter excludes thinking blocks, images, system messages, developer messages, generated user messages, and unrelated custom context.
It does not read artifact contents to expand a report.
A source without explicit user attribution cannot establish a user page.
Worker sessions do not create book pages. Their visible reports enter through the parent session.
`ATHANOR_REPLAY_MODE=1` disables book capture, baselines, and attempted-input entries.

## Original attempted inputs

The adapter installs its `tool_call` observer before its own input-revision hooks.
It stores a bounded custom entry before those hooks can revise the attempted input.
That entry has its own persistent ID and an observed timestamp flag.
Persisted assistant tool arguments remain separate source facts; they can contain execution summaries.

OMP can refuse a call before extension dispatch, including through TTSR or an existing approval denial.
Those paths do not provide this observer with the original attempted input.
An oversized attempted input leaves an explicit source gap without its contents.
The gap prevents replay from advancing past that entry.

## Delivery and failures

Each request carries `harness`, `sourceSession`, `generation`, and normalized `records`.
Normal batches contain at most 64 records and target 4 MiB of raw JSON.
A larger individual echo travels alone without adapter truncation.
Rust redacts and truncates the echo before it enforces the prepared batch limit.
The adapter rejects non-echo text above 1 MiB.
Rust refuses a prepared batch above 4 MiB before it writes any record.
The adapter splits that refused batch at record boundaries and preserves acceptance order.
An oversized single record remains a visible gap.

The adapter requires the matching generation and exact accepted source IDs before it stores a checkpoint.
It also checks accepted counts and the current source session before checkpoint persistence.
A failed or off response does not advance the checkpoint for that batch.
An earlier accepted batch can retain its checkpoint if a later batch fails.
A missing baseline, checkpoint source, or ancestor produces a visible gap.

Capture failures produce a UI warning and a console warning.
Capture failures do not abort ordinary turns.
Normal lifecycle boundaries retry from the last accepted checkpoint.
No new daemon or event spool runs.
Session attachment reads the mode before normal work starts.
OMP session persistence does not establish power-loss durability or an fsync guarantee.
Lifecycle callbacks await each capture batch and its checkpoint.
Portable shutdown invokes capture before it detaches the room.
Listener failures remain visible and do not prevent detachment.

## Read tool

The only model tool is the read-only `orihon` tool.
It accepts `afterSeq`, `untilSeq`, `turnNo`, `day`, and `limit`.
The Host injects the room identity.
The response is provenance, not memory or canon authority.
Capture and mode operations remain internal adapter operations.

