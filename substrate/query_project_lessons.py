#!/usr/bin/env python3
"""Query project-specific lessons and gotchas."""
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


def load_dotenv(path: Path) -> None:
    if not path.exists():
        return
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, _, value = line.partition("=")
        os.environ.setdefault(key.strip(), value.strip())


def connect():
    return psycopg2.connect(
        host=os.environ["PGHOST"],
        port=int(os.environ["PGPORT"]),
        dbname=os.environ["PGDATABASE"],
        user=os.environ["PGUSER"],
        password=os.environ["PGPASSWORD"],
    )


def fetch_rows(cur, project: str, query: str | None, limit: int):
    if not query:
        cur.execute(
            """
            SELECT id, project, title, lesson, trigger_context, proof_pattern, tags
            FROM lessons
            WHERE lesson_key = 'project'
              AND project = %s
            ORDER BY updated_at DESC, id DESC
            LIMIT %s
            """,
            (project, limit),
        )
        return cur.fetchall()

    cur.execute(
        """
        WITH scored AS (
          SELECT id, project, title, lesson, trigger_context, proof_pattern, tags,
                 ts_rank(lesson_tsv, plainto_tsquery('portuguese', %s)) AS body_rank,
                 similarity(title, %s) AS title_sim
          FROM lessons
          WHERE lesson_key = 'project'
            AND project = %s
        )
        SELECT id, project, title, lesson, trigger_context, proof_pattern, tags
        FROM scored
        WHERE body_rank > 0 OR title_sim > 0.2
        ORDER BY (body_rank * 3 + title_sim) DESC, id DESC
        LIMIT %s
        """,
        (query, query, project, limit),
    )
    return cur.fetchall()


def render(rows) -> str:
    out = []
    out.append("─" * 72)
    out.append("PROJECT LESSONS")
    out.append("─" * 72)
    for row in rows:
        tags = ", ".join(row["tags"] or [])
        out.append(f"  [{row['project']}] {row['title']}")
        if tags:
            out.append(f"    tags: {tags}")
        out.append(f"    {row['lesson']}")
        if row["trigger_context"]:
            out.append(f"    context: {row['trigger_context']}")
        if row["proof_pattern"]:
            out.append(f"    proof: {row['proof_pattern']}")
        out.append("")
    out.append("─" * 72)
    return "\n".join(out)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project", required=True)
    parser.add_argument("query", nargs="?")
    parser.add_argument("--limit", type=int, default=8)
    parser.add_argument("--env-file", default=str(state_paths.default_dotenv_path()))
    args = parser.parse_args()

    load_dotenv(Path(args.env_file))
    conn = connect()
    try:
        with conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as cur:
            rows = fetch_rows(cur, args.project, args.query, args.limit)
            if not rows:
                print("no matching project lessons")
                return 1
            print(render(rows))
    finally:
        conn.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
