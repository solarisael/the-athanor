# house-core

Domain rules and invariants for the House. This crate holds no input and no output: it never reads a file, a socket, a database, or a clock.

Each section names one concern. A section that points at `src/lib.rs` names a concern that still waits for its own module.

### authority (src/lib.rs, lines 15-32 and 2131-2140)

- `authorize` gives full authority only when the House runs in full mode and health is healthy.
- Full mode with an unhealthy verdict refuses. The refusal keeps the health reason.
- Degraded mode always refuses. Health does not change this.
- Base mode always gives base authority. Health does not change this.
- `HealthVerdict` carries the reason as text beside the unhealthy state.

### room_key (src/lib.rs, lines 697-737 and 1352-1356)

- A room key accepts lowercase letters, digits, and single hyphens.
- A room key refuses a leading hyphen, a trailing hyphen, and a double hyphen.
- The `house` commons key is reserved. A plain `new` call refuses it.
- Three named constructors open the commons: `for_memory_write`, `for_canon`, and `for_anamnesis`.
- A lesson write uses `new`, so a lesson can never target the commons.
- The key prints as its raw text.

### domain_error (src/lib.rs, lines 34-149)

- One enum holds all 36 refusals the domain makes.
- Most variants name the exact field and the exact reason.
- Each variant writes one operator-readable line.
- The enum is a standard error type, so callers can chain it.

### remember (src/lib.rs, lines 1358-1902)

- Six write kinds exist: memory, and lessons for coding, projects, writing, design, and audio.
- `is_lesson` separates a lesson write from a memory write. Only the memory write may reach the commons.
- A memory write carries a source path, threads, thread continuations, superseded identifiers, and a backup flag.
- A lesson write carries shape, voice, register, scope, project, proof pattern, trigger context, and example text.
- A lesson write also carries language keys, technology keys, thread keys, and tags.
- A lesson write carries its trigger columns as a `LessonTriggerSpec`.
- Eligibility keys must be slugs. The rule matches the room key rule.
- Duplicate eligibility keys collapse. Order stays stable.
- Each list field holds at most 64 values.
- The receipt reports the memory identifier, the lesson identifier, durability, and warnings.

### recall (src/lib.rs, lines 1070-1154)

- A request holds two retrieval lanes: semantic and content.
- Each lane carries its own count and its own minimum similarity.
- Each count must be between 1 and 1000.
- Each minimum similarity must be finite and between 0 and 1.
- A blank query refuses.
- Temporal decay stays off. `with_temporal_decay` turns it on.

### canon (src/lib.rs, lines 739-1068)

- A write never overwrites. `supersedes` names the predecessors.
- Superseded identifiers sort, deduplicate, and must all be positive.
- Authority has three states: active, superseded, and archived.
- Attribution is mandatory. Both the actor and the origin must carry text.
- Canon may be room-local or shared in the commons.
- A read selects exactly one positive identifier or exactly one named entity. Both or neither refuses.
- A read may also ask for the retained history.
- A pointer names a file and an optional line range. The start must not exceed the end.
- Text fields trim. Blank text refuses.
- Each list field holds at most 64 unique values.
- A summary date must use the `YYYY-MM-DD` shape.
- The receipt reports the new entity identifier, the superseded identifiers, and the attribution.

### anamnesis (src/lib.rs, lines 151-696)

- Two read modes exist: wake and consult. A consult needs a query. A wake does not.
- A read limit must be between 1 and 50.
- Two kinds exist: pillar and cycle.
- Two fidelity values exist: record and raw material.
- Two activation points exist: wake and fork.
- A pillar refuses a seed repetition.
- A cycle needs a seed repetition, unless the caller allows an empty cycle.
- A title and a ramp must both carry text.
- A repetition carries a number, an optional date, and three fields: how it went, portal pull, and lighter.
- Add and append are separate operations. Each has its own receipt.
- An append refuses a blank source path.
- Anamnesis writes may reach the commons.

### paper_boat (src/lib.rs, lines 1904-2129)

