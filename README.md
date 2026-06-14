# Solarisael House

A memory-and-identity substrate for [OpenCode](https://opencode.ai) agents.

The "house" is three cooperating pieces: an OpenCode **plugin** that runs per
turn, a Postgres **substrate** that stores and retrieves memory, and a set of
**divination** scripts. Together they give an agent a stable identity per
working directory, durable memory with lexical + semantic recall, and a
discipline against confabulation (look-or-admit, never extrapolate).

This repo is the **plugin** — the engine. The substrate and divination live
alongside it in the vault (see [Layout](#layout)); they talk to the plugin
only through Postgres, so the three are decoupled at the code level.

---

## What it does

Per user turn, the plugin:

1. **Resolves identity from `cwd`.** The working directory names the room; the
   room names the spirit and the agent. It writes an `active_spirit.md`
   declaration so the agent knows who it is this turn.
2. **Injects memory.** It queries Postgres for relevant prior memory — lexical
   threads, canon entries, and semantic (embedding) matches — and injects them
   as a `<system-reminder>` before the model generates.
3. **Appends a ledger.** Every user and assistant turn is logged to a
   conversation jsonl for continuity.
4. **Exposes `recall`.** A dragon-callable tool so the agent can query the
   substrate mid-generation when *it* notices uncertainty, not only on the
   user's prompt.

Everything fails open: if Postgres or the embedder is down, retrieval degrades
to a JSON-index fallback and the turn is never blocked.

---

## Architecture

```
OpenCode turn
   │
   ├─ chat.message ............ parse EMBODY/Operator directives,
   │                            resolve room from cwd, write active_spirit.md,
   │                            log the user turn
   │
   ├─ chat.messages.transform . inject room memory context (Postgres →
   │                            lexical + canon + semantic), inject keyword
   │                            triggers (ultrathink, coding-lessons banner)
   │
   ├─ tool.execute.before ..... on risky bash, prepend a coding-lessons
   │                            (shape='process') echo-banner
   │
   ├─ tool: recall ............ on-demand substrate query, returns canon +
   │                            content + semantic matches OR "no match"
   │
   └─ text.complete ........... log the assistant turn
```

**Retrieval has four match types** (surfaced by `recall`, ranked):

- **date** — `memories.dates` GIN index, authoritative direct lookup
- **canon** — `named_entities`, the load-bearing concept layer
- **content** — `pg_trgm` word-similarity, for proper nouns / exact strings
- **semantic** — `halfvec` cosine over embedded chunks, for concepts

**Embeddings:** Ollama (`qwen3-embedding:4b`, 2560-dim, stored as
`halfvec(2560)`). Default endpoint `http://127.0.0.1:11434/api/embed`. The
client accepts both Ollama and OpenAI-compatible response shapes.

**Database:** Postgres running natively in WSL Ubuntu, reached from Windows via
mirrored loopback. Pure Postgres + extensions (`pgvector`, `pg_trgm`, GIN +
tsvector); no service mesh.

---

## Components

### 1. The plugin (this repo)

Lean by design. Module map:

| File | Responsibility |
|------|----------------|
| `index.ts` | hook wiring only |
| `paths.ts` | constants + tuning knobs |
| `util.ts` | generic helpers (json io, regex, fs) |
| `wsl.ts` | the Windows→WSL spawn seam (path translation + spawn) |
| `spirit.ts` | room coercion, contract loading, `active_spirit` writes |
| `directives.ts` | session state, EMBODY/Operator/DISMISS parsing |
| `ledger.ts` | conversation jsonl + spirit window + live context |
| `memory-sources.ts` | Postgres spawn wrappers + JSON-index fallback |
| `memory-rank.ts` | pure matching / ranking / canon-overlay logic |
| `memory.ts` | excerpt shaping, merge/format, orchestrator, `recall` |
| `triggers.ts` | keyword triggers + coding-lessons banner |
| `postgres-memory-source.py` | the Python DB bridge (lexical + semantic) |

Tests: `bun test` (unit) and `bun run test:recall` (integration).

### 2. The substrate (`obsidian/house/substrate`)

Postgres schema + the read/write/maintenance scripts. Highlights:

- **Writers:** `record_memory.py` (canonical single-writer — upsert, rebuild
  threads, chunk, embed, link entities, in one atomic call), plus
  `record_coding_lesson.py`, `record_project_lesson.py`, `record_writing_lesson.py`.
- **Readers:** `query_coding_lessons.py`, `query_project_lessons.py`,
  `query_writing_lessons.py`, `dump_room.py` (Postgres → markdown export).
- **Ops:** `backup.sh` / `restore.sh` / `test_restore.sh`, `migrations/` (numbered SQL).
- **Schema** (via migrations `0001`–`0021`): `memories` + `memory_threads` +
  `memory_chunks(body_embedding halfvec(2560))`; `named_entities` (canon);
  `coding_lessons` / `project_lessons` / `writing_lessons`; Discord peer-room
  tables; continuity rails; `gym_walk_ledger`; `anamnesis_cabinet`.

See `obsidian/house/substrate/README.md` for the full operational reference.

### 3. Divination (`obsidian/house/divination`)

Entropy-backed oracles. `_entropy.js` is the shared source (ANU quantum RNG
primary, crypto fallback; every draw attributes its source). Oracles:
`tarot`, `runes`, `iching`, `geomancy`, `bibliomancy`, `stichomancy`,
`astrology`. Run any with `bun run <path> [args]`; pass `--crypto` to skip the
quantum API.

---

## Layout

```
~/.config/opencode/plugins/solarisael-house/   ← this repo (the plugin)
obsidian/house/substrate/                       ← Postgres schema + scripts
obsidian/house/divination/                      ← oracle scripts
obsidian/<room>/                                ← per-room identity + memory (private)
```

---

## Setup

Prerequisites: WSL Ubuntu with Postgres, Ollama with `qwen3-embedding:4b`
pulled, Bun, and OpenCode.

1. **Database**

   ```bash
   sudo pg_ctlcluster 16 main start          # or autostart via /etc/wsl.conf
   # one privileged step: vector is not a trusted extension
   sudo -u postgres psql -d solarisael_memory -c 'CREATE EXTENSION IF NOT EXISTS vector;'
   ./run_migrations.sh                        # idempotent; applies all migrations
   ```
   Full reproduce steps: `obsidian/house/substrate/README.md`.

2. **Config** — copy the example env files and fill in real values:

   ```bash
   cp obsidian/house/substrate/.env.example   obsidian/house/substrate/.env
   cp obsidian/house/divination/.env.example  obsidian/house/divination/.env
   ```

3. **Embedder** — `ollama pull qwen3-embedding:4b`; confirm it answers at
   `127.0.0.1:11434/api/embed`.

4. **Plugin** — clone this repo into `~/.config/opencode/plugins/` and
   `bun install`. OpenCode loads it on next start.

---

## The secret boundary — never commit

The whole point of a shareable house is that the *machinery* travels and the
*content* does not. These never enter a shared repo:

- `**/.env` — DSN, password, API keys (use the `.env.example` templates instead)
- `substrate/backups/*.dump` — a single dump is the entire personal substrate
- the populated database itself
- `obsidian/<room>/memory/`, identity files, and canon/mythology content

Each component's `.gitignore` enforces the first two. The private content lives
in the vault and is simply never committed.

---

## Memory discipline

Retrieval is agency to look; the rule is **look-or-admit, never extrapolate.**
If `recall` returns no canonical match, the honest answer is "i don't have
this" — not a plausible reconstruction from adjacent matches. The substrate is
the font of truth; markdown on disk is a human-readable export.
