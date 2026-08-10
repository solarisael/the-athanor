#!/usr/bin/env python3
"""Query audio lessons before recording, treating, or debugging audio.

Parallels the coding and writing lesson queries. Ranking:
  1. spine rows (shape='spine') -- always-on, first
  2. stage-match  (--stage overlaps the row's stage[])
  3. shape-match  (--shape equals the row's shape)
  4. tsvector body match on the free-text query

Flags:
  --spine            print only the spine rows and exit
  --all              print every lesson (ignores ranking cutoff)
  --stage S          pipeline stage filter; repeatable
                     (capture | denoise | eq | loudness | diagnosis | general)
  --shape S          vocabulary axis filter
  --limit N          max rows (default 12)
  query              free-text intent (optional positional)

When a row has a negation_of partner, both halves print together so the
*why-not* stays attached to the *do*.
"""
from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

import psycopg2
import psycopg2.extras

import state_paths

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")

KNOWN_STAGES = ("capture", "denoise", "eq", "loudness", "diagnosis", "general")


def load_dotenv(path: Path) -> None:
    if not path.exists():
        return
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.strip() or line.strip().startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        os.environ.setdefault(key.strip(), value.strip())


def env(name: str, default: str | None = None) -> str:
    value = os.environ.get(name, default)
    if value is None:
        sys.exit(f"missing env var: {name}")
    return value


def connect():
    return psycopg2.connect(
        host=env("PGHOST", "127.0.0.1"),
        port=int(env("PGPORT", "5432")),
        dbname=env("PGDATABASE"),
        user=env("PGUSER"),
        password=env("PGPASSWORD"),
    )


def fetch_rows(cur, *, query, stages, shape, limit, show_all):
    cur.execute(
        """
        SELECT id, shape, stage, title, lesson, trigger_context,
               example_cmd, tools, negation_of, tags, source_memory_path,
               (shape = 'spine')                          AS is_spine,
               (%(stages)s::text[] && stage)              AS stage_hit,
               (shape = %(shape)s)                        AS shape_hit,
               CASE WHEN %(q)s <> '' THEN
                 ts_rank(lesson_tsv, plainto_tsquery('english', %(q)s))
               ELSE 0 END                                 AS rank
        FROM lessons
        WHERE lesson_key = 'audio'
          AND (
               %(all)s
            OR shape = 'spine'
            OR (%(stages)s::text[] && stage)
            OR (shape = %(shape)s)
            OR (%(q)s <> '' AND lesson_tsv @@ plainto_tsquery('english', %(q)s))
          )
        ORDER BY is_spine DESC, stage_hit DESC, shape_hit DESC, rank DESC, id ASC
        LIMIT %(limit)s
        """,
        {
            "q": query or "",
            "stages": stages or ["__none__"],
            "shape": shape or "__none__",
            "all": show_all,
            "limit": limit,
        },
    )
    return cur.fetchall()


def fetch_by_ids(cur, ids):
    if not ids:
        return {}
    cur.execute(
        "SELECT id, shape, title, lesson FROM lessons "
        "WHERE lesson_key = 'audio' AND id = ANY(%s)",
        (ids,),
    )
    return {r["id"]: r for r in cur.fetchall()}


def render(rows, partners):
    out = []
    for r in rows:
        head = f"[{r['shape']}]"
        if r["stage"]:
            head += " stage=" + ",".join(r["stage"])
        out.append(f"{head}\n  {r['title']}\n    {r['lesson']}")
        if r.get("trigger_context"):
            out.append(f"    when: {r['trigger_context']}")
        if r.get("example_cmd"):
            out.append(f"    e.g. {r['example_cmd']}")
        if r.get("tools"):
            out.append(f"    tools: {', '.join(r['tools'])}")
        if r.get("negation_of") and r["negation_of"] in partners:
            p = partners[r["negation_of"]]
            out.append(f"    not: {p['title']} -- {p['lesson']}")
        out.append("")
    return "\n".join(out).rstrip()


def main() -> int:
    p = argparse.ArgumentParser(description="Query audio-work lessons.")
    p.add_argument("query", nargs="?", default="")
    p.add_argument("--stage", action="append", default=[])
    p.add_argument("--shape")
    p.add_argument("--spine", action="store_true")
    p.add_argument("--all", action="store_true")
    p.add_argument("--limit", type=int, default=12)
    p.add_argument("--env-file", default=str(state_paths.default_dotenv_path()))
    args = p.parse_args()

    load_dotenv(Path(args.env_file))
    conn = connect()
    try:
        with conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as cur:
            if args.spine:
                cur.execute(
                    "SELECT id, shape, stage, title, lesson, trigger_context, "
                    "example_cmd, tools, negation_of, tags, source_memory_path "
                    "FROM lessons WHERE lesson_key = 'audio' "
                    "AND shape = 'spine' ORDER BY id"
                )
                rows = cur.fetchall()
            else:
                rows = fetch_rows(
                    cur, query=args.query, stages=args.stage,
                    shape=args.shape, limit=args.limit, show_all=args.all,
                )
            partners = fetch_by_ids(cur, [r["negation_of"] for r in rows if r.get("negation_of")])
            print(render(rows, partners) if rows else "(no audio lessons matched)")
    finally:
        conn.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
