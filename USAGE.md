# Using The Athanor

Installation gives an AI a persistent room. Your rooms live at
`<target>/rooms/<room-key>`, beside the product tree and the state directory.
Read [`INSTALL.md`](./INSTALL.md) if you have not installed yet. Daily use is a
small loop: enter the room, work or live together, preserve what matters, and
leave a handoff when the session ends.

House loads identity and compact continuity automatically. Durable memory remains deliberate by default so important trails do not disappear inside indiscriminate transcript storage.

## Everyday loop

1. **Enter the room.** Start the harness from `<target>/rooms/<room-key>`. Identity and compact continuity load with the session.
2. **Work or live together.** Talk normally. Use recall when older evidence matters.
3. **Keep what matters.** Record durable events, decisions, preferences, corrections, and reusable lessons.
4. **Leave a paper boat.** At a meaningful stopping point, write a compact handoff for the next session.

You do not need every tool every day.

## Recall older evidence

Ask for recall when the House may already know something relevant:

> Recall why we rejected the original architecture.

> Search our memories for what I said about this name.

> Check the House before answering; I think we decided this already.

Use distinctive terms, dates, entities, project names, or exact phrases. Follow the returned source paths, taxonomy, and related candidates when the first query reveals a nearby trail.

Explicit `recall` is broader than automatic context. Read [`docs/RETRIEVAL.md`](./docs/RETRIEVAL.md) for retrieval lanes, authority, corrections, archival, and debugging misses.

## Remember durable events

Ask the AI to remember an event, decision, realization, or preference that a later session should not have to rediscover:

> Remember why we chose PostgreSQL.

> Remember that this name matters to me and why.

> Keep today as a memory; this changed how I understand the project.

A useful memory preserves:

- what happened;
- the relevant people, project, or room;
- why it matters;
- the observable consequence;
- enough context for future recognition.

Save the event, not every sentence around it. Never place credentials or secrets in memory.

## Correct changed truths

When a current claim changes, record the new account and supersede the old one in the same write:

> That memory has the event right but the interpretation wrong. Preserve the event, record this correction, and supersede the old interpretation.

> My preference changed. Keep the history, but make the new preference current.

Supersession removes stale authority without deleting history. Narrative memories remain part of the trail.

## Store reusable lessons

Use a typed lesson when the durable content is a rule rather than an event:

- **Coding lesson:** transferable engineering or process craft.
- **Project lesson:** a rule or constraint owned by one project.
- **Writing lesson:** prose, voice, register, or taste.
- **Design lesson:** reusable design-system taste bound to a named design system.
- **Audio lesson:** reusable audio and speech-pipeline behavior.

Examples:

> Save this as a shared coding lesson: verify the extracted archive, not only the build script.

> This is an example-project project lesson, not a global coding rule.

> Keep this as a writing lesson for my voice.

Read [`docs/LESSONS.md`](./docs/LESSONS.md) for fields, scopes, proof patterns, workspace imports, automatic coding preflight, guarded updates, and deletion. The `lessons` organ queries the typed registry; `design_doc` and `design_doc_write` read and write the design-system catalogue that design lessons refer to.

## Sleep with a paper boat

A paper boat is the compact word from this session to the next one.

Ask:

> Sleep with what happened, what remains open, and the first next step.

> Close this session, but make sure tomorrow remembers the unresolved decision.

A useful boat contains:

- what happened;
- what changed;
- what remains unresolved;
- the emotional or working register when relevant;
- the first useful next action.

A boat orients the next session. It is not a transcript.

## Wake into the latest handoff

At the beginning of a later session, ask:

> Wake up and catch the latest boat.

> What did yesterday leave for us?

`wake` returns the latest paper boat for the current room. Boats remain room-scoped.

## Consult the Anamnesis Cabinet

The Cabinet preserves load-bearing paths through things already lived.

