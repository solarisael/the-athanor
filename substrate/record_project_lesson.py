#!/usr/bin/env python3
"""Direct write helper for project lessons."""
from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

import psycopg2
import psycopg2.extras

from backup_runner import run_backup

import state_paths


def load_dotenv(path: Path) -> None:
    if not path.exists():
        return
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, _, value = line.partition("=")
        os.environ.setdefault(key.strip(), value.strip())


def env(name: str, default: str | None = None) -> str:
    value = os.environ.get(name, default)
    if value is None:
        sys.exit(f"missing env var: {name}")
    return value


def parse_meta(raw: str) -> dict:
    if raw.strip() == "":
        return {}
    decoded = json.loads(raw)
    if not isinstance(decoded, dict):
        raise ValueError("--meta must decode to a JSON object")
    return decoded


def main() -> int:
    parser = argparse.ArgumentParser(description="Record a project-specific lesson.")
    parser.add_argument("--project", required=True)
    parser.add_argument("--title", required=True)
    parser.add_argument("--lesson")
    parser.add_argument("--lesson-stdin", action="store_true",
                        help="read lesson text from stdin")
    parser.add_argument("--trigger-context")
    parser.add_argument("--proof-pattern")
    parser.add_argument("--tag", action="append", default=[])
    parser.add_argument("--language-key", action="append", default=[])
    parser.add_argument("--technology-key", action="append", default=[])
    parser.add_argument("--thread-key", action="append", default=[])
    parser.add_argument("--source-memory-path")
    parser.add_argument("--source-lines-start", type=int)
    parser.add_argument("--source-lines-end", type=int)
    parser.add_argument("--meta", default="{}")
    parser.add_argument("--env-file", default=str(state_paths.default_dotenv_path()))
    parser.add_argument("--no-backup", action="store_true")
    args = parser.parse_args()
    if args.lesson is not None and args.lesson_stdin:
        parser.error("--lesson and --lesson-stdin are mutually exclusive")
    if args.lesson_stdin:
        args.lesson = sys.stdin.read()
    elif args.lesson is None:
        parser.error("provide --lesson or --lesson-stdin")

    load_dotenv(Path(args.env_file))
    meta = parse_meta(args.meta)

    conn = psycopg2.connect(
        host=env("PGHOST", "127.0.0.1"),
        port=int(env("PGPORT", "5432")),
        dbname=env("PGDATABASE"),
        user=env("PGUSER"),
        password=env("PGPASSWORD"),
    )
    try:
        with conn, conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as cur:
            cur.execute(
                """
                INSERT INTO lessons
                  (lesson_key, project, title, lesson, trigger_context, proof_pattern,
                   language_keys, technology_keys, thread_keys, tags, source_memory_path,
                   source_lines_start, source_lines_end, meta)
                VALUES
                  ('project', %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s::jsonb)
                ON CONFLICT (project, title) WHERE lesson_key = 'project' DO UPDATE SET
                  lesson = EXCLUDED.lesson,
                  trigger_context = EXCLUDED.trigger_context,
                  proof_pattern = EXCLUDED.proof_pattern,
                  language_keys = EXCLUDED.language_keys,
                  technology_keys = EXCLUDED.technology_keys,
                  thread_keys = EXCLUDED.thread_keys,
                  tags = EXCLUDED.tags,
                  source_memory_path = EXCLUDED.source_memory_path,
                  source_lines_start = EXCLUDED.source_lines_start,
                  source_lines_end = EXCLUDED.source_lines_end,
                  meta = EXCLUDED.meta
                RETURNING id, project, title
                """,
                (
                    args.project,
                    args.title,
                    args.lesson,
                    args.trigger_context,
                    args.proof_pattern,
                    args.language_key,
                    args.technology_key,
                    args.thread_key,
                    args.tag,
                    args.source_memory_path,
                    args.source_lines_start,
                    args.source_lines_end,
                    json.dumps(meta, ensure_ascii=False),
                ),
            )
            row = cur.fetchone()
            print(
                f"upserted project_lesson id={row['id']} project={row['project']} title={row['title']}"
            )
    finally:
        conn.close()
    run_backup(__file__, skip=args.no_backup)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
