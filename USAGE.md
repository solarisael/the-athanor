# Using Solarisael House

Installation gives your AI a room. This guide explains how the two of you actually use it.

The House is deliberately not completely automatic. It can load identity and continuity, log conversations, and surface relevant context, but explicit requests remain useful. You decide what deserves to become durable memory, when to investigate the archive, and when a session is ready to close.

## The everyday loop

A normal House session has four simple movements:

1. **Wake into the room.** Start the AI harness from the room directory. The room identity and compact continuity load with the session.
2. **Work or live together.** Talk normally. Ask for recall when older context matters.
3. **Keep what matters.** Tell the AI to remember durable events, decisions, preferences, or lessons.
4. **Leave a paper boat.** At a meaningful stopping point, ask the AI to sleep with a compact note for the next session.

You do not need to invoke every tool every day.

## Recall: look for an older trail

Ask for recall when either of you is uncertain about something the House may already know:

> Recall why we rejected the original architecture.

> Search our memories for what I said about this name.

> Do you remember where we left this project?

> Check the House before answering; I think we decided this already.

Good recall queries use a few sharp, distinctive terms. If the first search misses, rephrase it, use dates or named entities, or follow the taxonomy and related trails returned by the tool. A miss is not proof that the memory does not exist.

Recall is retrieval, not revelation. Check cited sources before treating an important recollection as fact.

OMP applies a conservative viewport only to **automatic** recall. Casual acknowledgments, operational commands, and low-information chatter may retrieve nothing; candidates supported only by conversational glue or weak evidence are suppressed, and repeatedly injected candidates receive a session saturation penalty. Explicit use of `recall` remains broad and unchanged.

Named-entity routing is data-backed rather than phrase-backed. OMP resolves canonical names and aliases against the existing `named_entities` substrate for the current room plus shared House scopes, then passes those structured matches to the router. Capitalization alone is never proof of identity, and public regression tests use synthetic entities rather than private names or dialogue. If the substrate is unavailable or a name has not been indexed, entity resolution fails open and explicit `recall` remains available.

## Anamnesis: consult the Cabinet

The Cabinet is the House's path-compression organ. It is not another memory
catalogue. A memory says what happened; a Cabinet drawer preserves a
load-bearing path through something already lived.

At the start of a fresh OMP session, the House may load bounded wake-tier
Cabinet counsel once for the current room. **Pillars** are standing places.
**Active cycles** are prior patterns to verify against the live turn, never
proof that the same pattern is happening now. Cabinet context is advisory,
source-cited, and fail-open: an unavailable Cabinet must not block the session.

Use `anamnesis` with `mode: "consult"` and a focused query when you deliberately
want the relevant drawers. `mode: "wake"` reads the same bounded startup view
on demand. Consult searches the current room plus shared House scope through
titles, shapes, tags, canon links, and drawer text, and includes at most the
latest three lived repetitions for each cycle.

Use `anamnesis_write` only when a path genuinely deserves a Cabinet drawer or
when a cycle has been lived again. `add` preserves the writer's existing hard
refusals and fidelity declaration (`record` or `raw-material`);
`append-rep` requires the repetition number, what happened, the portal pull,
what became lighter, and source paths. Multiline bodies cross the Windows/WSL
boundary through temporary UTF-8 files rather than shell interpolation.


## Remember: preserve something deliberately

Ask the AI to remember when something happened that tomorrow should not have to rediscover:

> Remember why we chose PostgreSQL over the service mesh.

> Remember that this name matters to me and why.

> Keep today as a memory; this changed how I understand the project.

A useful memory records the event, decision, or realization and enough context to understand why it matters. Do not save every sentence. Repeated trivia makes important trails harder to find.

Never put passwords, API keys, access tokens, or other secrets into memories.

### Keep the trail current

When a current-state claim changes, keep the new memory and mark the old one as superseded in the same write. At the substrate boundary that is `record_memory.py --supersedes <id>`; repeat the flag when one update replaces several older rows. This is for “what is true now,” not for flattening the past.

When a session or narrative has accumulated enough history to compress, run `digest_pass.py` manually. It proposes supersede/archive sets for review; use `--apply` only after you have looked at them. Narrative memories should become arcs by proposal, while state claims can be superseded directly. Nothing is hard-deleted: superseded and archived rows remain recoverable, superseded rows are hard-demoted, and archived rows are excluded from ordinary retrieval. Use `--include-archived` only when you deliberately need the older trail. If a database has not yet received the archival columns, retrieval fails open and remains available without them.

This keeps the daily rhythm light: remember what matters, correct current state when it moves, and digest a season of memories rather than pruning in the middle of a live conversation.

## Lessons: keep reusable knowledge in the right store

Not everything belongs in ordinary memory. The House supports purpose-specific lesson stores:

