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

## Remember: preserve something deliberately

Ask the AI to remember when something happened that tomorrow should not have to rediscover:

> Remember why we chose PostgreSQL over the service mesh.

> Remember that this name matters to me and why.

> Keep today as a memory; this changed how I understand the project.

A useful memory records the event, decision, or realization and enough context to understand why it matters. Do not save every sentence. Repeated trivia makes important trails harder to find.

Never put passwords, API keys, access tokens, or other secrets into memories.

## Lessons: keep reusable knowledge in the right store

Not everything belongs in ordinary memory. The House supports purpose-specific lesson stores:

- **Coding lesson:** a reusable engineering or process rule, ideally with a proof pattern
- **Project lesson:** a rule or constraint that belongs to one named project
- **Writing lesson:** a durable prose, voice, register, or taste preference
- **Audio lesson:** a reusable rule for an audio or speech pipeline

Tell the AI what kind of lesson it is:

> Save this as a coding lesson: verify the staged set before every commit.

> This is a Multistock project lesson, not a global coding rule.

> Keep this as a writing lesson for my voice.

Before a risky engineering operation, the AI can consult `coding_lessons` for the relevant process shape.

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
