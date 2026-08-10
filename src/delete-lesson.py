#!/usr/bin/env python3
"""Delete exactly one coding, project, writing, or design lesson after title confirmation.

The operation is deliberately narrow: kind selects one of four allowlisted
lesson types in the unified table, the numeric id identifies one typed row,
and expected-title must match that row before deletion. Errors are emitted
as structured JSON and the CLI stays fail-open for OMP callers.
"""
from __future__ import annotations

import argparse
import json
import sys

from substrate_config import substrate_env

try:
    import psycopg2
    import psycopg2.extras
except ImportError:
    psycopg2 = None


LESSON_KEYS = {
    "coding-lesson": "coding",
    "project-lesson": "project",
    "writing-lesson": "writing",
    "design-lesson": "design",
}




def delete_lesson(conn, kind: str, lesson_id: int, expected_title: str) -> dict:
    """Delete one exact row, or return a refusal without changing the DB."""
    lesson_key = LESSON_KEYS.get(kind)
    if lesson_key is None:
        return {"ok": False, "kind": kind, "id": lesson_id, "deleted": False, "error": "kind must be coding-lesson, project-lesson, writing-lesson, or design-lesson"}
    if isinstance(lesson_id, bool) or not isinstance(lesson_id, int) or lesson_id <= 0:
        return {"ok": False, "kind": kind, "id": lesson_id, "deleted": False, "error": "id must be a positive integer"}
    if not isinstance(expected_title, str) or not expected_title:
        return {"ok": False, "kind": kind, "id": lesson_id, "deleted": False, "error": "expected_title is required"}

    # The connection context commits on normal exit and rolls back on errors.
    with conn:
        with conn.cursor(cursor_factory=psycopg2.extras.DictCursor) as cur:
            cur.execute(
                "SELECT title FROM lessons WHERE lesson_key = %s AND id = %s FOR UPDATE",
                (lesson_key, lesson_id),
            )
            row = cur.fetchone()
            if row is None:
                return {"ok": False, "kind": kind, "id": lesson_id, "deleted": False, "error": "lesson not found"}
            actual_title = str(row["title"] if isinstance(row, dict) else row[0])
            if actual_title != expected_title:
                return {"ok": False, "kind": kind, "id": lesson_id, "deleted": False, "error": "title mismatch", "actual_title": actual_title}
            cur.execute(
                "DELETE FROM lessons WHERE lesson_key = %s AND id = %s AND title = %s",
                (lesson_key, lesson_id, expected_title),
            )
            if cur.rowcount != 1:
                return {"ok": False, "kind": kind, "id": lesson_id, "deleted": False, "error": "delete affected an unexpected number of rows"}
    return {"ok": True, "kind": kind, "id": lesson_id, "title": expected_title, "deleted": True}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--room-dir", required=True)
    parser.add_argument("--kind", required=True, help="coding-lesson, project-lesson, writing-lesson, or design-lesson")
    parser.add_argument("--id", required=True, type=int)
    parser.add_argument("--expected-title", required=True)
    args = parser.parse_args()
    try:
        env = substrate_env()
        if psycopg2 is None:
            raise RuntimeError("psycopg2 is required for lesson deletion")
        conn = psycopg2.connect(host=env.get("PGHOST"), port=env.get("PGPORT"), user=env.get("PGUSER"), password=env.get("PGPASSWORD"), dbname=env.get("PGDATABASE"), connect_timeout=2)
        try:
            result = delete_lesson(conn, args.kind, args.id, args.expected_title)
        finally:
            conn.close()
        print(json.dumps(result, ensure_ascii=False))
    except Exception as exc:
        print(json.dumps({"ok": False, "kind": args.kind, "id": args.id, "deleted": False, "error": str(exc)}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