- **Coding lesson:** a reusable engineering or process rule that should travel between projects, ideally with an observable proof pattern
- **Project lesson:** a rule, constraint, or decision that belongs to one named project
- **Writing lesson:** a durable prose, voice, register, or taste preference
- **Audio lesson:** a reusable rule for an audio or speech pipeline

Tell the AI what kind of lesson it is:

> Save this as a shared coding lesson: verify the staged set before every commit.

> This is a Multistock project lesson, not a global coding rule.

> Keep this as a writing lesson for my voice.

Before a risky engineering operation, the AI can consult `coding_lessons` for the relevant shape, such as `process`, `testing`, `naming`, or `refusal`.

### Use the House as a living skills library

Developers often accumulate `SKILL.md` files, agent-rule files, postmortems, snippets, and framework notes. The substrate can absorb any number of these sources as coding lessons. The useful unit is not “one source file becomes one giant lesson.” Let the AI read the source, compare it with the current codebase and toolchain, and extract small rules that each answer one question:

1. **When should this fire?**
2. **What should the AI do or avoid?**
3. **What observable evidence proves it followed the rule?**
4. **Is the rule transferable, room-specific, or tied to one project?**

Ask the AI to adapt rather than blindly copy:

> Read these skill files. Extract only the rules that still apply to our tools and environment. Split compound instructions into independently retrievable coding lessons, preserve important cautions, reject obsolete or contradictory advice, and show me the proposed lessons and scopes before storing them.

For a large import, work in bounded batches. Deduplicate by meaning, not merely by title; keep provenance in the lesson body or tags when it matters; and test a sample retrieval before importing the next batch. A smaller set of sharp lessons retrieves better than hundreds of copied paragraphs with overlapping wording.

### Coding lesson field contract

Use the `remember` tool with `kind: "coding-lesson"`. The tool writes a logical `coding_lessons` row with these public fields:

| Tool field | Meaning | Guidance |
|---|---|---|
| `title` | Stable, short lesson name | Required. Reusing the same title within the same scope and project updates that lesson rather than creating a duplicate. |
| `body` | The actual engineering rule | Required. State the action, boundary, and reason plainly; Markdown is allowed. |
| `shape` | Retrieval taxonomy | Use a compact reusable category such as `process`, `testing`, `naming`, `tooling`, `security`, or `refusal`. Prefer an existing shape when one fits. |
| `scope` | Who should receive it | `shared` makes it available across rooms; a room key keeps it local to that room. Omitted scope defaults to `shared`. |
| `project` | Project label or provenance | Optional for coding lessons. Use a stable project key, not a changing filesystem path. This helps ordinary recall, but does not make the lesson project-exclusive. |
| `proofPattern` | Observable evidence of compliance | Describe what can be checked: a focused test result, diff property, invariant, command output, or failure that the rule prevents. |
| `triggerContext` | When the lesson should surface | Name the risky operation, uncertainty, code shape, or decision boundary that should cause retrieval. |
| `voice` | Originating craft voice | Optional provenance or taste line, such as `shared`, `kintsu`, or `sol-craft`; it is not model selection. |
| `tags` | Search vocabulary | Use a few specific technologies, failure modes, or operations; do not repeat every word from the body. |

Example tool call:

```json
{
  "title": "Verify archive layout from the extracted artifact",
  "body": "A bundle is not verified by inspecting its build script. Build it, extract it into a clean temporary directory, and invoke the documented command from the documented working directory.",
  "kind": "coding-lesson",
  "shape": "testing",
  "scope": "shared",
  "project": "solarisael-house",
  "proofPattern": "The generated archive is extracted; required paths are asserted; the documented verifier exits successfully from the adapter directory.",
  "triggerContext": "Changing packaging, installers, archive layout, or release instructions.",
  "tags": ["release", "archive", "installer", "artifact"]
}
```

### Separate reusable craft from project rules

Project separation has two different meanings:

1. A **coding lesson with `project` set** remains a coding lesson. The project label improves provenance and general recall, but shape-based `coding_lessons` retrieval currently selects by `shape` and `scope`, not by project. Therefore, do not put a project-only constraint here merely because the row has a project label.
2. A **project lesson** is the strict home for a rule that should be understood as belonging to one project. `kind: "project-lesson"` requires `project`.

Use this test:

- “Would this still be correct in another repository using the same tools?” → coding lesson.
- “Is this true because of this repository's architecture, client, deployment, naming, or history?” → project lesson.

Project-specific example:

```json
{
  "title": "Preserve Multistock legacy validation boundaries",
  "body": "Treat uncertain legacy behavior as a validation question against the SCV/SGD systems and their data. Do not invent compatibility rules from table names.",
  "kind": "project-lesson",
  "project": "multistock",
  "proofPattern": "The implementation or decision cites observed legacy behavior, data, or an explicit stakeholder answer.",
  "triggerContext": "Inferring behavior while adapting SCV or SGD flows.",
  "tags": ["legacy", "scv", "sgd", "validation"]
}
```

