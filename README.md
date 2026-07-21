# Solarisael House

A local-first continuity and memory system for AI companions and collaborators.

> [!IMPORTANT]
> **Give this repository to a tool-capable AI. The AI performs the installation.**
>
> Do not judge Solarisael House as a pre-compiled installer. It is an adaptable plugin and configuration framework.
>
> The AI inspects the current harness, operating system, paths, runtime, and database support.
>
> The AI then adapts the House to that environment, preserves compatible configuration, and verifies the installed behavior.

The operator chooses the desired continuity and accepts consequential tradeoffs. The AI owns command translation, configuration, execution, and verification.

Read [`INSTALL.md`](./INSTALL.md) for the AI-guided installation contract.

Read [`USAGE.md`](./USAGE.md) for practical examples of project work, recall, lessons, and daily operation.

Solarisael House gives an AI a persistent room, stable identity, and searchable history. It retrieves relevant context without loading the complete archive.

The project supports two deployment modes:

- **Base House** uses local files for identity, room state, session continuity, and restart recovery.
- **Full House** adds PostgreSQL, `pgvector`, local embeddings, hybrid retrieval, typed stores, and durable memory tools.

Read [`HOUSE.md`](./HOUSE.md) for the personal history and design philosophy behind the project.

## Documentation

| Document | Purpose |
|---|---|
| [`INSTALL.md`](./INSTALL.md) | AI-guided installation and verification |
| [`USAGE.md`](./USAGE.md) | Daily memory lifecycle and room operation |
| [`IDENTITY_GUIDE.md`](./IDENTITY_GUIDE.md) | Identity and room design |
| [`HOUSE.md`](./HOUSE.md) | Personal history and project philosophy |
| [`docs/roadmap.md`](./docs/roadmap.md) | Planned work and release direction |
| [`evals/README.md`](./evals/README.md) | Retrieval evaluation tools and limits |

## What the system provides

### Persistent rooms

Each room owns an identity, state, context, paths, and private memory scope. A room survives model changes, provider changes, context limits, and process restarts.

### Layered context

The context model separates stable identity, current state, recent continuity, and deep memory. This design keeps startup context small while preserving access to older records.

### Hybrid retrieval

Full House can retrieve information through:

- PostgreSQL full-text search;
- semantic vector search with `pgvector`;
- direct content matching;
- named entities and canon;
- dates and taxonomy;
- thread and relationship metadata;
- cluster-assisted discovery;
- coding and project lessons.

The retrieval pipeline ranks evidence and exposes its source. A model can inspect a miss instead of treating an empty viewport as proof of absence.

### Structured memory

Full House stores:

- personal and session memories;
- project, coding, writing, and audio lessons;
- entities, dates, threads, and relationships;
- provenance and source paths;
- corrections and supersession records;
- session-closing paper boats.

Supersession removes stale retrieval authority without deleting the historical record.

Full House includes an optional privacy-safe library with 117 reusable coding lessons and no project lessons.

The installer can preview the pack before import. The default import preserves lessons with the same scope, project, and title.

### Local semantic search

Full House generates embeddings through a local endpoint. The tested default is `qwen3-embedding:4b` through Ollama.

Another compatible model can replace it. Indexing and recall must use the same vector space.

A dimension change requires a database migration. Any model change requires a complete vector and index rebuild.

### Tool-native lifecycle

The House exposes tools for:

- `recall`;
- `remember`;
- `wake`;
- `sleep`;
- room state;
- identity state;
- coding and project lessons;
- guarded lesson updates and deletion.

The normal lifecycle is:

```text
remember → recall → sleep → wake
```

Not every action runs automatically. This default keeps memory writes, corrections, and identity changes deliberate.

### Retrieval evaluation

The repository includes evaluation tools for ranking behavior, route attribution, and real-use observation.

```console
bun run test
bun run test:recall
bun run eval:synthetic
bun run eval:id-ranking
bun run eval:daily
```

A non-empty retrieval is not proof of a useful match. Review the retrieved evidence and the model response before making a quality claim.

## Deployment modes

Base and Full are deployment modes. They are not beginner and expert levels.

Choose Base when file-backed continuity is the intended scope. Choose Full when the operator wants durable database memory and hybrid retrieval.

