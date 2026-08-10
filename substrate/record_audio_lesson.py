#!/usr/bin/env python3
"""Direct write helper for audio lessons. Parallels the writing lesson helper.

Audio-domain axes:
  --stage S       pipeline stage the rule applies to; repeatable
                  (capture | denoise | eq | loudness | diagnosis | general).
                  Defaults to ['general'].
  --tool T        tool involved; repeatable (DeepFilterNet, ffmpeg, sox, ...).
  --example-cmd C the incantation that demonstrates the rule.

shape vocabulary: spine | diagnosis | denoise | eq | loudness | capture | process

Negation pairing (keep the why-not attached to the do):
  --negation-of-id N      direct id
  --negation-of-title T   lookup by exact title

Usage:
  python3 record_audio_lesson.py --shape denoise --stage denoise \\
    --title "Neural deep filtering over spectral subtraction" \\
    --lesson "..." --tool DeepFilterNet --example-cmd "deep-filter -o out in.wav" \\
    --negation-of-title "Spectral subtraction warbles when pushed" \\
    --tag session:2026-06-29 --source-memory-path memory/2026-06-29_audio_chain.md
"""
from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

import psycopg2
import psycopg2.extras

from backup_runner import run_backup

import state_paths

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")


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


def resolve_negation_id(cur, *, neg_id, neg_title):
    if neg_id is not None:
        return neg_id
    if neg_title:
        cur.execute(
            "SELECT id FROM lessons WHERE lesson_key = 'audio' AND title = %s",
            (neg_title,),
        )
        row = cur.fetchone()
        if row:
            return row["id"]
        print(f"warn: negation-of-title {neg_title!r} not found", file=sys.stderr)
    return None


def upsert(cur, a) -> dict:
    negation_of = resolve_negation_id(cur, neg_id=a.negation_of_id, neg_title=a.negation_of_title)
    cur.execute(
        """
        INSERT INTO lessons
          (lesson_key, shape, stage, title, lesson, trigger_context, example_cmd,
           tools, negation_of, thread_keys, tags, source_memory_path)
        VALUES ('audio', %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
        ON CONFLICT (title) WHERE lesson_key = 'audio' DO UPDATE SET
          shape = EXCLUDED.shape, stage = EXCLUDED.stage,
          lesson = EXCLUDED.lesson, trigger_context = EXCLUDED.trigger_context,
          example_cmd = EXCLUDED.example_cmd, tools = EXCLUDED.tools,
          negation_of = EXCLUDED.negation_of, thread_keys = EXCLUDED.thread_keys,
          tags = EXCLUDED.tags, source_memory_path = EXCLUDED.source_memory_path
        RETURNING id, shape, stage, title
        """,
        (a.shape, a.stage or ["general"], a.title, a.lesson, a.trigger_context,
         a.example_cmd, a.tool, negation_of, a.thread_key, a.tag, a.source_memory_path),
    )
    return cur.fetchone()


def main() -> int:
    p = argparse.ArgumentParser(description="Record an audio-work lesson.")
    p.add_argument("--shape")
    p.add_argument("--stage", action="append", default=[])
    p.add_argument("--title", required=True)
    p.add_argument("--lesson")
    p.add_argument("--lesson-stdin", action="store_true",
                   help="read lesson text from stdin")
    p.add_argument("--trigger-context")
    p.add_argument("--example-cmd")
    p.add_argument("--tool", action="append", default=[])
    p.add_argument("--negation-of-id", type=int)
    p.add_argument("--negation-of-title")
    p.add_argument("--tag", action="append", default=[])
    p.add_argument("--thread-key", action="append", default=[])
    p.add_argument("--source-memory-path")
    p.add_argument("--env-file", default=str(state_paths.default_dotenv_path()))
    p.add_argument("--no-backup", action="store_true")
    a = p.parse_args()
    if a.lesson is not None and a.lesson_stdin:
        p.error("--lesson and --lesson-stdin are mutually exclusive")
    if a.lesson_stdin:
        a.lesson = sys.stdin.read()
    elif a.lesson is None:
        p.error("provide --lesson or --lesson-stdin")

    load_dotenv(Path(a.env_file))
    conn = connect()
    conn.autocommit = False
    try:
        with conn, conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as cur:
            row = upsert(cur, a)
            print(f"upserted audio_lesson id={row['id']} shape={row['shape']} "
                  f"stage={row['stage']} title={row['title']!r}")
    finally:
        conn.close()
    run_backup(__file__, skip=a.no_backup)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