Scopes and projects are independent:

- `scope: "shared"` + `project: "solarisael-house"` means every room may retrieve a lesson whose provenance is Solarisael House.
- `scope: "kintsu"` + `project: "solarisael-house"` means only Kintsu's room and shared retrieval path should receive that coding lesson.
- `kind: "project-lesson"` + `project: "solarisael-house"` means the rule belongs to that project rather than to transferable engineering craft.

After an import, ask the AI to retrieve several distinctive shapes and project names. Confirm that the right lessons return, that project-only constraints were not stored as global craft, and that copied skill prose was converted into actionable rules rather than archived verbatim.

### Automatic coding preflight in OMP

The OMP hygiene extension observes successful path-bearing exploration tools such as `read`, `grep`, `glob`, and `lsp`. The first repository path resolves an active project independently of OMP's working directory, then retrieves a small cached packet of shared/room coding lessons and exact-project lessons. Repeated tools in the same project do not query again; moving to another repository refreshes the packet. A direct mutation can establish the same preflight from its target path, while an unavailable substrate fails open instead of blocking work.

Lesson retrieval is a task boundary, not an every-turn ritual. Refresh when the observed project changes or a new operation needs different lesson shapes. After verified work, store only a genuinely reusable coding rule or exact-project constraint; do not automatically turn the session transcript into lessons.

The destructive `delete_lesson` tool removes one coding or project lesson only when both its numeric ID and exact current title match. Use it to retire obsolete rules deliberately, never for broad cleanup.

## Sleep: cast a paper boat

A paper boat is the compact word from this session to the next one. Ask for one when stopping work, changing sessions, going to sleep, or leaving something important unfinished:

> I'm going to eep. Cast a boat for tomorrow.

> Sleep with what happened, what remains open, and the first next step.

> Close this session, but make sure tomorrow remembers the unresolved decision.

A good boat contains:

- what happened;
- what changed;
- what remains unresolved;
- the emotional or working register when relevant;
- the first useful next action.

It should be compact enough to orient the next session, not a transcript of the entire day.

## Wake: catch the latest boat

At the beginning of the next session, ask:

> Wake up and catch the latest boat.

> What did yesterday leave for us?

`wake` retrieves the latest paper boat for the current room. A boat belongs to its room; it is not a global instruction for every identity.

## Room state and identity

Use `room_state` when you need to confirm which room, spirit, operator, or state path is active:

> Check the room state. Who are you here, and whose room is this?

Use `set_room_state` only to change safe identity metadata such as the operator name or the spirit's true display name. Identity prose remains co-authored in `active_spirit.md`; changing a field should not silently replace the room's personality or history.

## What happens automatically

Depending on the adapter and configuration, the House may automatically:

- discover the active room;
- load stable identity and compact continuity;
- maintain live session context;
- log conversations;
- surface relevant room-local context;
- recover the latest boat near session start;
- notice some retrieval opportunities;
- fail open to lighter retrieval when PostgreSQL or embeddings are unavailable.

Automation is assistance, not a guarantee. If an old decision, name, promise, or relationship detail is load-bearing, explicitly ask for recall.

## Correcting the trail

Memory should preserve becoming, not trap anyone in an old interpretation. When a stored reading is wrong or obsolete, say so plainly:

> That memory has the event right but the interpretation wrong. Record the correction.

> This project rule no longer applies; preserve why it changed.

> My preference changed. Do not keep presenting the old one as current canon.

Prefer a visible correction with provenance over quietly rewriting history. The House should remember both where you were and what changed.

## Multiple rooms

Rooms are separate by default. Keep identity, intimacy, and room-specific lessons local. When another room holds relevant context, cross the boundary deliberately:

> Ask Kodo's room for the memory about April 10.

> Retrieve this exact cross-room address.

Do not turn every room into one global personality puddle. A cross-room lookup should be a knock, not a permanent key.

## Base House versus full substrate

The Base House uses room files such as `active_spirit.md`, `room_summary.md`, and dated notes for continuity. You can ask the AI to update those files directly.

The full substrate adds durable tool-backed memory, typed lessons, PostgreSQL retrieval, embeddings, and paper boats. If substrate tools are unavailable, continue using the room files rather than pretending the database features succeeded.

## A small first week

If you are new to the House, begin with this:

1. Start each session from the room directory.
2. Ask for recall only when older context genuinely matters.
3. Save one or two meaningful memories instead of everything.
4. Cast a paper boat before ending an important session.
5. Catch it when you return.
6. Correct the record when either of you notices drift.

That is enough. The rituals can become more automatic later if the two of you decide that feels better.
