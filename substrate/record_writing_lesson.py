#!/usr/bin/env python3
"""Direct write helper for writing lessons.

Parallels the coding lesson helper with two writing-specific axes:

  --register R    text[] array of writing contexts the rule applies to.
                  Repeatable. e.g. --register nigredo --register video-rant.
                  Defaults to ['general'] if not specified.
  --example-text  short prose snippet demonstrating the rule (coding's
                  analogous field is `proof_pattern`, renamed here because
                  writing examples are usually paired-quote demonstrations,
                  not code-pattern proofs).
  --writer W      exemplifying writer name. Repeatable. e.g.
                  --writer 'Madeline Miller' --writer 'Susanna Clarke'.

`--voice` is a free-form provenance or editorial register. Use stable names
within one House; the substrate does not impose a personality vocabulary.

Negation pairing lookup:
  --negation-of-id N      direct ID reference
  --negation-of-title T   lookup by exact title (within same --voice if given,
                          falling back to global if not). Useful when seeding
                          rows that reference each other before all IDs exist.

Usage example:
    python3 record_writing_lesson.py \\
        --voice house-editor \\
        --register essay \\
        --shape voice-specificity \\
        --title "Voice-specificity over generic vernacular" \\
        --lesson "Generic vernacular drops the writer's actual voice." \\
        --example-text "A paired before/after excerpt." \\
        --tag editorial \\
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


def resolve_negation_id(cur, *, neg_id: int | None, neg_title: str | None,
                        voice: str | None) -> int | None:
    """Pick the negation_of foreign key from CLI flags.

    Direct --negation-of-id wins. Otherwise --negation-of-title looks up by
    exact title, preferring same-voice match (more specific) before falling
    back to any-voice match. Returns None if neither flag given. Errors if
    title lookup yields no match (silent miss would be worse than failing
    the seed).
    """
    if neg_id is not None:
        return int(neg_id)
    if not neg_title:
        return None

    if voice:
        cur.execute(
            "SELECT id FROM lessons "
            "WHERE lesson_key = 'writing' AND title = %s AND voice = %s LIMIT 1",
            (neg_title, voice),
        )
        row = cur.fetchone()
        if row:
            return int(row["id"])

    cur.execute(
        "SELECT id FROM lessons WHERE lesson_key = 'writing' AND title = %s LIMIT 1",
        (neg_title,),
    )
    row = cur.fetchone()
    if row:
        return int(row["id"])

    raise ValueError(
        f"--negation-of-title {neg_title!r}: no matching row found. "
        f"Seed the partner row first, or pass --negation-of-id directly."
    )


def main() -> int:
    parser = argparse.ArgumentParser(description="Record a writing-craft lesson.")
    parser.add_argument("--voice", default="general",
                        help="free-form provenance or editorial register")
    parser.add_argument("--register", action="append", default=[],
                        help="writing context the rule applies to; repeatable. "
                             "Defaults to ['general'] if omitted.")
    parser.add_argument("--shape",
                        help="vocabulary axis (spine | register-fit | density | "
                             "opening | closing | voice-specificity | "
                             "coordination | image-vs-name | process | ...)")
    parser.add_argument("--title", required=True)
    parser.add_argument("--lesson")
    parser.add_argument("--lesson-stdin", action="store_true",
                        help="read lesson text from stdin")
    parser.add_argument("--trigger-context")
    parser.add_argument("--example-text")
    parser.add_argument("--writer", action="append", default=[],
                        help="exemplifying writer name; repeatable")
    parser.add_argument("--negation-of-id", type=int)
    parser.add_argument("--negation-of-title")
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
            negation_of = resolve_negation_id(
                cur,
                neg_id=args.negation_of_id,
                neg_title=args.negation_of_title,
                voice=args.voice,
            )

            cur.execute(
                """
                INSERT INTO lessons
                  (lesson_key, voice, register, shape, title, lesson, trigger_context,
                   example_text, writers, negation_of, thread_keys, tags,
                   source_memory_path, source_lines_start, source_lines_end, meta)
                VALUES
                  ('writing', %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s::jsonb)
                ON CONFLICT (voice, title) WHERE lesson_key = 'writing' DO UPDATE SET
                  register = EXCLUDED.register,
                  shape = EXCLUDED.shape,
                  lesson = EXCLUDED.lesson,
                  trigger_context = EXCLUDED.trigger_context,
                  example_text = EXCLUDED.example_text,
                  writers = EXCLUDED.writers,
                  negation_of = EXCLUDED.negation_of,
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
                    args.example_text,
                    args.writer,
                    negation_of,
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
                f"upserted writing_lesson id={row['id']} voice={row['voice']} "
                f"register=[{registers_str}] shape={row['shape']} "
                f"title={row['title']!r}"
            )
    finally:
        conn.close()

    run_backup(__file__, skip=args.no_backup)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