| Capability | Base House | Full House |
|---|:---:|:---:|
| Persistent identity and room state | Yes | Yes |
| Local file continuity | Yes | Yes |
| Restart recovery | Yes | Yes |
| PostgreSQL authority | No | Yes |
| Local embeddings | No | Yes |
| Semantic and hybrid retrieval | No | Yes |
| Typed memory and lesson stores | No | Yes |
| Corrections and supersession | Limited | Yes |
| Entity, date, taxonomy, and cluster retrieval | No | Yes |

The public [`solarisael-house-substrate`](https://github.com/solarisael/solarisael-house-substrate) repository owns the Full House backend.

That repository contains the migrations, dependencies, environment template, health probe, lifecycle smoke, memory tools, and backup path.

## Requirements

### Base House

| Component | Requirement |
|---|---|
| Operating system | Windows 10 or 11, or a modern Linux distribution |
| AI harness | [Oh My Pi](https://github.com/can1357/oh-my-pi) is recommended; OpenCode has an adapter |
| Runtime | [Bun](https://bun.sh/) |
| System memory | 8 GB RAM |
| GPU | Not required |
| Database | Not required |

### Full House

| Component | Tested or recommended configuration |
|---|---|
| System memory | 16 GB RAM |
| GPU memory | Approximately 4 GB available VRAM |
| Free storage | 10 GB |
| Database | PostgreSQL with `pgvector` |
| Embedding endpoint | Ollama or another compatible local endpoint |
| Embedding model | `qwen3-embedding:4b` is the tested default |
| Substrate runtime | Python 3 |
| Guided Windows backend | WSL 2 with Ubuntu |

A GPU is optional. CPU embeddings work through system memory, but they are slower.

The tested guided path uses Windows, OMP, and WSL. The substrate itself runs on Linux.

Native Linux integration may require adapter work and verification. macOS remains untested and unsupported.

## Installation contract for AI agents

Solarisael House expects a tool-capable AI to install and configure it with the operator.

The installing AI must:

1. Ask which continuity behavior the operator wants.
2. Inspect the host and existing configuration.
3. Select a viable deployment mode from the requested behavior.
4. Preserve existing configuration unless a change is necessary.
5. Install the selected mode.
6. Run the relevant health and lifecycle checks.
7. Report the verified mode and any degraded capability.

Do not default to Base only because Full has more prerequisites.

Do not invent a trial period. Do not require the operator to prove that semantic memory is necessary.

Do not claim that Full House needs private maintainer files. Its backend implementation is public.

Do not convert “untested” into “impossible.” Adapt another host only when you can verify the result.

Do not treat harness memory as an equivalent replacement. Compare the requested behavior and verify it directly.

Read [`INSTALL.md`](./INSTALL.md) before changing the host.

## Health states

An installation can report one of three states:

- **Base:** the file-backed room and continuity path works.
- **Full:** the Base path and the canonical substrate health contract work.
- **Configured-but-degraded:** the substrate is configured, but one or more Full requirements fail.

A configured path does not prove that Full House works. Run the canonical health and lifecycle checks.

When Full becomes degraded, the Base files remain available. The adapter must not report a successful database read or write without evidence.

## Privacy boundary

The House keeps private room data on the operator's machine.

Local embeddings prevent the memory archive from entering a hosted embedding service. Model providers can still receive the context included in a model prompt.

Telemetry must not duplicate raw user text. Store prompt hashes and retrieval diagnostics separately from private session logs.

Do not publish raw turns, source paths, entities, thread names, or private retrieval viewports. Publish only censored aggregates and non-identifying failure categories.

## Repository boundaries

This repository owns the core routing, retrieval presentation, room behavior, evaluation tools, and shared memory logic.

The public [Full House substrate](https://github.com/solarisael/solarisael-house-substrate) owns:

- PostgreSQL migrations;
- Python dependencies;
- environment configuration;
- `health.py`;
- lifecycle smoke tests;
- backup and restore;
- Full House memory tools.

Follow the substrate instructions for backend setup. Do not copy that procedure into this repository.

## License

Solarisael House uses the Apache License 2.0.

Use it, adapt it, and build another room. Preserve the attribution required by [`LICENSE`](./LICENSE) and [`NOTICE`](./NOTICE).
