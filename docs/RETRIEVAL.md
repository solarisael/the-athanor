# Athanor Retrieval

Not re-verified at a6ab453.
The 2026-10-04 census did not walk `crates/akasha`; claims below keep their earlier date.

A House retrieves the smallest useful evidence surface for the current turn while preserving source, scope, and authority.

## Explicit recall

Use `recall` when older context matters or either person is uncertain:

> Recall why we rejected the original architecture.

> Search our memories for what I said about this name.

> Check the House before answering; I think we decided this already.

Sharp queries work best. Use distinctive terms, dates, entities, project names, or exact phrases. Follow returned taxonomy, threads, and related candidates when the first viewport reveals a nearby trail.

The evidence viewport depends on the active storage profile.

Manual OMP Recall accepts a caller-selected `limit`, with a default of 12.
The limit controls memory records and results from each retrieval lane.
Vault uses the requested limit instead of its configured result count.

Each selected AKASHA memory carries its complete authoritative text in `retrievalCandidates[].body`.
The existing `excerpt` remains a preview.
Automatic Recall retains its character and context budgets.
The default AKASHA semantic similarity floor is 0.25.
Use `semanticMinSimilarity` or `contentMinSimilarity` to select an AKASHA similarity floor from 0 through 1.
Manual Recall remains age-neutral.

An exact-ID-only query returns the requested records in order, without unrelated ranked matches.
Duplicate IDs use one record slot.
Warnings identify missing, refused, and overflow records.
Natural queries select ranked records and load their complete bodies within the same record-count limit.

The transport can refuse an oversized response under its existing message-size safety limit.
Such a refusal is an error, never a successful partial record.
Vault results, canon summaries, and related-record previews retain their existing contracts.

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

AKASHA removes casual English and Portuguese filler from lexical query terms.
It also removes recognized keyboard laughter.
Coverage counts the retained content terms.
Document tokenization does not change.
Canonical name lookup and exact memory references keep their original terms.
The `remember` tool remains a subject; a negated phrase such as `don't remember` does not.

Semantic retrieval first embeds the original query.
If its best visible similarity is below the floor, it can retry once with the retained subject terms.
An empty, unchanged, or reference-only retry does not run.
An empty vector corpus does not cause a retry.
Both passes use the same scope, floor, and limits.
Retry hits identify their pass in the evidence.
A retry failure preserves lexical results and adds a warning.

Treat the cited source as evidence. Relevance selects a viewport; it does not
change the source's authority.

### Manual read limits

Manual tools accept caller-selected positive result counts instead of fixed ceilings of 50, 200, or 250.
This applies to `lessons`, `design_doc`, `anamnesis`, `giga_candidate_list`, `hallway_read`, `quest_board`, `quest_evidence`, `orihon`, `unfold`, and notebook reads.
The public counts use unsigned 32-bit integers.
Notebook counts use positive JavaScript-safe integers.
Invalid counts return an error.

An ordinary lesson search defaults to 12 results.
An exact-ID lesson read without a limit returns all matching requested IDs within the caller's scope.
An explicit lesson limit includes related lessons from thread expansion.

Orihon search uses its limit for candidates.
Its provider byte limits still apply.
Automatic context budgets, panel pagination budgets, access checks, and transport byte limits remain in place.


## Automatic retrieval

The adapter can retrieve room context before the model answers. The core merges bounded streams:

1. pinned room context;
2. named entities supported by the room and shared House indexes;
3. lexical thread matches;
4. deferred candidates prefetched from the prior turn;
5. semantic memory chunks.

Candidates are fused, deduplicated, and trimmed to a context budget.
Automatic AKASHA Recall applies age decay with a default base half-life of 15 days.
Reviewed durability can slow this decay.
The age weight affects candidate ranking before Jev's contextual judgment.

Repeated user mentions do not accumulate a retrieval bonus.
Subject continuity can change the query and the context supplied to Jev.
Retrieval does not refresh memory timestamps.

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

A memory may continue a different predecessor in each of its threads. Each event has at most one predecessor per thread. Branching is valid; a memory can converge histories only by carrying one predecessor in each distinct thread.

A thread member with no named predecessor continues the latest earlier member of that thread.
The order is the memory date, then the memory ID.
This default applies to `remember` and to `thread_memories`.
It never replaces a link that exists, and it uses only predecessors that the writer can read.
A member that joins between two linked members branches from the earlier one.
House does not infer links from import order or matching labels.

A memory that supersedes other memories also joins every thread of those memories.
The superseded memories keep their membership and their links as history.

