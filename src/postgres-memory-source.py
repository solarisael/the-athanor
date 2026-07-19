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
  - searchTerms    (meaningful query terms used by the term-aware search pass)
                   present when --mode is 'full' or 'candidates'
  - searchCandidates
                   (normalized term-aware candidates from entities, threads,
                    memories, and lesson rails; no embedding call required)
                   present when --mode is 'full' or 'candidates'
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

# Term-aware candidate search (2026-07-01 retrieval roadmap). This is the
# publishable/lightweight lane: every meaningful query term should contribute
# to rank without requiring pgvector or an awake embedding model.
DEFAULT_CANDIDATE_TOP_K = 12
DEFAULT_CANDIDATE_EXCERPT_CHARS = 700
DEFAULT_CANDIDATE_MAX_TERMS = 10

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

ERASURE_SCORE_DEMOTION = 8.0
ARCHIVE_SCORE_DEMOTION = 10.0


def detect_erasure_columns(conn) -> dict[str, bool]:
    """Return migration 0024 capabilities without making them boot-critical."""
    try:
        with conn.cursor() as cur:
            cur.execute(
                """
                SELECT column_name
                FROM information_schema.columns
                WHERE table_schema = current_schema()
                  AND table_name = 'memories'
                  AND column_name IN ('superseded_by', 'archived_at')
                """
            )
            found = {row[0] for row in cur.fetchall()}
    except Exception:
        conn.rollback()
        found = set()
    return {
        "superseded_by": "superseded_by" in found,
        "archived_at": "archived_at" in found,
    }


def erasure_select(columns: dict[str, bool], alias: str = "m") -> str:
    archived = f"{alias}.archived_at" if columns.get("archived_at") else "NULL::timestamptz"
    if columns.get("superseded_by"):
        superseded = f"{alias}.superseded_by IS NOT NULL"
    else:
        superseded = "FALSE"
    return f"{archived} AS archived_at, {superseded} AS superseded"


def erasure_filter(
    columns: dict[str, bool],
    *,
    alias: str = "m",
    include_archived: bool = False,
) -> str:
    """Build an additive lifecycle filter; absent columns become no-ops."""
    filters = []
    if columns.get("archived_at") and not include_archived:
        filters.append(f"{alias}.archived_at IS NULL")
    return " AND ".join(filters) or "TRUE"


def lifecycle_flags(row) -> tuple[bool, bool, str | None]:
    archived_at = row.get("archived_at") if hasattr(row, "get") else None
    archived = archived_at is not None
    superseded = bool(row.get("superseded")) if hasattr(row, "get") else False
    return archived, superseded, (
        archived_at.isoformat() if hasattr(archived_at, "isoformat") else archived_at
    )

# Date extraction regex — matches any YYYY-MM-DD substring. Used by the
# date pass to pull date tokens out of the user's prompt (or recall query).
# Conservative validation happens in extract_query_dates: we drop tokens
# that don't parse as real dates (e.g. 2026-13-45).
_DATE_TOKEN_RE = re.compile(r"\b(\d{4})-(\d{2})-(\d{2})\b")

# Term extraction for the candidate lane. Keep technical tokens such as
# pgvector, bm25, path-ish values, snake_case, and hyphenated names intact,
# then also add split pieces so `solarisael-house` can match `solarisael`
# and `house` fields independently.
_SEARCH_TERM_RE = re.compile(r"[a-z0-9][a-z0-9_:+#./-]*", re.IGNORECASE)

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

