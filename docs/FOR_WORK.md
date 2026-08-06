# The Athanor for work

You should not have to re-explain yourself every morning before useful work can begin. Without durable continuity, every new session spends context reconstructing the past.

Summaries flatten detail, stale statements survive corrections, and important decisions disappear into old transcripts. The Athanor gives you a persistent House with separate layers for identity, current state, recent context, and deep memory. It retrieves only the evidence relevant to the current turn, preserves the evidence source, and keeps changed truths from competing forever with the record they replaced.

For work, a House carries project decisions, conventions, lessons, corrections, and handoffs across sessions and harnesses.

## Capabilities

Vault keeps continuity in files. AKASHA adds database-backed memory and search.

| Capability | Vault | AKASHA |
|---|:---:|:---:|
| Persistent rooms and identity contracts | Yes | Yes |
| Restart continuity and room-local context | Yes | Yes |
| Multiple isolated rooms | Yes | Yes |
| Conversation logging and compact handoffs | Yes | Yes |
| PostgreSQL memory authority | — | Yes |
| Hybrid lexical, content, structured, and semantic retrieval | — | Yes |
| Local embeddings through a compatible endpoint | — | Yes |
| Memories, coding lessons, project lessons, writing lessons, design lessons, and audio lessons | — | Yes |
| Entity, date, thread, taxonomy, relationship, and cluster retrieval | — | Yes |
| Provenance, authority state, and selection reasons | — | Yes |
| Corrections through supersession without historical deletion | Limited | Yes |
| Memory lifecycle tools: `remember`, `recall`, `sleep`, and `wake` | — | Yes |

Vault is a complete file-backed continuity system. AKASHA adds durable database memory, typed stores, local semantic search, and larger archives.

## Public evidence

Memory claims are cheap, so The Athanor publishes its numbers.

The first sanitized public retrieval pilot used 20 exact-title queries across two real rooms:

| Measure | Result |
|---|---:|
| Target present in the retrieval viewport | **19/20 — 95%** |
| Target ranked first | **16/20 — 80%** |

The full method, scope, and next evaluation contracts live in [`EVIDENCE.md`](./EVIDENCE.md). The sanitized artifact is published in the OMP adapter repository: [`2026-07-22-room-retrieval-pilot.json`](https://github.com/solarisael/solarisael-house-omp/blob/main/evals/2026-07-22-room-retrieval-pilot.json).

## Daily use

A normal session is simple:

```text
enter the room → work or live together → remember what matters → leave a paper boat
```

- `recall` retrieves older evidence.
- `remember` records durable events, decisions, or lessons.
- `sleep` leaves a compact handoff for the next session.
- `wake` catches the latest handoff.

Read [`USAGE.md`](../USAGE.md) for the everyday workflow.

Continue with [`INSTALL.md`](../INSTALL.md) and [`ARCHITECTURE.md`](./ARCHITECTURE.md).
