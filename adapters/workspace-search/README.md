# Athanor workspace search

## Not re-verified at a6ab453

The 2026-10-04 code census did not cover this adapter.
The text below moved here unchanged from the root README.
Read `package.json`, `install.ps1`, and `src/` before you act on it.

### Local workspace search for OMP

The optional Node adapter in `adapters/workspace-search` uses zvec-grep with local Ollama Nemotron embeddings.
It searches repository files. It does not change Vault, AKASHA, or the Host.
Node 24 or later and the following installed Ollama model are required:

```text
hf.co/zenmagnets/Nemotron-3-Embed-1B-Q4_K_M-GGUF:latest
```

Install the adapter from the repository root:

```powershell
pwsh -NoProfile -File adapters/workspace-search/install.ps1
```

The installer retains a hashed package archive under `~/.omp/tools/athanor-workspace-search`.
It installs pinned dependencies and replaces the `zvec_grep` entry in the OMP MCP configuration.
Restart OMP to load the installed server.

Index one repository explicitly:

```powershell
$entry = "$HOME/.omp/tools/athanor-workspace-search/node_modules/@solarisael/athanor-workspace-search/dist/cli.js"
node $entry index --root C:/Projects/my-repository
node $entry search --root C:/Projects/my-repository --query "Which module owns request cancellation?"
node $entry status --root C:/Projects/my-repository
```

Repeat the index command to update changed files.
Use another absolute root to index another repository.
Search never creates an index. A missing index returns `INDEX_MISSING`.
An ordinary search does not refresh the index or check every file.
Returned snippets carry their own freshness state.
Search returns five hits per query group by default, with at most 2000 characters of content per hit.
The `contentTruncated` flag marks clipped excerpts. Metadata is additional.
Use zvec for discovery, then native grep and read once the relevant path or symbol is known.
Set `--limit` explicitly when you need more results.
Use `--autoUpdate` for an inline refresh before a search.
Use `status` to inspect changes and failed files.

**Warning:** `index --rebuild` discards the existing derived index.
Use this option only when you explicitly need a rebuild.
The adapter never changes repository source files.
The model uses 2048 dimensions, a 4096-token context, and distinct query and passage prefixes.
The index uses a smaller 1024-token chunk budget because zvec estimates size from characters.
Oversized embedding inputs fail instead of being silently truncated.

The MCP server exposes `zvec_grep_search`, `zvec_grep_index`, and `zvec_grep_status`.
Use the CLI for long initial builds.
There is no background watcher.
Keep Lumen enabled until the new adapter passes an index and search check.
Then disable its OMP plugin:

```powershell
omp plugin disable lumen@claude-plugins-official
```
