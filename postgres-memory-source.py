#!/usr/bin/env python3
"""Load memory retrieval shapes from the Solarisael Postgres substrate.

OpenCode's plugin is TypeScript-only and should stay fail-open. This helper keeps
the database dependency isolated: stdout is JSON on success, non-zero on failure.
The caller falls back to room JSON indexes if anything here breaks.

Output JSON contains a subset of the following keys depending on --mode:
  - index          (lexical thread/file shape from `memories` + `memory_threads`)
                   present when --mode is 'full' or 'lexical'
  - importantIndex (named entity shape from `named_entities`)
                   present when --mode is 'full' or 'lexical'
  - taxonomy       (cheap corpus menu: memory types, threads, entities)
                   present when --mode is 'full' or 'taxonomy'
  - semanticChunks (top-K halfvec nearest-neighbors from `memory_chunks`,
                    optionally narrowed to --scope-files; only populated when
                    stdin contains a prompt and embedding endpoint is reachable;
                    empty list on any failure — fail-open)
                   present when --mode is 'full' or 'semantic'

Modes (audit ticket #1, tiered retrieval):
  - full     (default): lexical + taxonomy + semantic/content/date, original
              single-call behavior plus the cheap menu.
  - lexical: index + importantIndex only, no embed call. Pass 1 of tiered flow.
  - taxonomy: corpus menu only, no embed call.
  - semantic: semanticChunks only, requires --scope-files for narrowing.
              Pass 2 of tiered flow, called after plugin ranks Pass 1 threads.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
import urllib.request
import urllib.error
from datetime import date
from pathlib import Path

# Force UTF-8 on stdout — payload contains characters like '→' that break
# Windows default cp1252. The plugin spawns this on Windows, reads stdout JSON.
sys.stdout.reconfigure(encoding="utf-8")

import psycopg2
import psycopg2.extras


# Semantic retrieval defaults. Override via CLI flags or env vars.
# Switched 2026-05-10 from LMStudio (1234, broken on 0.4.12 + RDNA 4) to Ollama
# (11434, clean ROCm support). SOLARISAEL_LMSTUDIO_URL still honored as a
# fallback name for back-compat. Response shape detection (ollama vs OpenAI)
# is automatic in embed_query.
DEFAULT_EMBED_URL = os.environ.get(
    "SOLARISAEL_EMBED_URL",
    os.environ.get("SOLARISAEL_LMSTUDIO_URL", "http://127.0.0.1:11434/api/embed"),
)
DEFAULT_EMBED_MODEL = os.environ.get(
    "SOLARISAEL_EMBED_MODEL", "qwen3-embedding:4b"
)
DEFAULT_SEMANTIC_TOP_K = 5
DEFAULT_SEMANTIC_MIN_SIM = 0.40

# Content search (pg_trgm GIN on memory_chunks.body, added 2026-05-19 zeal pass).
# Complements semantic by catching proper-noun / exact-string matches that
# cosine cosine-distance misses — e.g. "Beel" in a session-notes file whose
# overall topic isn't Beel doesn't cross 0.40 cosine but DOES word_similarity-match
# the chunk where Beel actually appears.
DEFAULT_CONTENT_TOP_K = 5
DEFAULT_CONTENT_MIN_SIM = 0.30  # word_similarity threshold; 0.30 = "noticeable substring"

# Date retrieval (added 2026-05-23 — date-aware retrieval fix). Cap returned
# memories to avoid swamping context; date hits are direct user-targeted
# lookups so they should be small and high-signal, not a corpus dump.
DEFAULT_DATE_TOP_K = 6
DEFAULT_DATE_BODY_EXCERPT_CHARS = 800

EMBED_TIMEOUT_SECS = 5.0

# Per-turn retrieval visibility (2026-06-22). db-only memories — auto-recorded
# sessions and paper-boats the OhMyPi session/sleep tools write straight to the
# DB, never to a curated memory/*.md path — are normally excluded from injection
# so the auto-session firehose can't swamp context. Paper-boats are the lone
# exception: one deliberate, high-signal handoff per session that MUST stay
# retrievable (a GLOSSOPETRAE thread lived only in boat 2147 and was invisible
# to every pass). One policy, applied at every pass; assumes table alias `m`.
RETRIEVAL_VISIBILITY_SQL = (
    "(m.source_path NOT LIKE 'db-only/%%' OR m.type = 'paper-boat')"
)

# Date extraction regex — matches any YYYY-MM-DD substring. Used by the
# date pass to pull date tokens out of the user's prompt (or recall query).
# Conservative validation happens in extract_query_dates: we drop tokens
# that don't parse as real dates (e.g. 2026-13-45).
_DATE_TOKEN_RE = re.compile(r"\b(\d{4})-(\d{2})-(\d{2})\b")

# Stopwords stripped from content queries before word_similarity. Reason:
# pg_trgm.word_similarity finds the BEST-matching substring in body, so
# "with" appearing in body matches the query word "with" at ws=1.0 — false
# positive. Stripping common words means content search only fires on the
# meaningful tokens. If query becomes empty after stripping, content pass
# returns no results (correct — query was all noise words).
_CONTENT_STOPWORDS = frozenset({
    "a", "an", "and", "are", "as", "at", "be", "been", "but", "by", "do",
    "for", "from", "had", "has", "have", "i", "if", "in", "is", "it", "its",
    "of", "on", "or", "so", "than", "that", "the", "their", "them", "then",
    "there", "these", "they", "this", "to", "too", "very", "was", "were",
    "what", "when", "where", "which", "who", "why", "will", "with", "would",
    "you", "your", "about", "into", "onto", "over", "would", "my", "me",
})


def filter_content_query(query: str) -> str:
    """Strip common stopwords from a content-search query.

    pg_trgm.word_similarity returns the max similarity of any substring in
    body that matches query. If query contains "with" and body contains
    "with" anywhere, word_similarity returns 1.0 — false positive. By
    stripping stopwords we leave only meaningful tokens (proper nouns,
    technical terms, content words) for the match.

    Returns empty string if nothing meaningful remains (caller should skip
    the content pass in that case).
    """
    if not query:
        return ""

    tokens = [t for t in query.lower().split() if t and t not in _CONTENT_STOPWORDS]

    # Reconstruct as a single space-separated string for word_similarity.
    # Preserve the order of meaningful words (matters for word_similarity's
    # continuous-extent matching in body).
    return " ".join(tokens).strip()


def read_env_file(path: Path) -> dict[str, str]:
    out: dict[str, str] = {}
    if not path.exists():
        return out

    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, _, value = line.partition("=")
        out[key.strip()] = value.strip()

    return out


def substrate_env(room_dir: Path) -> dict[str, str]:
    # Substrate moved kodo/substrate/ → house/substrate/ on 2026-05-07
    # (shared infra used by both rooms). Resolve relative-first under the vault
    # root, fall back to PG* env vars or hardcoded host fallback.
    shared_root = room_dir.parent
    env_path = shared_root / "house" / "substrate" / ".env"
    values = read_env_file(env_path)

    for key in ("PGHOST", "PGPORT", "PGUSER", "PGPASSWORD", "PGDATABASE"):
        if os.environ.get(key):
            values[key] = os.environ[key]

    return values


def connect(env: dict[str, str]):
    return psycopg2.connect(
        host=env.get("PGHOST"),
        port=env.get("PGPORT"),
        user=env.get("PGUSER"),
        password=env.get("PGPASSWORD"),
        dbname=env.get("PGDATABASE"),
        connect_timeout=2,
    )


def load_index(conn, rooms=("kintsu", "house")) -> dict:
    with conn.cursor(cursor_factory=psycopg2.extras.DictCursor) as cur:
        cur.execute(
            f"""
            SELECT m.room, m.source_path, m.date, m.type, m.meta->>'one_line' AS one_line
            FROM memories m
            WHERE m.room = ANY(%s)
              AND {RETRIEVAL_VISIBILITY_SQL}
            """,
            (list(rooms),),
        )
        files: dict = {}
        for row in cur.fetchall():
            key = f"house/{row['source_path']}" if row["room"] == "house" else row["source_path"]
            files[key] = {
                "date": row["date"].isoformat() if row["date"] else None,
                "type": row["type"],
                "one_line": row["one_line"] or "",
            }

        cur.execute(
            f"""
            SELECT m.room, mt.thread_key, m.source_path AS file,
                   mt.lines_start, mt.lines_end, mt.context
            FROM memory_threads mt
            JOIN memories m ON m.id = mt.memory_id
            WHERE m.room = ANY(%s)
              AND {RETRIEVAL_VISIBILITY_SQL}
            ORDER BY mt.thread_key
            """,
            (list(rooms),),
        )
        threads: dict = {}
        for row in cur.fetchall():
            if row["room"] == "house":
                thread_key = f"house / {row['thread_key']}"
                file_path = f"house/{row['file']}"
                context = ("Shared house memory. " + (row["context"] or "")).strip()
            else:
                thread_key = row["thread_key"]
                file_path = row["file"]
                context = row["context"] or ""
            lines = []
            if row["lines_start"] is not None:
                lines = [row["lines_start"], row["lines_end"] or row["lines_start"]]
            threads.setdefault(thread_key, []).append({
                "file": file_path,
                "lines": lines or [0, 0],
                "context": context,
            })

        return {"files": files, "threads": threads}


def load_important_index(conn, room="kintsu") -> dict:
    with conn.cursor(cursor_factory=psycopg2.extras.DictCursor) as cur:
        cur.execute(
            """
            SELECT name, kind, summary, aliases, search_boost, weighty, pointer_files
            FROM named_entities
            WHERE room = %s
            ORDER BY weighty DESC, name
            """,
            (room,),
        )
        entries: dict = {}
        for row in cur.fetchall():
            entries[row["name"]] = {
                "type": row["kind"],
                "summary": row["summary"],
                "files": row["pointer_files"] or [],
                "aliases": list(row["aliases"] or []),
                "search_boost": row["search_boost"] or "",
                "weighty": bool(row["weighty"]),
            }

        return entries

def load_taxonomy(conn, rooms=("kintsu", "house"), room="kintsu", limit=16) -> dict:
    """Return a small self-describing menu of what retrieval can target.

    Cheap SQL only: no embeddings, no body excerpts. This is for alignment and
    recall targeting, not context injection.
    """
    with conn.cursor(cursor_factory=psycopg2.extras.DictCursor) as cur:
        cur.execute(
            """
            SELECT room, COALESCE(type, '') AS type, COUNT(*) AS count
            FROM memories
            WHERE room = ANY(%s)
              AND (source_path NOT LIKE 'db-only/%%' OR type = 'paper-boat')
            GROUP BY room, COALESCE(type, '')
            ORDER BY count DESC, room, type
            """,
            (list(rooms),),
        )
        memory_types = [
            {"room": row["room"], "type": row["type"] or "unknown", "count": int(row["count"])}
            for row in cur.fetchall()
        ]

        cur.execute(
            """
            SELECT mt.thread_key, COUNT(*) AS count
            FROM memory_threads mt
            JOIN memories m ON m.id = mt.memory_id
            WHERE m.room = ANY(%s)
              AND (m.source_path NOT LIKE 'db-only/%%' OR m.type = 'paper-boat')
            GROUP BY mt.thread_key
            ORDER BY count DESC, mt.thread_key
            LIMIT %s
            """,
            (list(rooms), limit),
        )
        thread_keys = [
            {"thread_key": row["thread_key"], "count": int(row["count"])}
            for row in cur.fetchall()
        ]

        cur.execute(
            """
            SELECT name, kind, weighty
            FROM named_entities
            WHERE room = %s
            ORDER BY weighty DESC, name
            LIMIT %s
            """,
            (room, limit),
        )
        named_entities = [
            {"name": row["name"], "kind": row["kind"], "weighty": bool(row["weighty"])}
            for row in cur.fetchall()
        ]

        return {
            "rooms": list(rooms),
            "memoryTypes": memory_types,
            "threadKeys": thread_keys,
            "namedEntities": named_entities,
        }


def embed_query(prompt: str, url: str, model: str) -> list[float] | None:
    """POST prompt to an embedding endpoint, return the vector or None on failure.

    Supports two response shapes:
      - Ollama (``/api/embed``): ``{"embeddings": [[...]]}``
      - OpenAI-compat (LMStudio ``/v1/embeddings``): ``{"data": [{"embedding": [...]}]}``

    Fail-open: any exception (timeout, JSON parse, model unavailable) returns
    None. Caller skips semantic retrieval and returns empty list.
    """
    body = json.dumps({"model": model, "input": prompt}).encode("utf-8")
    req = urllib.request.Request(
        url,
        data=body,
        headers={"Content-Type": "application/json"},
        method="POST",
    )

    try:
        with urllib.request.urlopen(req, timeout=EMBED_TIMEOUT_SECS) as resp:
            payload = json.loads(resp.read().decode("utf-8"))
            # Ollama shape first (current default).
            embeddings = payload.get("embeddings")
            if isinstance(embeddings, list) and embeddings:
                vec = embeddings[0]
                if isinstance(vec, list) and vec:
                    return [float(x) for x in vec]

            # OpenAI-compat fallback (LMStudio etc).
            data = payload.get("data") or []
            if data:
                vec = data[0].get("embedding")
                if isinstance(vec, list) and vec:
                    return [float(x) for x in vec]

            return None
    except (urllib.error.URLError, urllib.error.HTTPError, TimeoutError, ValueError, OSError):
        return None


def load_semantic_chunks(
    conn,
    rooms: tuple[str, ...],
    query_vec: list[float],
    top_k: int,
    min_sim: float,
    scope_files: list[str] | None = None,
) -> list[dict]:
    """Return top-K halfvec nearest-neighbors from `memory_chunks` for the rooms.

    Computes ``1 - (body_embedding <=> $vec)`` as cosine similarity. Filters by
    ``min_sim``. Returns a list of dicts ready for the TypeScript caller to
    render as excerpts.

    When ``scope_files`` is non-empty, restricts the search to chunks whose
    parent memory's ``source_path`` is in that list. This is the audit-ticket
    #1 narrowing: the plugin ranks threads via Pass 1 (lexical), extracts the
    files those active threads reference, and passes them here as the scope
    for Pass 2 (semantic). Without scope, falls back to room-wide search
    (the original ``mode=full`` behavior — backward compatible).
    """
    vec_str = "[" + ",".join(f"{x:.6f}" for x in query_vec) + "]"

    # The ``house/`` prefix is a cross-room rendering convention added by
    # load_index; the database column ``m.source_path`` stores bare paths.
    # Strip the prefix here so ANY-equality matches the column values.
    normalized_scope: list[str] | None = None
    if scope_files:
        normalized_scope = sorted({
            (p[len("house/"):] if p.startswith("house/") else p)
            for p in scope_files
            if p
        })

    filters = ["m.room = ANY(%s)", "mc.body_embedding IS NOT NULL"]
    params: list = [vec_str, list(rooms)]
    if normalized_scope:
        filters.append("m.source_path = ANY(%s)")
        params.append(normalized_scope)
    else:
        filters.append(RETRIEVAL_VISIBILITY_SQL)
    params.extend([vec_str, top_k])
    where = " AND ".join(filters)

    sql = f"""
        SELECT m.source_path,
               m.room,
               mc.chunk_index,
               mc.heading_path,
               mc.body,
               mc.char_start,
               mc.char_end,
               1 - (mc.body_embedding <=> %s::halfvec) AS sim
        FROM memory_chunks mc
        JOIN memories m ON m.id = mc.memory_id
        WHERE {where}
        ORDER BY mc.body_embedding <=> %s::halfvec
        LIMIT %s
    """
    with conn.cursor(cursor_factory=psycopg2.extras.DictCursor) as cur:
        cur.execute(sql, params)
        out: list[dict] = []
        for row in cur.fetchall():
            sim = float(row["sim"]) if row["sim"] is not None else 0.0
            if sim < min_sim:
                continue

            # Match house-prefix convention used by load_index for cross-room rendering.
            source_path = (
                f"house/{row['source_path']}" if row["room"] == "house" else row["source_path"]
            )
            out.append({
                "source_path": source_path,
                "room": row["room"],
                "chunk_index": int(row["chunk_index"]) if row["chunk_index"] is not None else 0,
                "heading_path": row["heading_path"] or "",
                "body": row["body"] or "",
                "char_start": int(row["char_start"]) if row["char_start"] is not None else 0,
                "char_end": int(row["char_end"]) if row["char_end"] is not None else 0,
                "sim": round(sim, 4),
            })

        return out


def load_content_chunks(
    conn,
    rooms: tuple[str, ...],
    query: str,
    top_k: int,
    min_sim: float,
    scope_files: list[str] | None = None,
) -> list[dict]:
    """Return top-K chunks where `word_similarity(query, body)` exceeds min_sim.

    This is the third retrieval pass (added 2026-05-19), complementing:
      - lexical thread/canon (Pass 1, plugin-side via index + importantIndex)
      - semantic cosine (Pass 2, halfvec nearest-neighbors)
      - content trigram (Pass 3, THIS — pg_trgm word_similarity on body)

    Content-search catches proper-noun and exact-string matches that semantic
    cosine misses. Example: "Beel" appears in a chunk of `2026-04-14_*.md`,
    but the file's overall topic isn't Beel — semantic cosine of "Beel" query
    against the chunk's full-text embedding may not cross threshold. Content
    trigram fires because the literal word is there.

    Uses the `memory_chunks_body_trgm` GIN index (added 2026-05-19). Without
    that index, this query would be sequential-scan slow.

    When `scope_files` is non-empty, restricts to those source paths (mirror
    of `load_semantic_chunks` scope behavior). Empty/None scope = room-wide.

    Fail-open: returns empty list on failure.
    """
    if not query or not query.strip():
        return []

    # Strip stopwords before content matching. Without this, common words
    # like "with" in query match common words in body and produce ws=1.0
    # false positives. See filter_content_query docstring.
    filtered_query = filter_content_query(query)
    if not filtered_query:
        # Query was all stopwords — no meaningful tokens to match. Skip.
        return []

    # Split the filtered query into words for the ILIKE filter. We require
    # AT LEAST ONE query word to appear as a literal substring in body, then
    # rank surviving chunks by word_similarity. This hybrid:
    #   - ILIKE filter eliminates trigram-fuzzy false positives (e.g., query
    #     "wagyu beef truffle butter" shouldn't match a chunk that just
    #     happens to share some trigrams with "butter" via "but")
    #   - word_similarity ranking provides fine-grained ordering within
    #     qualifying chunks (e.g., "Beel audience" chunks outrank chunks
    #     mentioning Beel only briefly)
    query_words = [w for w in filtered_query.split() if w]
    if not query_words:
        return []

    word_patterns = [f"%{w}%" for w in query_words]

    normalized_scope: list[str] | None = None
    if scope_files:
        normalized_scope = sorted({
            (p[len("house/"):] if p.startswith("house/") else p)
            for p in scope_files
            if p
        })

    # Use filtered_query for the SQL, not the original.
    query = filtered_query

    # Param order matches positional %s in final SQL, left-to-right:
    #   1. SELECT word_similarity(%s, mc.body)    ← query (for the SELECT score)
    #   2. WHERE m.room = ANY(%s)                 ← rooms
    #   3. WHERE mc.body ILIKE ANY(ARRAY[...])    ← word_patterns
    #   4. WHERE word_similarity(%s, ...) >= %s   ← query, min_sim
    #   5. WHERE m.source_path = ANY(%s)          ← normalized_scope (if present)
    #   6. LIMIT %s                               ← top_k
    filters = [
        "m.room = ANY(%s)",
        "mc.body ILIKE ANY(%s::text[])",
        "word_similarity(%s, mc.body) >= %s",
    ]
    params: list = [query, list(rooms), word_patterns, query, min_sim]

    if normalized_scope:
        filters.append("m.source_path = ANY(%s)")
        params.append(normalized_scope)
    else:
        filters.append(RETRIEVAL_VISIBILITY_SQL)

    params.append(top_k)
    where = " AND ".join(filters)

    sql = f"""
        SELECT m.source_path,
               m.room,
               mc.chunk_index,
               mc.heading_path,
               mc.body,
               mc.char_start,
               mc.char_end,
               word_similarity(%s, mc.body) AS ws
        FROM memory_chunks mc
        JOIN memories m ON m.id = mc.memory_id
        WHERE {where}
        ORDER BY ws DESC
        LIMIT %s
    """
    with conn.cursor(cursor_factory=psycopg2.extras.DictCursor) as cur:
        cur.execute(sql, params)
        out: list[dict] = []
        for row in cur.fetchall():
            ws = float(row["ws"]) if row["ws"] is not None else 0.0
            source_path = (
                f"house/{row['source_path']}" if row["room"] == "house" else row["source_path"]
            )
            out.append({
                "source_path": source_path,
                "room": row["room"],
                "chunk_index": int(row["chunk_index"]) if row["chunk_index"] is not None else 0,
                "heading_path": row["heading_path"] or "",
                "body": row["body"] or "",
                "char_start": int(row["char_start"]) if row["char_start"] is not None else 0,
                "char_end": int(row["char_end"]) if row["char_end"] is not None else 0,
                "ws": round(ws, 4),
            })

        return out


def extract_query_dates(query: str) -> list:
    """Pull validated date objects out of any YYYY-MM-DD tokens in the query.

    Date-aware retrieval (added 2026-05-23). When the user/dragon asks about
    a specific date — "what happened 2026-05-22", "show me yesterday's session
    (2026-05-22)", etc. — the prior 3-pass pipeline missed cross-midnight
    stitched files because `source_path` was never indexed for retrieval and
    the body trigrams don't match raw ISO dates well. This extractor feeds
    the new `dateMatches` pass.

    Returns sorted unique date objects. Invalid date tokens (2026-13-45,
    9999-99-99) are silently dropped — we don't want to fail the whole
    retrieval call because the user wrote a typo.
    """
    out: set = set()
    if not query:
        return []

    for m in _DATE_TOKEN_RE.finditer(query):
        try:
            out.add(date(int(m.group(1)), int(m.group(2)), int(m.group(3))))
        except ValueError:
            continue

    return sorted(out)


def load_date_matches(
    conn,
    rooms: tuple,
    query_dates: list,
    top_k: int,
    excerpt_chars: int,
) -> list[dict]:
    """Return memories whose `dates` array intersects any of the query dates.

    Direct, structural match — no embedding, no trigram. Hits when the user
    asks about a specific YYYY-MM-DD that's authoritatively tagged on the
    memory (via record_memory.py's auto-parse + --also-date, backfilled by
    migration 0009 for older entries).

    `dates && %s::date[]` uses the GIN index `memories_dates_gin`. Falls back
    to scalar `date = ANY` for the primary `date` column too, since some
    very-old rows may have `date` set but `dates` empty if backfill missed
    them (shouldn't happen post-0009 but cheap to belt+suspender).

    Returns full memory metadata + a body excerpt. Caller renders as a
    high-priority section above other excerpt types because a direct date
    match means the user literally asked for THIS memory.
    """
    if not query_dates:
        return []

    sql = f"""
        SELECT m.id, m.room, m.source_path, m.title, m.type,
               m.date, m.dates, m.threads,
               LEFT(m.body, %s) AS body_excerpt,
               OCTET_LENGTH(m.body) AS body_full_chars
        FROM memories m
        WHERE m.room = ANY(%s)
          AND {RETRIEVAL_VISIBILITY_SQL}
          AND (m.dates && %s::date[] OR m.date = ANY(%s::date[]))
        ORDER BY m.date DESC NULLS LAST, m.id DESC
        LIMIT %s
    """
    with conn.cursor(cursor_factory=psycopg2.extras.DictCursor) as cur:
        cur.execute(
            sql,
            (excerpt_chars, list(rooms), query_dates, query_dates, top_k),
        )
        out: list[dict] = []
        for row in cur.fetchall():
            source_path = (
                f"house/{row['source_path']}" if row["room"] == "house" else row["source_path"]
            )
            out.append({
                "memory_id": int(row["id"]),
                "source_path": source_path,
                "room": row["room"],
                "title": row["title"] or "",
                "type": row["type"] or "",
                "date": row["date"].isoformat() if row["date"] else None,
                "dates": [d.isoformat() for d in (row["dates"] or [])],
                "threads": list(row["threads"] or []),
                "body_excerpt": row["body_excerpt"] or "",
                "body_full_chars": int(row["body_full_chars"] or 0),
            })

        return out


def read_prompt_from_stdin() -> str:
    """Read the user prompt off stdin. Empty string means no semantic search."""
    if sys.stdin.isatty():
        return ""

    try:
        return sys.stdin.read().strip()
    except Exception:
        return ""


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--room-dir", required=True)
    parser.add_argument(
        "--room",
        default=None,
        help="Room name (kodo|kintsu). If omitted, derived from --room-dir basename.",
    )
    parser.add_argument(
        "--semantic-top-k",
        type=int,
        default=DEFAULT_SEMANTIC_TOP_K,
        help=f"Max halfvec neighbors to return (default {DEFAULT_SEMANTIC_TOP_K}).",
    )
    parser.add_argument(
        "--semantic-min-sim",
        type=float,
        default=DEFAULT_SEMANTIC_MIN_SIM,
        help=f"Drop chunks below this cosine similarity (default {DEFAULT_SEMANTIC_MIN_SIM}).",
    )
    parser.add_argument(
        "--embed-url",
        "--lmstudio-url",  # back-compat alias; index.ts may still pass --lmstudio-url
        dest="embed_url",
        default=DEFAULT_EMBED_URL,
        help="Embedding endpoint URL (Ollama /api/embed or OpenAI /v1/embeddings).",
    )
    parser.add_argument(
        "--embed-model",
        default=DEFAULT_EMBED_MODEL,
        help="Embedding model id (Ollama tag like 'qwen3-embedding:4b' or LMStudio model name).",
    )
    parser.add_argument(
        "--mode",
        choices=("full", "lexical", "semantic", "content", "date", "taxonomy"),
        default="full",
        help=(
            "Retrieval mode (audit ticket #1 + 2026-05-19 GIN pass + 2026-05-23 date pass). "
            "'full': lexical + important + taxonomy + global semantic + content + date. "
            "'lexical': index + importantIndex only, no embed call, no content. "
            "'taxonomy': cheap corpus menu only, no embed call. "
            "'semantic': embed prompt + narrowed semantic against --scope-files. "
            "'content': pg_trgm word_similarity on memory_chunks.body, no embed call. "
            "'date': YYYY-MM-DD-token lookup against memories.dates GIN. No embed. "
            "Plugin's tiered flow: lexical first → rank threads → semantic scoped + "
            "content global + date global → merge."
        ),
    )
    parser.add_argument(
        "--scope-files",
        default=None,
        help=(
            "Comma-separated source paths to narrow semantic and content search to. "
            "Only meaningful for --mode=semantic, --mode=content (or --mode=full). "
            "Empty/omitted means search the full room corpus."
        ),
    )
    parser.add_argument(
        "--content-top-k",
        type=int,
        default=DEFAULT_CONTENT_TOP_K,
        help=f"Max content (trigram) chunks to return (default {DEFAULT_CONTENT_TOP_K}).",
    )
    parser.add_argument(
        "--content-min-sim",
        type=float,
        default=DEFAULT_CONTENT_MIN_SIM,
        help=f"Drop content chunks below this word_similarity (default {DEFAULT_CONTENT_MIN_SIM}).",
    )
    parser.add_argument(
        "--date-top-k",
        type=int,
        default=DEFAULT_DATE_TOP_K,
        help=f"Max memories returned by the date pass (default {DEFAULT_DATE_TOP_K}).",
    )
    parser.add_argument(
        "--date-excerpt-chars",
        type=int,
        default=DEFAULT_DATE_BODY_EXCERPT_CHARS,
        help=(
            f"Truncate body excerpts in date hits to this many chars "
            f"(default {DEFAULT_DATE_BODY_EXCERPT_CHARS})."
        ),
    )

    args = parser.parse_args()

    room_dir = Path(args.room_dir).resolve()
    # Derive room name from cwd basename if not explicitly passed. Fixes the
    # 2026-05-04 bug where opencode-Kodo's session loaded Kintsu memory because
    # load_index/load_important_index defaulted to "kintsu" regardless of cwd.
    room_name = (args.room or room_dir.name or "kodo").lower()
    if room_name not in ("kodo", "kintsu", "tuner"):
        room_name = "kodo"  # safe fallback; both rooms have substrate rows

    prompt = read_prompt_from_stdin()
    scope_files: list[str] | None = None
    if args.scope_files:
        scope_files = [s.strip() for s in args.scope_files.split(",") if s.strip()]

    env = substrate_env(room_dir)
    conn = connect(env)
    try:
        payload: dict = {}

        # Lexical pass (Pass 1 of tiered retrieval). Loads the full thread
        # index + named-entity index for plugin-side ranking. mode=semantic
        # skips these to avoid wasted work in the two-stage flow.
        if args.mode in ("full", "lexical"):
            payload["index"] = load_index(conn, rooms=(room_name, "house"))
            payload["importantIndex"] = load_important_index(conn, room=room_name)

        if args.mode in ("full", "taxonomy"):
            payload["taxonomy"] = load_taxonomy(conn, rooms=(room_name, "house"), room=room_name)

        # Semantic pass (Pass 2 of tiered retrieval). Embeds prompt + queries
        # memory_chunks for top-K cosine neighbors, optionally narrowed to
        # active-thread files via --scope-files. mode=lexical skips this
        # entirely (no embed call, no postgres query). Fail-open: any
        # failure produces an empty semanticChunks list and a stderr line.
        if args.mode in ("full", "semantic"):
            chunks: list[dict] = []
            if prompt:
                # Auto-wake Ollama before embedding, same contract the WRITERS
                # already hold (record_memory.py / record_cabinet_entry.py call
                # ensure_ollama_up before embedding). 2026-06-05 lesson: a
                # sleeping Ollama silently returned 0 semantic matches for a
                # whole session — fail-open masked a dead embedder as "no
                # matches." The reader should guarantee the embedder just like
                # the writer does.
                #
                # Asymmetry with writers ON PURPOSE: reads do NOT stop Ollama
                # afterward. The next pre-turn injection also needs semantic;
                # start-stopping per query would re-dark it and pay a ~6s
                # cold-start every time. Reads keep the lights on.
                #
                # Best-effort: if the helper import fails for any reason, fall
                # straight through to embed_query — never break fail-open.
                try:
                    substrate_dir = (
                        Path(args.room_dir).resolve().parent / "house" / "substrate"
                    )
                    if str(substrate_dir) not in sys.path:
                        sys.path.insert(0, str(substrate_dir))
                    from embed_4b_pass import ensure_ollama_up
                    ensure_ollama_up()
                except Exception as err:
                    print(f"semantic: ollama auto-wake skipped: {err}", file=sys.stderr)

                vec = embed_query(prompt, args.embed_url, args.embed_model)
                if vec is None:
                    print(
                        "semantic: embed failed (embedding endpoint unreachable or model error)",
                        file=sys.stderr,
                    )
                else:
                    try:
                        chunks = load_semantic_chunks(
                            conn,
                            rooms=(room_name, "house"),
                            query_vec=vec,
                            top_k=args.semantic_top_k,
                            min_sim=args.semantic_min_sim,
                            scope_files=scope_files,
                        )
                    except Exception as err:
                        print(f"semantic: query failed: {err}", file=sys.stderr)

            payload["semanticChunks"] = chunks

        # Content pass (added 2026-05-19, pg_trgm GIN on memory_chunks.body).
        # No embed call, no scope-narrowing by default — catches proper-noun /
        # exact-string matches the semantic cosine misses. Cheap due to the
        # trigram index. mode=lexical skips this; mode=content runs ONLY this.
        if args.mode in ("full", "content"):
            content_chunks: list[dict] = []
            if prompt:
                try:
                    content_chunks = load_content_chunks(
                        conn,
                        rooms=(room_name, "house"),
                        query=prompt,
                        top_k=args.content_top_k,
                        min_sim=args.content_min_sim,
                        scope_files=scope_files,
                    )
                except Exception as err:
                    print(f"content: query failed: {err}", file=sys.stderr)

            payload["contentChunks"] = content_chunks

        # Date pass (added 2026-05-23 — date-aware retrieval fix). Extracts
        # YYYY-MM-DD tokens from the prompt and queries memories.dates (GIN
        # array). Direct authoritative match: no fuzz, no embedding, no
        # threshold. Hits when the user/dragon literally names a date.
        # mode=date runs ONLY this; mode=full includes it; lexical/semantic/
        # content skip it.
        if args.mode in ("full", "date"):
            date_matches: list[dict] = []
            query_dates = extract_query_dates(prompt)
            if query_dates:
                try:
                    date_matches = load_date_matches(
                        conn,
                        rooms=(room_name, "house"),
                        query_dates=query_dates,
                        top_k=args.date_top_k,
                        excerpt_chars=args.date_excerpt_chars,
                    )
                except Exception as err:
                    print(f"date: query failed: {err}", file=sys.stderr)

            payload["dateMatches"] = date_matches
            payload["queryDates"] = [d.isoformat() for d in query_dates]
    finally:
        conn.close()

    print(json.dumps(payload, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