After ranking, `recall` expands only the bounded final candidates with their directly linked `previous` and `next` memories. Historical neighbors can remain visible as labeled history. Supersession still decides current authority; continuation only records chronology.

Each AKASHA candidate with a memory ID also carries `threads`.
`threads` has one entry for each active thread of the memory:

```json
{
  "key": "Solarisael website / Work page",
  "previousMemoryId": 2935,
  "nextMemoryIds": [2940]
}
```

`previousMemoryId` is the predecessor in that thread, or `null` for a first member.
`nextMemoryIds` lists the successors. A thread can fork, so the list can hold more than one ID.
The list is empty when the memory has no successor.
Recall omits each predecessor and successor that the reader cannot read.
To follow a thread, recall the exact ID, for example `memory 2940`.
The Host keeps at most 8 threads for each candidate and at most 8 successors for each thread.

### Join existing memories to a thread

Use `thread_memories` to add existing memories to one thread.
Omit `room` for this room, or set `room` to `"house"` for the House commons.

```json
{
  "thread": "Solarisael website / Work page",
  "memoryIds": ["2935", "2936"]
}
```

All IDs must identify readable, unarchived memories in the target room.
One unknown ID refuses the full call and changes nothing.
The tool ignores duplicate IDs.
It returns each memory's title and `alreadyMember` status.
A repeat call succeeds without duplicate events.

A joined memory with no predecessor continues the latest earlier member by date.
The receipt shows each link that the call makes as `linkedAfter`.
The tool does not change titles, bodies, or links that exist.
Use `remember` with `supersedes` to replace memory text.
Superseded memories can still join a thread.

### Remove memories from a thread

Use `unthread_memories` to remove memories from one thread.
Give a short reason of 500 characters or less.
Omit `room` for this room, or set `room` to `"house"` for the House commons.

```json
{
  "thread": "Solarisael website / Work page",
  "memoryIds": ["2936"],
  "reason": "Joined this thread by mistake."
}
```

The tool does not delete a membership. It moves the membership into the table `thread_event_archive`.
The archive row keeps the archive time, the reason, and the operator.
It also keeps the predecessor, the successors, and the thread references from before the change.

An archived membership is not active.
Thread reads, the `threads` field of `recall`, the thread lane, and `memories.threads` show only active memberships.

The active chain stays whole.
The successors of a removed memory continue from its predecessor.
When the removed memory has no predecessor, its successors become first members.

All IDs must identify readable, unarchived memories in the target room.
One unknown ID refuses the full call and changes nothing.
A repeat call succeeds and changes nothing.
The receipt shows `wasMember` for each memory.
`relinkedSuccessors` lists the successors that the call moved, and `relinkedTo` names their new predecessor.
The receipt names only memories that the caller can read.
The tool does not change titles or bodies.
Use `thread_memories` to add a memory to the thread again. The archive row stays.


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

### Amend a memory

Supersession removes the retrieval authority of the old memory.
Use an amendment when the old memory stays true as history and only part of it changed.
An amendment is an edge from the older memory to the newer memory.
The older memory keeps its full retrieval authority, and Recall does not rank it lower.

Use `amend_memories` to record one amendment:

```json
{
  "olderMemoryId": "2935",
  "newerMemoryId": "2940",
  "changed": "The launch moved from February to March.",
  "keeps": "The original budget and the team list."
}
```

`changed` is required. It tells what the newer memory corrects or updates.
`keeps` is optional. It tells what only the older memory still holds.
Each note holds at most 500 characters.
Omit `room` for this room, or set `room` to `"house"` for the House commons.

Both memories must be readable, unarchived memories of the target room.
The newer memory must not be older than the older memory by memory date.
A memory cannot amend itself, and two memories cannot amend each other.
A repeat call with the same text changes nothing and returns `alreadyAmended: true`.
A call with other text for the same pair is refused. The stored amendment stays.
The tool does not change titles or bodies, does not set `superseded_by`, and does not archive.

To amend in the same write, give `remember` the `amends` list:

```json
{
  "amends": [{
    "memoryId": "2935",
    "changed": "The launch moved from February to March."
  }]
}
```

The same rules apply. One refused amendment refuses the whole write.
Keep `supersedes` for a new memory that fully replaces a duplicate.

Each AKASHA candidate with a memory ID carries `amendedBy` and `amends`.
`amendedBy` lists the newer memories with `memoryId` and `changed`.
`amends` lists the older memories with `memoryId`, `changed`, and `keeps`.
An amended memory means: read the newer memory for the current state, and keep this memory as true history.
Recall omits each amendment whose other memory the reader cannot read.
The Host keeps at most 8 amendments in each direction for each candidate.

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