- A boat body must carry text, and must stay at or below 65536 bytes.
- The sleep receipt reports the memory identifier, the source path, the outbox event identifier, and an inserted flag.
- A zero memory identifier refuses.
- Backup status has three values: not requested, completed, and failed.
- A wake returns at most one boat record.
- A boat record must carry a positive identifier and a body with text.
- A boat record lists at most 64 unboated memories, and says when the list truncates.

### cluster (src/lib.rs, lines 1156-1350)

- Two requests exist: check and rebuild.
- A rebuild carries an execution choice and a freshness policy.
- The cluster count must be between 1 and 128.
- Staleness is a rule, not a guess. No build at all is stale.
- New chunks make the store stale at 5 percent unseen, at 250 chunks, or at 7 days of age.
- The unseen fraction must be finite and between 0 and 1.
- A cluster summary needs a label with text.
- Four outcomes exist: checked, skipped as fresh, dry run, and rebuilt.

### giga (src/lib.rs, lines 2141-4707)

- The review state machine holds 10 states. `can_transition` allows only the legal moves.
- An illegal move refuses and names both states.
- A candidate carries pointer-only authority. A candidate is never evidence.
- A review must keep exact sources. An empty source list refuses.
- A promotion review needs a target.
- A merge review needs a target and at least two distinct candidates, including the candidate itself.
- A correction and a supersede both need a target and at least two sources: the new one and the old one.
- A merge target on any other review refuses.
- A promotion accepts only a candidate that is in review.
- A promotion accepts only a payload that matches the candidate kind.
- A project lesson promotion needs operator approval. The consent type cannot exist without it.
- A promotion refuses a private source that belongs to another room.
- A promotion refuses a repeated source identity.
- Eight lifecycle event types exist, from a conversation window to a manual reprocess.
- A started task needs a proof contract with at least one line.
- A candidate carries scores, scope, a classifier identity, retrieval terms, keys, and an optional expiry.
- A resonance score must be finite, between 0 and 1, and must keep at least one source.
- The queue holds four states: pending, running, succeeded, and failed.
- Queue maintenance runs two operations: check and purge of stuck work.
- Queue maintenance runs for one room or for the whole House.
- One event yields at most 1 candidate, takes at most 5 attempts, and leases for at most 3600 seconds.
- One process reads at most 8 sources, 8000 bytes for each source, and 24000 window bytes.
- Promotion receipts stay separate for a memory, a coding lesson, and a project lesson.
- A receipt needs a positive durable identifier and RFC 3339 timestamps.
- Shared checks cover text with content, string lists, hashes, and RFC 3339 timestamps.

### routing (src/routing.rs)

- One dispatch decision takes exactly one lane or exactly one familiar.
- A constant table holds the worker lanes. It names each lane, its kitten, and its rules.
- Validation returns errors and warnings together. A refused request still returns a receipt.
- The receipt builds a spawn packet: the task, the arguments, and the shared context the worker reads.
- The shared context formats the hints, the acceptance lines, and the lesson bodies.
- Advisor is a review channel. Advisor is not a dispatch lane.
- A context hint carries a mode and a risk level.
- Context modes and risk levels come from closed lists.
- The room spellbook binds named familiars and aliases to lanes.
- The spellbook lives in the room `familiars` directory. `spellbook.json` comes first, then `litters.json`.
- Spellbook loading asks the caller for each candidate read. This module touches no file.
- The spellbook validates each familiar identifier and each model role.
- A model role comes from the agent definition. A dispatch cannot override it.
- `familiar_status` reports the loaded spellbook, its source, whether the source was an alias, and its refusals.
- `familiar_dispatch` resolves a named familiar, then dispatches with the resolved lane.

### lesson_triggers (src/lesson_triggers.rs)