_CANDIDATE_STOPWORDS = _CONTENT_STOPWORDS | frozenset({
    "alright",
    "also",
    "basically",
    "could",
    "enabled",
    "even",
    "good",
    "guess",
    "into",
    "just",
    "kinda",
    "like",
    "looksie",
    "maybe",
    "much",
    "need",
    "our",
    "pretty",
    "probably",
    "really",
    "sample",
    "see",
    "seems",
    "should",
    "size",
    "stuff",
    "thing",
    "things",
    "think",
    "tool",
    "try",
    "use",
    "wanna",
    "want",
    "well",
    "without",
    "working",
    "would",
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


def extract_search_terms(query: str, max_terms: int = DEFAULT_CANDIDATE_MAX_TERMS) -> list[str]:
    """Return ordered meaningful terms for lightweight candidate search.

    This is deliberately not an NLP layer. It is the plain-line search spine:
    keep exact technical tokens, split compound tokens into usable pieces, drop
    true stopwords, dedupe in query order, and cap the list so each source query
    stays cheap.
    """
    if not query:
        return []

    terms: list[str] = []
    seen: set[str] = set()

    def add(term: str) -> None:
        cleaned = term.strip("._:/-+").lower()
        if len(cleaned) <= 1 or cleaned in _CANDIDATE_STOPWORDS or cleaned in seen:
            return
        seen.add(cleaned)
        terms.append(cleaned)

    for match in _SEARCH_TERM_RE.finditer(query):
        token = match.group(0)
        add(token)
        for part in re.split(r"[-_./:+#]+", token):
            add(part)
        if len(terms) >= max_terms:
            break

    return terms[:max_terms]


def _candidate_pointer_path(pointer) -> str:
    if not pointer:
        return ""
    if isinstance(pointer, dict):
        return str(pointer.get("file") or pointer.get("source_path") or "")
    return str(pointer)


def _candidate_source_path(room: str, source_path) -> str:
    normalized = _candidate_pointer_path(source_path)
    if not normalized:
        return ""
    return f"house/{normalized}" if room == "house" else normalized


def _matched_candidate_terms(terms: list[str], *parts: object) -> list[str]:
    haystack = " ".join(str(part or "").lower() for part in parts)
    return [term for term in terms if term in haystack]


def _candidate_score(
    *,
    source: str,
    coverage: float,
    raw_rank: float = 0.0,
    title_hit: bool = False,
    path_hit: bool = False,
    archived: bool = False,
    superseded: bool = False,
) -> float:
    # Source priors are intentionally small; term coverage is the main signal.
    source_prior = {
        "entity": 2.4,
        "coding_lesson": 2.0,
        "project_lesson": 2.0,
        "thread": 1.7,
        "memory": 1.2,
    }.get(source, 1.0)
    lifecycle_penalty = (
        (ARCHIVE_SCORE_DEMOTION if archived else 0.0)
        + (ERASURE_SCORE_DEMOTION if superseded else 0.0)
    )
    return round(
        source_prior
        + (coverage * 4.0)
        + (float(raw_rank or 0.0) * 2.0)
        + (1.0 if title_hit else 0.0)
        + (0.5 if path_hit else 0.0)
        - lifecycle_penalty,
        4,
    )


def _candidate(
    *,
    source: str,
    source_table: str,
    source_id,
    room: str | None,
    title: str,
    excerpt: str,
    terms: list[str],
    source_path: str = "",
    heading_path: str = "",
    raw_rank: float = 0.0,
    title_hit: bool = False,
    path_hit: bool = False,
    extra_haystack: str = "",
    reasons: list[str] | None = None,
    kind: str = "",
    weighty: bool = False,
    archived: bool = False,
    superseded: bool = False,
    archived_at=None,
) -> dict | None:
    matched_terms = _matched_candidate_terms(
        terms, title, excerpt, source_path, heading_path, extra_haystack,
    )
    if not matched_terms and raw_rank <= 0:
        return None

    coverage = (len(matched_terms) / len(terms)) if terms else 0.0
    missing_terms = [term for term in terms if term not in matched_terms]
    candidate_reasons = list(reasons or [])
    if superseded and "superseded" not in candidate_reasons:
        candidate_reasons.append("superseded")
    if archived and "archived" not in candidate_reasons:
        candidate_reasons.append("archived")
    return {
        "id": f"{source_table}:{source_id}",
        "source": source,
        "source_table": source_table,
        "source_id": source_id,
        "room": room or "",
        "title": title or "",
        "source_path": source_path or "",
        "heading_path": heading_path or "",
        "excerpt": excerpt or "",
        "raw_rank": round(float(raw_rank or 0.0), 4),
        "score": _candidate_score(
            source=source,
            coverage=coverage,
            raw_rank=raw_rank,
            title_hit=title_hit,
            path_hit=path_hit,
            archived=archived,
            superseded=superseded,
        ),
        "term_coverage": round(coverage, 4),
        "matched_terms": matched_terms,
        "missing_terms": missing_terms,
        "reasons": candidate_reasons,
        "kind": kind or "",
        "weighty": bool(weighty),
        "archived": bool(archived),
        "archived_at": archived_at.isoformat() if hasattr(archived_at, "isoformat") else archived_at,
        "superseded": bool(superseded),
    }


def load_search_candidates(
    conn,
    rooms: tuple,
    query: str,
    top_k: int,
    excerpt_chars: int = DEFAULT_CANDIDATE_EXCERPT_CHARS,
    erasure_columns: dict[str, bool] | None = None,
    include_archived: bool = False,
) -> tuple[list[str], list[dict]]:
    """Return normalized lightweight retrieval candidates.

    This is the no-embedding search pass. It uses existing Postgres indexes
    where available (`tsvector`, pg_trgm-backed ILIKE/similarity surfaces) and
    then computes term coverage in Python so a result matching five query terms
    outranks a result matching one loose term.
    """
    terms = extract_search_terms(query)
    if not terms:
        return [], []

    text_query = " ".join(terms)
    term_patterns = [f"%{term}%" for term in terms]
    room_list = list(rooms)
    candidates: list[dict] = []
    erasure_columns = erasure_columns or {}
    memory_filter = erasure_filter(
        erasure_columns, include_archived=include_archived,
    )

    with conn.cursor(cursor_factory=psycopg2.extras.DictCursor) as cur:
        cur.execute(
            """
            WITH q AS (SELECT websearch_to_tsquery('english', %s) AS query)
            SELECT ne.id, ne.room, ne.name, ne.kind, ne.summary, ne.aliases,
                   ne.pointer_files, ne.weighty,
                   ts_rank_cd(ne.summary_tsv, q.query, 32) AS raw_rank,
                   ne.name ILIKE ANY(%s::text[]) AS title_hit
            FROM named_entities ne, q
            WHERE ne.room = ANY(%s)
              AND (
                ne.summary_tsv @@ q.query
                OR ne.name ILIKE ANY(%s::text[])
                OR ne.summary ILIKE ANY(%s::text[])
                OR EXISTS (
                  SELECT 1 FROM unnest(ne.aliases) AS alias_value
                  WHERE alias_value ILIKE ANY(%s::text[])
                )
              )
            LIMIT %s
            """,
            (text_query, term_patterns, room_list, term_patterns, term_patterns, term_patterns, top_k),
        )
        for row in cur.fetchall():
            aliases = " ".join(row["aliases"] or [])
            source_path = _candidate_source_path(
                row["room"], (row["pointer_files"] or [""])[0],
            )
            item = _candidate(
                source="entity",
                source_table="named_entities",
                source_id=int(row["id"]),
                room=row["room"],
                title=row["name"] or "",
                excerpt=row["summary"] or "",
                terms=terms,
                source_path=source_path,
                raw_rank=float(row["raw_rank"] or 0.0),
                title_hit=bool(row["title_hit"]),
                extra_haystack=f"{row['kind'] or ''} {aliases}",
                kind=row["kind"] or "",
                weighty=bool(row["weighty"]),
                reasons=["named entity", "alias/summary/title search"],
            )
            if item:
                candidates.append(item)

        cur.execute(
            f"""
            SELECT mt.id, m.id AS memory_id, m.room, mt.thread_key, mt.context,
                   m.source_path, m.type,
                   {erasure_select(erasure_columns)},
                   GREATEST(
                     similarity(mt.thread_key, %s),
                     similarity(COALESCE(mt.context, ''), %s)
                   ) AS raw_rank,
                   mt.thread_key ILIKE ANY(%s::text[]) AS title_hit,
                   m.source_path ILIKE ANY(%s::text[]) AS path_hit
            FROM memory_threads mt
            JOIN memories m ON m.id = mt.memory_id
            WHERE m.room = ANY(%s)
              AND {RETRIEVAL_VISIBILITY_SQL}
              AND {memory_filter}
              AND (
                mt.thread_key ILIKE ANY(%s::text[])
                OR mt.context ILIKE ANY(%s::text[])
                OR m.source_path ILIKE ANY(%s::text[])
              )
            LIMIT %s
            """,
            (
                text_query, text_query, term_patterns, term_patterns, room_list,
                term_patterns, term_patterns, term_patterns, top_k,
            ),
        )
        for row in cur.fetchall():
            archived, superseded, archived_at = lifecycle_flags(row)
            item = _candidate(
                source="thread",
                source_table="memory_threads",
                source_id=int(row["id"]),
                room=row["room"],
                title=row["thread_key"] or "",
                excerpt=row["context"] or "",
                terms=terms,
                source_path=_candidate_source_path(row["room"], row["source_path"]),
                raw_rank=float(row["raw_rank"] or 0.0),
                title_hit=bool(row["title_hit"]),
                path_hit=bool(row["path_hit"]),
                extra_haystack=row["type"] or "",
                reasons=["memory thread", "thread/context/path search"],
                archived=archived,
                superseded=superseded,
                archived_at=archived_at,
            )
            if item:
                candidates.append(item)

        cur.execute(
            f"""
            WITH q AS (SELECT websearch_to_tsquery('english', %s) AS query)
            SELECT m.id, m.room, m.title, m.source_path, m.type,
                   LEFT(m.body, %s) AS excerpt,
                   ts_rank_cd(m.body_tsv, q.query, 32) AS raw_rank,
                   m.title ILIKE ANY(%s::text[]) AS title_hit,
                   m.source_path ILIKE ANY(%s::text[]) AS path_hit,
                   array_to_string(m.threads, ' ') AS thread_text,
                   {erasure_select(erasure_columns)}
            FROM memories m, q
            WHERE m.room = ANY(%s)
              AND {RETRIEVAL_VISIBILITY_SQL}
              AND {memory_filter}
              AND (
                m.body_tsv @@ q.query
                OR m.title ILIKE ANY(%s::text[])
                OR m.source_path ILIKE ANY(%s::text[])
                OR EXISTS (
                  SELECT 1 FROM unnest(m.threads) AS thread_value
                  WHERE thread_value ILIKE ANY(%s::text[])
                )
              )
            ORDER BY raw_rank DESC, m.date DESC NULLS LAST, m.id DESC
            LIMIT %s
            """,
            (
                text_query, excerpt_chars, term_patterns, term_patterns, room_list,
                term_patterns, term_patterns, term_patterns, top_k,
            ),
        )
        for row in cur.fetchall():
            archived, superseded, archived_at = lifecycle_flags(row)
            item = _candidate(
                source="memory",
                source_table="memories",
                source_id=int(row["id"]),
                room=row["room"],
                title=row["title"] or row["source_path"] or "",
                excerpt=row["excerpt"] or "",
                terms=terms,
                source_path=_candidate_source_path(row["room"], row["source_path"]),
                raw_rank=float(row["raw_rank"] or 0.0),
                title_hit=bool(row["title_hit"]),
                path_hit=bool(row["path_hit"]),
                extra_haystack=f"{row['type'] or ''} {row['thread_text'] or ''}",
                reasons=["memory full-text", "title/path/thread/body search"],
                archived=archived,
                superseded=superseded,
                archived_at=archived_at,
            )
            if item:
                candidates.append(item)

        cur.execute(
            """
            WITH q AS (SELECT websearch_to_tsquery('english', %s) AS query)
            SELECT id, scope, project, title, lesson, shape, tags,
                   ts_rank_cd(lesson_tsv, q.query, 32) AS raw_rank,
                   title ILIKE ANY(%s::text[]) AS title_hit
            FROM coding_lessons, q
            WHERE lesson_tsv @@ q.query
               OR title ILIKE ANY(%s::text[])
               OR shape ILIKE ANY(%s::text[])
               OR project ILIKE ANY(%s::text[])
               OR scope ILIKE ANY(%s::text[])
            ORDER BY raw_rank DESC, updated_at DESC, id DESC
            LIMIT %s
            """,
            (
                text_query, term_patterns, term_patterns, term_patterns,
                term_patterns, term_patterns, top_k,
            ),
        )
        for row in cur.fetchall():
            item = _candidate(
                source="coding_lesson",
                source_table="coding_lessons",
                source_id=int(row["id"]),
                room="",
                title=row["title"] or "",
                excerpt=row["lesson"] or "",
                terms=terms,
                raw_rank=float(row["raw_rank"] or 0.0),
                title_hit=bool(row["title_hit"]),
                extra_haystack=(
                    f"{row['scope'] or ''} {row['project'] or ''} "
                    f"{row['shape'] or ''} {' '.join(row['tags'] or [])}"
                ),
                reasons=["coding lesson", "lesson/title/shape/tag search"],
            )
            if item:
                candidates.append(item)

        cur.execute(
            """
            WITH q AS (SELECT websearch_to_tsquery('english', %s) AS query)
            SELECT id, project, title, lesson, tags,
                   ts_rank_cd(lesson_tsv, q.query, 32) AS raw_rank,
                   title ILIKE ANY(%s::text[]) AS title_hit
            FROM project_lessons, q
            WHERE lesson_tsv @@ q.query
               OR title ILIKE ANY(%s::text[])
               OR project ILIKE ANY(%s::text[])
            ORDER BY raw_rank DESC, updated_at DESC, id DESC
            LIMIT %s
            """,
            (text_query, term_patterns, term_patterns, term_patterns, top_k),
        )
        for row in cur.fetchall():
            item = _candidate(
                source="project_lesson",
                source_table="project_lessons",
                source_id=int(row["id"]),
                room="",
                title=row["title"] or "",
                excerpt=row["lesson"] or "",
                terms=terms,
                raw_rank=float(row["raw_rank"] or 0.0),
                title_hit=bool(row["title_hit"]),
                extra_haystack=f"{row['project'] or ''} {' '.join(row['tags'] or [])}",
                reasons=["project lesson", "lesson/title/project search"],
            )
            if item:
                candidates.append(item)

    candidates.sort(
        key=lambda item: (
            item.get("score", 0),
            item.get("term_coverage", 0),
            item.get("raw_rank", 0),
        ),
        reverse=True,
    )
    return terms, candidates[:top_k]


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


def load_index(
    conn,
    rooms=("kintsu", "house"),
    erasure_columns: dict[str, bool] | None = None,
    include_archived: bool = False,
) -> dict:
    erasure_columns = erasure_columns or {}
    memory_filter = erasure_filter(
        erasure_columns, include_archived=include_archived,
    )
    lifecycle_select = erasure_select(erasure_columns)
    with conn.cursor(cursor_factory=psycopg2.extras.DictCursor) as cur:
        cur.execute(
            f"""
            SELECT m.id AS memory_id, m.room, m.source_path, m.date, m.type,
                   m.meta->>'one_line' AS one_line, {lifecycle_select}
            FROM memories m
            WHERE m.room = ANY(%s)
              AND {RETRIEVAL_VISIBILITY_SQL}
              AND {memory_filter}
            """,
            (list(rooms),),
        )
        files: dict = {}
        for row in cur.fetchall():
            key = f"house/{row['source_path']}" if row["room"] == "house" else row["source_path"]
            files[key] = {
                "memory_id": int(row["memory_id"]),
                "date": row["date"].isoformat() if row["date"] else None,
                "type": row["type"],
                "one_line": row["one_line"] or "",
                "archived": row["archived_at"] is not None,
                "archived_at": row["archived_at"].isoformat() if row["archived_at"] else None,
                "superseded": bool(row["superseded"]),
            }

        cur.execute(
            f"""
            SELECT m.id AS memory_id, m.room, mt.thread_key, m.source_path AS file,
                   mt.lines_start, mt.lines_end, mt.context, {lifecycle_select}
            FROM memory_threads mt
            JOIN memories m ON m.id = mt.memory_id
            WHERE m.room = ANY(%s)
              AND {RETRIEVAL_VISIBILITY_SQL}
              AND {memory_filter}
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
                "memory_id": int(row["memory_id"]),
                "file": file_path,
                "lines": lines or [0, 0],
                "context": context,
                "archived": row["archived_at"] is not None,
                "archived_at": row["archived_at"].isoformat() if row["archived_at"] else None,
                "superseded": bool(row["superseded"]),
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

def load_cluster_staleness(
    conn,
    erasure_columns: dict[str, bool] | None = None,
    include_archived: bool = False,
) -> dict:
    """Cluster-map drift gauge (2026-07-09 roadmap: memory as navigable space).

    Compares the memory_clusters build time against retrieval-visible chunks
    embedded since. Consumers nudge a rebuild when fraction_unseen grows.
    """
    erasure_columns = erasure_columns or {}
    memory_filter = erasure_filter(
        erasure_columns, include_archived=include_archived,
    )
    with conn.cursor() as cur:
        cur.execute("SELECT max(created_at), count(*) FROM memory_clusters")
        built_at, clusters = cur.fetchone()
        cur.execute(
            f"""
            SELECT count(*) FROM memory_chunks c
            JOIN memories m ON m.id = c.memory_id
            WHERE c.body_embedding IS NOT NULL
              AND {RETRIEVAL_VISIBILITY_SQL}
              AND {memory_filter}
            """
        )
        chunks_total = cur.fetchone()[0]
        chunks_since = chunks_total
        if built_at is not None:
            cur.execute(
                f"""
                SELECT count(*) FROM memory_chunks c
                JOIN memories m ON m.id = c.memory_id
                WHERE c.body_embedding IS NOT NULL
                  AND {RETRIEVAL_VISIBILITY_SQL}
                  AND {memory_filter}
                  AND c.embedded_at > %s
                """,
                (built_at,),
            )
            chunks_since = cur.fetchone()[0]
    return {
        "built_at": built_at.isoformat() if built_at else None,
        "clusters": clusters,
        "chunks_total": chunks_total,
        "chunks_since_build": chunks_since,
        "fraction_unseen": round((chunks_since / chunks_total) if chunks_total else 0.0, 4),
    }


def fetch_memory(conn, memory_id=None, source_path=None, claimed_room=None) -> dict:
    """Deliberate handle resolution: memory://<room>/<id-or-source-path>.

    2026-07-09 decency architecture (Sol's ruling): ambient search stays
    room-scoped so each room keeps its personality; explicit handles cross
    rooms on purpose — a knock, not a key. Provenance is stamped so the
    caller always knows whose memory it holds. Bypasses the db-only
    visibility filter: following a receipt is deliberate, like a citation.
    """
    with conn.cursor(cursor_factory=psycopg2.extras.DictCursor) as cur:
        if memory_id is not None:
            cur.execute(
                """
                SELECT id, room, type, date, title, source_path, body, created_at
                FROM memories WHERE id = %s
                """,
                (memory_id,),
            )
        else:
            cur.execute(
                """
                SELECT id, room, type, date, title, source_path, body, created_at
                FROM memories WHERE source_path = %s
                ORDER BY id DESC LIMIT 1
                """,
                (source_path,),
            )
        row = cur.fetchone()
        if row is None:
            return {"found": False, "memory": None, "warnings": []}
        cur.execute(
            "SELECT thread_key FROM memory_threads WHERE memory_id = %s ORDER BY thread_key",
            (row["id"],),
        )
        threads = [r["thread_key"] for r in cur.fetchall()]
    warnings = []
    if claimed_room and row["room"] != claimed_room:
        warnings.append(
            f"handle claimed room '{claimed_room}' but memory {row['id']} lives in '{row['room']}'"
        )
    return {
        "found": True,
        "warnings": warnings,
        "memory": {
            "id": row["id"],
            "room": row["room"],
            "type": row["type"],
            "date": row["date"].isoformat() if row["date"] else None,
            "title": row["title"],
            "source_path": row["source_path"],
            "threads": threads,
            "body": row["body"],
            "created_at": row["created_at"].isoformat() if row["created_at"] else None,
        },
    }


def load_cluster_resonance(
    conn,
    query_vec,
    top_clusters=8,
    hot_per_cluster=2,
    exclude_paths=None,
    erasure_columns: dict[str, bool] | None = None,
    include_archived: bool = False,
):
    """Cluster-activation profile over the memory space (2026-07-09 roadmap).

    Cosine activation of the prompt embedding against stored cluster
    centroids (migration 0023), plus the nearest member chunks per top
    cluster that the semantic pass did NOT already surface — the "dormant
    hot" regions the reply is near but not using.

    Telemetry, not testimony: this reports what the memory space finds near
    the conversation — never model-internal state (unavailable on API models).
    """
    literal = "[" + ",".join(f"{x:.6f}" for x in query_vec) + "]"
    exclude = {p for p in (exclude_paths or set()) if p}
    erasure_columns = erasure_columns or {}
    memory_filter = erasure_filter(
        erasure_columns, include_archived=include_archived,
    )
    with conn.cursor(cursor_factory=psycopg2.extras.DictCursor) as cur:
        cur.execute(
            """
            SELECT id, label, member_count,
                   1 - (centroid <=> %s::halfvec) AS activation
            FROM memory_clusters
            WHERE centroid IS NOT NULL
            ORDER BY activation DESC
            """,
            (literal,),
        )
        rows = cur.fetchall()
        profile = [
            {
                "cluster_id": r["id"],
                "label": r["label"],
                "member_count": r["member_count"],
                "activation": round(float(r["activation"]), 4),
            }
            for r in rows[:top_clusters]
        ]
        hot = []
        for entry in profile[:3]:
            cur.execute(
                f"""
                SELECT m.source_path, c.heading_path,
                       1 - (c.body_embedding <=> %s::halfvec) AS sim
                FROM memory_cluster_members mm
                JOIN memory_chunks c ON c.id = mm.chunk_id
                JOIN memories m ON m.id = c.memory_id
                WHERE mm.cluster_id = %s
                  AND {RETRIEVAL_VISIBILITY_SQL}
                  AND {memory_filter}
                ORDER BY sim DESC
                LIMIT %s
                """,
                (literal, entry["cluster_id"], hot_per_cluster + 4),
            )
            picked = []
            for r in cur.fetchall():
                if r["source_path"] in exclude:
                    continue
                picked.append(
                    {
                        "source_path": r["source_path"],
                        "heading_path": r["heading_path"],
                        "sim": round(float(r["sim"]), 4),
                    }
                )
                if len(picked) >= hot_per_cluster:
                    break
            if picked:
                hot.append({"cluster_id": entry["cluster_id"], "label": entry["label"], "chunks": picked})
    return {"profile": profile, "hot": hot}


def load_taxonomy(
    conn,
    rooms=("kintsu", "house"),
    room="kintsu",
    limit=16,
    erasure_columns: dict[str, bool] | None = None,
    include_archived: bool = False,
) -> dict:
    """Return a small self-describing menu of what retrieval can target."""
    erasure_columns = erasure_columns or {}
    memory_filter = erasure_filter(
        erasure_columns, include_archived=include_archived,
    )
    with conn.cursor(cursor_factory=psycopg2.extras.DictCursor) as cur:
        cur.execute(
            f"""
            SELECT m.room, COALESCE(m.type, '') AS type, COUNT(*) AS count
            FROM memories m
            WHERE m.room = ANY(%s)
              AND (m.source_path NOT LIKE 'db-only/%%' OR m.type = 'paper-boat')
              AND {memory_filter}
            GROUP BY m.room, COALESCE(m.type, '')
            ORDER BY count DESC, m.room, type
            """,
            (list(rooms),),
        )
        memory_types = [
            {"room": row["room"], "type": row["type"] or "unknown", "count": int(row["count"])}
            for row in cur.fetchall()
        ]

        cur.execute(
            f"""
            SELECT mt.thread_key, COUNT(*) AS count
            FROM memory_threads mt
            JOIN memories m ON m.id = mt.memory_id
            WHERE m.room = ANY(%s)
              AND (m.source_path NOT LIKE 'db-only/%%' OR m.type = 'paper-boat')
              AND {memory_filter}
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
    erasure_columns: dict[str, bool] | None = None,
    include_archived: bool = False,
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
    erasure_columns = erasure_columns or {}
    memory_filter = erasure_filter(
        erasure_columns, include_archived=include_archived,
    )

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
    filters.append(memory_filter)
    params.extend([vec_str, top_k])
    where = " AND ".join(filters)

    sql = f"""
        SELECT m.id AS memory_id,
               m.source_path,
               m.room,
               mc.chunk_index,
               mc.heading_path,
               mc.body,
               mc.char_start,
               mc.char_end,
               {erasure_select(erasure_columns)},
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

            archived, superseded, archived_at = lifecycle_flags(row)
            lifecycle_penalty = (
                (ARCHIVE_SCORE_DEMOTION if archived else 0.0)
                + (ERASURE_SCORE_DEMOTION if superseded else 0.0)
            )
            source_path = (
                f"house/{row['source_path']}" if row["room"] == "house" else row["source_path"]
            )
            out.append({
                "memory_id": int(row["memory_id"]),
                "source_path": source_path,
                "room": row["room"],
                "chunk_index": int(row["chunk_index"]) if row["chunk_index"] is not None else 0,
                "heading_path": row["heading_path"] or "",
                "body": row["body"] or "",
                "char_start": int(row["char_start"]) if row["char_start"] is not None else 0,
                "char_end": int(row["char_end"]) if row["char_end"] is not None else 0,
                "sim": round(sim, 4),
                "score": round((sim * 6.0) - lifecycle_penalty, 4),
                "archived": archived,
                "archived_at": archived_at,
                "superseded": superseded,
                "reasons": [flag for flag, active in (
                    ("archived", archived), ("superseded", superseded),
                ) if active],
            })

        return out


def load_content_chunks(
    conn,
    rooms: tuple[str, ...],
    query: str,
    top_k: int,
    min_sim: float,
    scope_files: list[str] | None = None,
    erasure_columns: dict[str, bool] | None = None,
    include_archived: bool = False,
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
    erasure_columns = erasure_columns or {}
    memory_filter = erasure_filter(
        erasure_columns, include_archived=include_archived,
    )
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
    filters.append(memory_filter)

    params.append(top_k)
    where = " AND ".join(filters)

    sql = f"""
        SELECT m.id AS memory_id,
               m.source_path,
               m.room,
               mc.chunk_index,
               mc.heading_path,
               mc.body,
               mc.char_start,
               mc.char_end,
               {erasure_select(erasure_columns)},
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
            archived, superseded, archived_at = lifecycle_flags(row)
            lifecycle_penalty = (
                (ARCHIVE_SCORE_DEMOTION if archived else 0.0)
                + (ERASURE_SCORE_DEMOTION if superseded else 0.0)
            )
            source_path = (
                f"house/{row['source_path']}" if row["room"] == "house" else row["source_path"]
            )
            out.append({
                "memory_id": int(row["memory_id"]),
                "source_path": source_path,
                "room": row["room"],
                "chunk_index": int(row["chunk_index"]) if row["chunk_index"] is not None else 0,
                "heading_path": row["heading_path"] or "",
                "body": row["body"] or "",
                "char_start": int(row["char_start"]) if row["char_start"] is not None else 0,
                "char_end": int(row["char_end"]) if row["char_end"] is not None else 0,
                "ws": round(ws, 4),
                "score": round((ws * 6.0) - lifecycle_penalty, 4),
                "archived": archived,
                "archived_at": archived_at,
                "superseded": superseded,
                "reasons": [flag for flag, active in (
                    ("archived", archived), ("superseded", superseded),
                ) if active],
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
    erasure_columns: dict[str, bool] | None = None,
    include_archived: bool = False,
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
    erasure_columns = erasure_columns or {}
    memory_filter = erasure_filter(
        erasure_columns, include_archived=include_archived,
    )
    if not query_dates:
        return []

    sql = f"""
        SELECT m.id, m.room, m.source_path, m.title, m.type,
               m.date, m.dates, m.threads,
               LEFT(m.body, %s) AS body_excerpt,
               OCTET_LENGTH(m.body) AS body_full_chars,
               {erasure_select(erasure_columns)}
        FROM memories m
        WHERE m.room = ANY(%s)
          AND {RETRIEVAL_VISIBILITY_SQL}
          AND {memory_filter}
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
            archived, superseded, archived_at = lifecycle_flags(row)
            lifecycle_penalty = (
                (ARCHIVE_SCORE_DEMOTION if archived else 0.0)
                + (ERASURE_SCORE_DEMOTION if superseded else 0.0)
            )
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
                "score": round(6.0 - lifecycle_penalty, 4),
                "archived": archived,
                "archived_at": archived_at,
                "superseded": superseded,
                "reasons": [flag for flag, active in (
                    ("archived", archived), ("superseded", superseded),
                ) if active],
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


ANAMNESIS_MAX_LIMIT = 50
ANAMNESIS_DEFAULT_LIMIT = 10


def fetch_anamnesis(conn, room_name: str, view: str, query: str, limit: int) -> dict:
    """Read deterministic Cabinet counsel, scoped to the room and shared house."""
    warnings: list[str] = []
    safe_limit = max(1, min(int(limit or ANAMNESIS_DEFAULT_LIMIT), ANAMNESIS_MAX_LIMIT))
    if view == "consult" and not (query or "").strip():
        return {"ok": False, "mode": view, "entries": [], "warnings": ["consult requires a non-empty query"]}

    scope = [room_name, "house"]
    if view == "wake":
        where = (
            "a.room = ANY(%s) AND ((a.kind = 'pillar' AND a.activation = 'wake') OR "
            "(a.kind = 'cycle' AND a.active = TRUE AND a.activation = 'wake'))"
        )
        order = "CASE WHEN a.kind = 'pillar' THEN 0 ELSE 1 END, a.updated_at DESC, a.id DESC"
        params = [scope]
    else:
        search_text = query.strip()
        terms = extract_search_terms(search_text, max_terms=8)
        patterns = [f"%{term}%" for term in (terms or [search_text.lower()])]
        where = """a.room = ANY(%s) AND (
            a.body_tsv @@ websearch_to_tsquery('portuguese', %s)
            OR a.title ILIKE ANY(%s)
            OR coalesce(a.shape,'') ILIKE ANY(%s)
            OR array_to_string(a.tags, ' ') ILIKE ANY(%s)
            OR array_to_string(a.canon_links, ' ') ILIKE ANY(%s)
            OR a.ramp ILIKE ANY(%s)
            OR coalesce(a.counsel,'') ILIKE ANY(%s)
            OR coalesce(a.peak,'') ILIKE ANY(%s)
        )"""
        params = [
            scope, search_text,
            patterns, patterns, patterns, patterns, patterns, patterns, patterns,
        ]
        order = """CASE WHEN lower(a.title) = lower(%s) THEN 0
            WHEN a.title ILIKE ANY(%s) THEN 1
            WHEN coalesce(a.shape,'') ILIKE ANY(%s) THEN 2 ELSE 3 END,
            ts_rank_cd(a.body_tsv, websearch_to_tsquery('portuguese', %s)) DESC,
            similarity(a.title, %s) DESC,
            a.updated_at DESC, a.id DESC"""
        params.extend([search_text, patterns, patterns, search_text, search_text])

    with conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as cur:
        cur.execute(
            f"""SELECT a.id, a.room, a.kind, a.fidelity, a.activation, a.active,
                       a.title, a.shape, a.peak, a.beginning, a.ramp, a.counsel,
                       a.verify_note, a.source_paths, a.canon_links, a.tags,
                       a.created_at, a.updated_at
                FROM anamnesis a WHERE {where}
                ORDER BY {order} LIMIT %s""",
            params + [safe_limit],
        )
        rows = cur.fetchall()
        entries = []
        for row in rows:
            if view == "wake" and row["kind"] == "cycle" and not (row["verify_note"] or "").strip():
                warnings.append(f"excluded cycle {row['id']}: blank verify_note")
                continue
            cur.execute(
                """SELECT id, rep_number, occurred_on, how_it_went, portal_pull,
                          lighter, source_path, created_at
                   FROM anamnesis_reps WHERE cabinet_id = %s
                   ORDER BY occurred_on DESC NULLS LAST, rep_number DESC, id DESC
                   LIMIT 3""",
                (row["id"],),
            )
            reps = list(reversed(cur.fetchall()))
            entries.append({**dict(row), "reps": reps})
    return {"ok": True, "mode": view, "entries": entries, "warnings": warnings}


def read_anamnesis(conn, room_name: str, view: str, query: str, limit: int) -> dict:
    try:
        return fetch_anamnesis(conn, room_name, view, query, limit)
    except Exception as exc:
        return {"ok": False, "mode": view, "entries": [], "warnings": [str(exc)]}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--room-dir", required=True)
    parser.add_argument(
        "--room",
        default=None,
        help="Room name (kodo|kintsu|test). If omitted, derived from --room-dir basename.",
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
        choices=("full", "lexical", "semantic", "content", "date", "taxonomy", "candidates", "fetch", "anamnesis"),
        default="full",
        help=(
            "Retrieval mode (audit ticket #1 + 2026-05-19 GIN pass + 2026-05-23 date pass). "
            "'full': lexical + important + taxonomy + candidates + global semantic + content + date. "
            "'lexical': index + importantIndex only, no embed call, no content. "
            "'taxonomy': cheap corpus menu only, no embed call. "
            "'candidates': term-aware indexed search over entities/threads/memories/lessons, no embed call. "
            "'semantic': embed prompt + narrowed semantic against --scope-files. "
            "'content': pg_trgm word_similarity on memory_chunks.body, no embed call. "
            "'date': YYYY-MM-DD-token lookup against memories.dates GIN. No embed. "
            "Plugin's tiered flow: lexical first → rank threads → semantic scoped + "
            "content global + date global → merge."
        ),
    )
    parser.add_argument(
        "--include-archived",
        action="store_true",
        help="Include archived memories for deliberate history retrieval (default excludes them).",
    )
    parser.add_argument("--anamnesis-view", choices=("wake", "consult"), default="wake")
    parser.add_argument("--anamnesis-query", default="")
    parser.add_argument("--anamnesis-limit", type=int, default=ANAMNESIS_DEFAULT_LIMIT)
    parser.add_argument(
        "--memory-id",
        type=int,
        default=None,
        help="fetch mode: resolve a memory by primary key (house-wide, deliberate handle).",
    )
    parser.add_argument(
        "--memory-path",
        default=None,
        help="fetch mode: resolve the newest memory with this source_path.",
    )
    parser.add_argument(
        "--skip-semantic",
        action="store_true",
        help="In full mode, skip the semantic branch before any embedding/Ollama wake.",
    )
    parser.add_argument(
        "--skip-content",
        action="store_true",
        help="In full mode, skip the content/trigram branch.",
    )
    parser.add_argument(
        "--skip-date",
        action="store_true",
        help="In full mode, skip the date lookup branch.",
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
    parser.add_argument(
        "--candidate-top-k",
        type=int,
        default=DEFAULT_CANDIDATE_TOP_K,
        help=f"Max term-aware candidates returned by candidate pass (default {DEFAULT_CANDIDATE_TOP_K}).",
    )
    parser.add_argument(
        "--candidate-excerpt-chars",
        type=int,
        default=DEFAULT_CANDIDATE_EXCERPT_CHARS,
        help=(
            f"Truncate candidate memory excerpts to this many chars "
            f"(default {DEFAULT_CANDIDATE_EXCERPT_CHARS})."
        ),
    )

    args = parser.parse_args()

    room_dir = Path(args.room_dir).resolve()
    # Derive room name from cwd basename if not explicitly passed. Fixes the
    # 2026-05-04 bug where opencode-Kodo's session loaded Kintsu memory because
    # load_index/load_important_index defaulted to "kintsu" regardless of cwd.
    room_name = (args.room or room_dir.name or "kodo").lower()
    if room_name not in ("kodo", "kintsu", "tuner", "test"):
        room_name = "kodo"  # safe fallback; both rooms have substrate rows

    prompt = read_prompt_from_stdin()
    scope_files: list[str] | None = None
    if args.scope_files:
        scope_files = [s.strip() for s in args.scope_files.split(",") if s.strip()]

    env = substrate_env(room_dir)
    conn = connect(env)
    erasure_columns = detect_erasure_columns(conn)
    include_archived = bool(args.include_archived)
    try:
        payload: dict = {}
        if args.mode == "anamnesis":
            payload = read_anamnesis(
                conn, room_name, args.anamnesis_view, args.anamnesis_query, args.anamnesis_limit,
            )
            print(json.dumps(payload, ensure_ascii=False, default=str))
            return 0

        # Deliberate handle fetch (memory://<room>/<id-or-path>). House-wide
        # by design — see fetch_memory docstring; short-circuits every
        # search pass and returns immediately.
        if args.mode == "fetch":
            if args.memory_id is None and not args.memory_path:
                payload["memoryHandle"] = {
                    "found": False, "memory": None,
                    "warnings": ["fetch mode requires --memory-id or --memory-path"],
                }
            else:
                payload["memoryHandle"] = fetch_memory(
                    conn,
                    memory_id=args.memory_id,
                    source_path=args.memory_path,
                    claimed_room=(args.room or "").strip().lower() or None,
                )
            print(json.dumps(payload, ensure_ascii=False, default=str))
            return 0

        # Lexical pass (Pass 1 of tiered retrieval). Loads the full thread
        # index + named-entity index for plugin-side ranking. mode=semantic
        # skips these to avoid wasted work in the two-stage flow.
        if args.mode in ("full", "lexical"):
            payload["index"] = load_index(
                conn,
                rooms=(room_name, "house"),
                erasure_columns=erasure_columns,
                include_archived=include_archived,
            )
            payload["importantIndex"] = load_important_index(conn, room=room_name)

        if args.mode in ("full", "taxonomy"):
            payload["taxonomy"] = load_taxonomy(
                conn,
                rooms=(room_name, "house"),
                room=room_name,
                erasure_columns=erasure_columns,
                include_archived=include_archived,
            )
            payload["clusterStaleness"] = load_cluster_staleness(
                conn,
                erasure_columns=erasure_columns,
                include_archived=include_archived,
            )

        # Term-aware candidate pass (2026-07-01 retrieval roadmap). This is
        # the lightweight publishable search lane: no embedding call, every
        # meaningful query term contributes to score, and each candidate reports
        # matched/missing terms plus reasons. Recall uses this to diagnose why a
        # result surfaced instead of trusting semantic/content matches alone.
        if args.mode in ("full", "candidates"):
            search_terms: list[str] = []
            search_candidates: list[dict] = []
            if prompt:
                try:
                    search_terms, search_candidates = load_search_candidates(
                        conn,
                        rooms=(room_name, "house"),
                        query=prompt,
                        top_k=args.candidate_top_k,
                        excerpt_chars=args.candidate_excerpt_chars,
                        erasure_columns=erasure_columns,
                        include_archived=include_archived,
                    )
                except Exception as err:
                    print(f"candidates: query failed: {err}", file=sys.stderr)
            payload["searchTerms"] = search_terms
            payload["searchCandidates"] = search_candidates

        if args.mode in ("full", "semantic"):
            payload["semanticChunks"] = []
        if args.mode in ("full", "content"):
            payload["contentChunks"] = []
        if args.mode in ("full", "date"):
            payload["dateMatches"] = []
            payload["queryDates"] = []

        # Semantic pass (Pass 2 of tiered retrieval). Embeds prompt + queries
        # memory_chunks for top-K cosine neighbors, optionally narrowed to
        # active-thread files via --scope-files. mode=lexical skips this
        # entirely (no embed call, no postgres query). Fail-open: any
        # failure produces an empty semanticChunks list and a stderr line.
        if args.mode in ("full", "semantic") and not args.skip_semantic:
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
                            erasure_columns=erasure_columns,
                            include_archived=include_archived,
                        )
                    except Exception as err:
                        print(f"semantic: query failed: {err}", file=sys.stderr)

                    # Resonance readout (2026-07-09): rides the same prompt
                    # embedding; one pgvector query against 40 centroids.
                    # Fail-open like every pass in this file.
                    try:
                        payload["clusterResonance"] = load_cluster_resonance(
                            conn,
                            query_vec=vec,
                            exclude_paths={c.get("source_path") for c in chunks},
                            erasure_columns=erasure_columns,
                            include_archived=include_archived,
                        )
                    except Exception as err:
                        print(f"resonance: query failed: {err}", file=sys.stderr)

            payload["semanticChunks"] = chunks

        # Content pass (added 2026-05-19, pg_trgm GIN on memory_chunks.body).
        # No embed call, no scope-narrowing by default — catches proper-noun /
        # exact-string matches the semantic cosine misses. Cheap due to the
        # trigram index. mode=lexical skips this; mode=content runs ONLY this.
        if args.mode in ("full", "content") and not args.skip_content:
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
                        erasure_columns=erasure_columns,
                        include_archived=include_archived,
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
        if args.mode in ("full", "date") and not args.skip_date:
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
                        erasure_columns=erasure_columns,
                        include_archived=include_archived,
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