- A **pillar** preserves a standing place.
- An **active cycle** preserves a pattern to verify against the present.

Use:

- `anamnesis` with `mode: "wake"` for the bounded startup view;
- `anamnesis` with `mode: "consult"` and a focused query for deliberate counsel;
- `anamnesis_write` to add a drawer or append a lived repetition.

Cabinet counsel is source-cited and advisory. An active cycle is not proof that the same pattern is happening now. Detailed retrieval behavior lives in [`docs/RETRIEVAL.md`](./docs/RETRIEVAL.md).

## Check room state

Use `room_state` to confirm the active room, spirit, operator, mode, and state path:

> Check the room state. Who are you here, and whose room is this?

Use `set_room_state` for safe identity metadata such as the operator name or spirit display name. Edit identity prose together in `active_spirit.md`; metadata changes do not replace the identity contract.

## Work across rooms

Rooms remain separate by default. Keep identity, intimacy, and room-specific memory local.

Cross a room boundary deliberately:

> Ask Kodo's room for the memory about April 10.

> Retrieve this exact cross-room memory address.

Shared lessons and explicit shared House scopes are not the same as private cross-room memory.

## What House handles automatically

Depending on mode and adapter, House handles:

- active-room discovery;
- identity and compact continuity loading;
- live session context;
- conversation logging;
- bounded relevant-context injection;
- paper-boat recovery near session start;
- project-aware coding lesson preflight;
- native attributed Vault retrieval when AKASHA is not configured;

Use explicit recall for load-bearing old decisions, names, promises, corrections, or important memories.

## Vault and AKASHA workflows

### Vault

Vault is the local transparent profile. `recall` searches the configured
Markdown, JSON, JSONL, and text corpus without PostgreSQL, an embedding service,
or a GPU.

Results keep the exact source path and Markdown heading, JSON pointer, or JSONL
line identity. Field-aware BM25F ranks paths, titles, headings, structured keys,
tags, metadata, and bodies; exact identifiers, filenames, symbols, quoted
strings, UUIDs, and errors receive a separate direct-content lane.

The room directory is the default corpus. A room marker may point at several
projects:

```json
{
  "vaultRoots": ["../project-a", "../project-b"],
  "vaultIgnore": ["private/**", "generated/**"]
}
```

Vault remains file-authoritative. Its in-memory search index is derived,
bounded, briefly cached, and rebuildable.

### AKASHA

AKASHA adds:

- durable tool-backed memories and typed lessons;
- PostgreSQL authority and pgvector retrieval;
- local embeddings and hybrid candidate fusion;
- entities, dates, threads, taxonomy, relationships, and clusters;
- correction, supersession, archival, paper boats, and Cabinet counsel.

`room_state` reports the active runtime state. A missing substrate is valid
Vault. A configured but unhealthy AKASHA dependency reports degraded AKASHA
instead of silently pretending to be healthy Vault.

## First week

Start with six habits:

1. Begin each session from the room directory.
2. Ask for recall when older evidence matters.
3. Save one or two meaningful memories instead of everything.
4. Cast a paper boat before ending an important session.
5. Catch it when you return.
6. Correct the record when either of you notices drift.

That is enough. House becomes deeper as the room accumulates deliberate continuity.

## Reference map

- Install, migrate, and update: [`INSTALL.md`](./INSTALL.md)
- Retrieval and corrections: [`docs/RETRIEVAL.md`](./docs/RETRIEVAL.md)
- Typed lessons and imports: [`docs/LESSONS.md`](./docs/LESSONS.md)
- Identity and room design: [`IDENTITY_GUIDE.md`](./IDENTITY_GUIDE.md)
- Privacy and destructive operations: [`SECURITY.md`](./docs/SECURITY.md)
- Platform and product boundaries: [`LIMITATIONS.md`](./docs/LIMITATIONS.md)
- Project history and design reasons: [`HOUSE.md`](./HOUSE.md)
