# Athanor Retrieval

A House retrieves the smallest useful evidence surface for the current turn while preserving source, scope, and authority.

## Explicit recall

Use `recall` when older context matters or either person is uncertain:

> Recall why we rejected the original architecture.

> Search our memories for what I said about this name.

> Check the House before answering; I think we decided this already.

Sharp queries work best. Use distinctive terms, dates, entities, project names, or exact phrases. Follow returned taxonomy, threads, and related candidates when the first viewport reveals a nearby trail.

The evidence viewport depends on the active storage profile.

Vault can return:

- exact source paths;
- Markdown headings, JSON pointers, or JSONL line identity;
- field-aware BM25F score and term coverage;
- matched and missing query terms;
- exact-content and matching-field reasons;
- bounded source excerpts.

AKASHA can additionally return canon, named entities, thread candidates,
semantic and direct-content memory chunks, dates, taxonomy, cluster resonance,
authority, lifecycle state, and suppression reasons.

Treat the cited source as evidence. Relevance selects a viewport; it does not
change the source's authority.

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

### Vault

Vault searches configured operator-controlled roots directly. It parses
Markdown into heading-addressed sections, JSON into pointer-addressed records,
JSONL line by line, and eligible text into bounded chunks.

Its native lanes are:

- field-aware BM25F over relative path, title, heading, structured keys, tags,
  metadata, and body;
- direct exact-content matching for identifiers, filenames, symbols, UUIDs,
  quoted strings, and errors.

The scanner applies configured limits and ignore patterns, honors each root's
top-level `.gitignore`, skips common generated and secret-bearing paths, and
does not follow symlinks. The in-memory index is derived and briefly cached;
files remain authoritative. Missing embeddings do not weaken lexical Vault
retrieval because embeddings are not a Vault requirement.

### AKASHA

AKASHA uses PostgreSQL source lanes for full-text and trigram search, direct
content, named entities and aliases, dates, threads, taxonomy, typed lessons,
pgvector semantic search, controlled lexical expansion, and cluster resonance.

AKASHA source failures are explicit diagnostics. An absent substrate selects
Vault; a configured but unhealthy substrate reports degraded AKASHA rather than
silently changing authority profiles.

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
2. retry with one to three distinctive names, dates, identifiers, or project
   terms;
3. inspect matched and missing terms, reasons, source scope, and warnings;
4. verify the room and active Vault or AKASHA profile;
5. in Vault, verify `vaultRoots`, ignore rules, file eligibility, and scan
   limits;
6. in AKASHA, inspect substrate health, authority suppression, and source-lane
   diagnostics;
7. fetch the exact source when the viewport gives a path;
8. record an evaluation fixture only after the intended authority is known.

A miss can mean query mismatch, ignored or ineligible files, scan limits,
indexing lag, scope, authority suppression, source failure, or true absence.
Diagnostics and attributed results distinguish those cases.

## Public evaluation

Retrieval metrics and their scope live in [`EVIDENCE.md`](./EVIDENCE.md). Never publish raw private retrieval payloads.