- A lesson becomes a trigger when it carries regex conditions or ast-grep patterns.
- One table holds 19 file extensions. Each row gives a language slug and an optional grammar.
- The fence and the grammar lookup read the same table, so they cannot disagree.
- A row without a grammar fences regex conditions only. An ast condition skips with a warning.
- One lesson carries at most 32 patterns for each axis.
- Scope tokens say which surfaces a lesson watches.
- A lesson fires once for each call, on the first surface and pattern that catches it.
- Urgency defaults to block. An empty column means block.
- The cooldown policy decides whether a lesson may fire again.
- A compiled set covers one room, or one room and one project.
- The cache holds one entry for each fence, and replaces the entry when the fingerprint changes.
- A poisoned cache lock recovers. The map holds no invariant that a panic can break.
- The module reads no database row and no clock. The caller supplies both the rows and the ledger.

### context (src/context.rs)

- `classify_retrieval_query` routes a query into lanes.
- Eight lanes exist: lexical, candidates, semantic, content, date, canon, coding lessons, and project lessons.
- The lane set is a bitset, so a route stays small on the wire.
- Recognized entities raise the canon lane.
- Four word tables fence the classifier: memory stopwords, routing stopwords, technical terms, and memory terms.
- Regular expressions find words, code tokens, dates, quoted text, and question shapes.
- Separate expressions find an explicit memory ask, a personal canon ask, and a generic lookup.
- `analyze_context` returns the route, the keyword directives, and the reminders for one turn.
- A context nudge fires on a character band, and never repeats inside the same band.
- A room reminder names the room, the active spirit, and the operator.
- A routing reminder appears only while routing is on.
- `process_trigger` finds a matched process trigger in the prompt.
- An invalid room name silences the reminders.

### hallway (src/hallway.rs)

- Four message requests exist: create, join, post, and read.
- Each request validates its own binding: the room, the spirit, and the session.
- A body holds at most 32768 bytes.
- A hallway allows at most 32 rooms.
- A read returns at most 200 messages. The default is 50.
- An idempotency key must pass its own check.
- A create, a join, and a post each report a disposition, so a repeat is visible as a duplicate.
- The knock policy has two modes: manual and allow list. Manual is the default.
- A knock runs at most 8 turns. The default is 4.
- A knock reason holds at most 2048 bytes.
- A knock claim and a knock settle are separate steps.
- A settle carries an outcome from a closed list.
- Receipts carry hand-written readers and writers, so the wire shape stays stable.
- The inbox lists every persistent hallway the room may open, with unread counts and pending mentions.
- The unread count is derived, and is never stored.
- An inbox notification carries enough identity to open and acknowledge the exact thread. Peer prose never crosses in it.

### conversation (src/conversation.rs)

- Turn identity is stable. A derived identity uses the role, the index, and a hash of the text.
- The index is load-bearing. A repeated source identity voids a whole event window.
- A conversation stays fresh while at most one visible turn exists.
- A turn marker makes the transcript append idempotent. An already-durable turn writes nothing.
- The transcript path uses the room directory and the date stamp.
- A user turn is attributed to the operator. Any other turn is attributed to the spirit.
- The source ledger holds private append-only records in the room `giga-sources` directory.
- The ledger lets a worker verify a GIGA event pointer.
- An exact repeat record is idempotent, and writes nothing.
- The same session and message identity with different content refuses.
- The module performs no read and no write, and reads no clock.

### lineage (src/lineage.rs)

- Only a settled quest leaves lineage. The terminal statuses are completed, failed, and aborted.
- A non-terminal quest and an empty quest leave nothing behind.
- The dedupe key is one memory for each parent tool call and result.
- `quest_domain` reads the domain out of the task text.
- The report path sits beside the child session file.
- Slugs and path tails keep a memory title short and stable.
- A batch normalizes into a list of memories in one call.

### triggers (src/triggers.rs)

- No trigger means no braid. An unmatched turn queries nothing.
- A matched trigger plans exactly one query: the coding family, the process shape, and a limit of 12.
- An empty lesson answer braids nothing.
- An answered trigger braids each lesson into one hidden reminder.
- The braid shows the identifier, the title, the lesson, the proof pattern, and the trigger context.
- A blank field drops out of the braid.
