#!/usr/bin/env python3
"""Direct write helper for design-system lessons.

Records taste rules for tokens, component contracts, safety refusals, and
accessibility floors in the unified ``lessons`` table.

Usage example:
    python3 record_design_lesson.py \
        --voice house-design \
        --register web \
        --shape component-contract \
        --title "Honor the focus floor" \
        --lesson "Every interactive control has a visible keyboard focus state." \
        --proof-pattern "Tab through every enabled control." \
        --example-text "Use the focus ring token; do not remove outlines." \
        --tag accessibility \
        --source-memory-path memory/source.md
"""
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
    parser = argparse.ArgumentParser(description="Record a design-system lesson.")
    parser.add_argument("--voice", default="general",
                        help="free-form provenance or design register")
    parser.add_argument("--register", action="append", default=[],
                        help="design context the rule applies to; repeatable. "
                             "Defaults to ['general'] if omitted.")
    parser.add_argument("--shape",
                        help="vocabulary axis (token | component-contract | "
                             "safety-refusal | accessibility-floor | ...)")
    parser.add_argument("--title", required=True)
    parser.add_argument("--lesson")
    parser.add_argument("--lesson-stdin", action="store_true",
                        help="read lesson text from stdin")
    parser.add_argument("--trigger-context")
    parser.add_argument("--proof-pattern")
    parser.add_argument("--example-text")
    parser.add_argument("--tag", action="append", default=[])
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
    register_values = args.register or ["general"]

    conn = psycopg2.connect(
        host=env("PGHOST", "127.0.0.1"),
        port=int(env("PGPORT", "5432")),
        dbname=env("PGDATABASE"),
        user=env("PGUSER"),
        password=env("PGPASSWORD"),
    )
    conn.autocommit = False
    try:
        with conn, conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as cur:
            cur.execute(
                """
                INSERT INTO lessons
                  (lesson_key, voice, register, shape, title, lesson, trigger_context,
                   proof_pattern, example_text, thread_keys, tags, source_memory_path,
                   source_lines_start, source_lines_end, meta)
                VALUES
                  ('design', %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s::jsonb)
                ON CONFLICT (voice, title) WHERE lesson_key = 'design' DO UPDATE SET
                  register = EXCLUDED.register,
                  shape = EXCLUDED.shape,
                  lesson = EXCLUDED.lesson,
                  trigger_context = EXCLUDED.trigger_context,
                  proof_pattern = EXCLUDED.proof_pattern,
                  example_text = EXCLUDED.example_text,
                  thread_keys = EXCLUDED.thread_keys,
                  tags = EXCLUDED.tags,
                  source_memory_path = EXCLUDED.source_memory_path,
                  source_lines_start = EXCLUDED.source_lines_start,
                  source_lines_end = EXCLUDED.source_lines_end,
                  meta = EXCLUDED.meta
                RETURNING id, voice, register, shape, title
                """,
                (
                    args.voice,
                    register_values,
                    args.shape,
                    args.title,
                    args.lesson,
                    args.trigger_context,
                    args.proof_pattern,
                    args.example_text,
                    args.thread_key,
                    args.tag,
                    args.source_memory_path,
                    args.source_lines_start,
                    args.source_lines_end,
                    json.dumps(meta, ensure_ascii=False),
                ),
            )
            row = cur.fetchone()
            registers_str = ",".join(row["register"]) if row["register"] else ""
            print(
                f"upserted design_lesson id={row['id']} voice={row['voice']} "
                f"register=[{registers_str}] shape={row['shape']} "
                f"title={row['title']!r}"
            )
    finally:
        conn.close()

    run_backup(__file__, skip=args.no_backup)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
