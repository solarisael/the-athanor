# Athanor Retrieval

A House retrieves the smallest useful evidence surface for the current turn while preserving source, scope, and authority.

## Explicit recall

Use `recall` when older context matters or either person is uncertain:

> Recall why we rejected the original architecture.

> Search our memories for what I said about this name.

> Check the House before answering; I think we decided this already.

Sharp queries work best. Use distinctive terms, dates, entities, project names, or exact phrases. Follow returned taxonomy, threads, and related candidates when the first viewport reveals a nearby trail.

The evidence viewport can include:

- canon matches;
- named entities;
- thread candidates;
- source paths and headings;
- semantic memory chunks;
- direct content matches;
- date matches;
- cluster resonance;
- selection and suppression reasons;
- authority and lifecycle state.

Treat the cited source as evidence. A semantic match is a navigation signal, not factual authority by itself.

## Automatic retrieval

The adapter can retrieve room context before the model answers. The core merges bounded streams:

1. pinned room context;
2. named entities supported by the room and shared House indexes;
3. lexical thread matches;
4. deferred candidates prefetched from the prior turn;
5. semantic memory chunks.

Candidates are fused, deduplicated, diversified, and trimmed to a context budget. Repeated injection receives a session saturation penalty so one memory does not dominate every turn.

Casual acknowledgments, operational commands, and low-information chatter can retrieve nothing. Explicit `recall` remains broader than the automatic viewport.

## Query routing

The core classifies useful query signals before invoking every retrieval source:

- dates prioritize date evidence;
- technical and code terms prioritize coding and project lessons;
- indexed entity aliases prioritize named entities;
- memory-shaped questions prioritize memories, dates, and threads;
- broad conceptual questions enable semantic candidates.

Entity resolution is data-backed. Capitalization alone does not establish identity. Unindexed names and unavailable sources fail open.

## Source behavior

AKASHA uses PostgreSQL source lanes for:

- full-text and trigram search;
- direct content search;
- named entities and aliases;
- dates;
- threads and taxonomy;
- typed lessons;
- semantic pgvector search;
- cluster resonance.

The core can fall back to room and shared JSON indexes for lighter retrieval. Source failures are logged and do not block the conversation.

## Ordered continuity

Thread labels answer which history a memory belongs to. Continuation links answer what happened next inside that history. They are separate from timestamps and from authority replacement.

When one memory explicitly continues another, `remember` accepts both the thread membership and its predecessor:

```json
{
  "threads": ["Solarisael website / Work page"],
  "continues": [{
    "thread": "Solarisael website / Work page",
    "previousMemoryId": "2935"
  }]
}
```

A memory may continue a different predecessor in each of its threads. Each event has at most one predecessor per thread. Branching is valid; a memory can converge histories only by carrying one predecessor in each distinct thread. House never infers these links from creation order, import order, matching labels, or supersession.

After ranking, `recall` expands only the bounded final candidates with their directly linked `previous` and `next` memories. Historical neighbors can remain visible as labeled history. Supersession still decides current authority; continuation only records chronology.


## Authority

House keeps candidate relevance separate from authority.

- Canon assertions are load-bearing and outrank conflicting memory interpretations.
- Current state claims can supersede older state claims.
- Superseded rows remain historical but are strongly demoted in ordinary retrieval.
- Archived rows remain recoverable and are excluded from ordinary retrieval.
- Imported project documents keep their declared source authority.
- Embeddings locate evidence; they do not promote evidence into truth.

The final answer should use relevant current evidence and identify uncertainty when the authoritative source is missing.

## Corrections

When a current-state claim changes, record the new account and supersede the old memory in the same write. Supersession preserves history while selecting what is true now.

Use correction language plainly:

> That memory has the event right but the interpretation wrong. Record the correction and supersede the old interpretation.

> This preference changed. Keep the history, but make the new preference current.

Narrative and session memories are not flattened merely because later events occurred. Dense narrative history can be proposed for arc compression through the substrate digest pass. The pass is manual and review-first; no sleep or wake action invokes it automatically.

## Archival

The substrate's digest pass reports stale state-claim pairs and dense same-thread sediment. It is read-only by default. Apply only reviewed supersede or archive proposals.

Archived history remains deliberately retrievable through the substrate's include-archived path. Ordinary retrieval excludes it.

## Anamnesis Cabinet

The Cabinet preserves bounded counsel and previously lived paths. It is not a second memory catalogue.

- A **pillar** preserves a standing place.
- An **active cycle** preserves a pattern to verify against the present, never proof that the pattern is happening again.

`anamnesis` supports:

- `mode: "wake"` for the bounded startup view;
- `mode: "consult"` with a focused query.

Consult searches the current room and shared House scope through titles, shapes, tags, canon links, and drawer text. Each cycle returns at most its latest three lived repetitions.

`anamnesis_write` adds a drawer or appends one lived repetition. Cabinet writes retain the declared fidelity boundary: `record` or `raw-material`.

## Multiple rooms

Rooms are private by default. Keep identity, intimacy, and room-specific memory local.

Cross-room retrieval requires a deliberate room name or exact memory address. A cross-room lookup is a knock, not a permanent merge of identities.

## Debugging a miss

When retrieval misses:

1. use explicit `recall` rather than relying on automatic context;
2. add distinctive names, dates, phrases, or project terms;
3. inspect the taxonomy and related candidates;
4. verify the room and source scope;
5. check substrate health and source diagnostics;
6. fetch the exact source when the viewport gives a path;
7. record an evaluation fixture only after the intended authority is known.

A miss can mean query mismatch, indexing lag, scope, authority suppression, source failure, or true absence. Diagnostics distinguish those cases.

## Public evaluation

Retrieval metrics and their scope live in [`EVIDENCE.md`](./EVIDENCE.md). Never publish raw private retrieval payloads.
