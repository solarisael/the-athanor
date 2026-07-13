#!/usr/bin/env python3
"""Delete exactly one coding or project lesson after title confirmation.

The operation is deliberately narrow: kind selects one of two allowlisted
lesson tables, the numeric id identifies one row, and expected-title must
match the row currently in the database before deletion.  Errors are emitted
as structured JSON and the CLI stays fail-open for OMP callers.
"""
from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

try:
    import psycopg2
    import psycopg2.extras
except Exception as exc:  # pragma: no cover - exercised by CLI environments
    print(json.dumps({"ok": False, "error": f"psycopg2 import failed: {exc}"}))
    raise SystemExit(0)


TABLES = {"coding-lesson": "coding_lessons", "project-lesson": "project_lessons"}


def read_env_file(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    if not path.exists():
        return values
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line and not line.startswith("#") and "=" in line:
            key, _, value = line.partition("=")
            values[key.strip()] = value.strip()
    return values


def substrate_env(room_dir: Path) -> dict[str, str]:
    shared_root = room_dir.parent
    values: dict[str, str] = {}
    for env_path in (shared_root / "house" / "substrate" / ".env", shared_root / "kodo" / "substrate" / ".env"):
        values = read_env_file(env_path)
        if values:
            break
    for key in ("PGHOST", "PGPORT", "PGUSER", "PGPASSWORD", "PGDATABASE"):
        if os.environ.get(key):
            values[key] = os.environ[key]
    return values


def delete_lesson(conn, kind: str, lesson_id: int, expected_title: str) -> dict:
    """Delete one exact row, or return a refusal without changing the DB."""
    table = TABLES.get(kind)
    if table is None:
        return {"ok": False, "kind": kind, "id": lesson_id, "deleted": False, "error": "kind must be coding-lesson or project-lesson"}
    if isinstance(lesson_id, bool) or not isinstance(lesson_id, int) or lesson_id <= 0:
        return {"ok": False, "kind": kind, "id": lesson_id, "deleted": False, "error": "id must be a positive integer"}
    if not isinstance(expected_title, str) or not expected_title:
        return {"ok": False, "kind": kind, "id": lesson_id, "deleted": False, "error": "expected_title is required"}

    # The connection context commits on normal exit and rolls back on errors.
    with conn:
        with conn.cursor(cursor_factory=psycopg2.extras.DictCursor) as cur:
            cur.execute(f"SELECT title FROM {table} WHERE id = %s FOR UPDATE", (lesson_id,))
            row = cur.fetchone()
            if row is None:
                return {"ok": False, "kind": kind, "id": lesson_id, "deleted": False, "error": "lesson not found"}
            actual_title = str(row["title"] if isinstance(row, dict) else row[0])
            if actual_title != expected_title:
                return {"ok": False, "kind": kind, "id": lesson_id, "deleted": False, "error": "title mismatch", "actual_title": actual_title}
            cur.execute(f"DELETE FROM {table} WHERE id = %s AND title = %s", (lesson_id, expected_title))
            if cur.rowcount != 1:
                return {"ok": False, "kind": kind, "id": lesson_id, "deleted": False, "error": "delete affected an unexpected number of rows"}
    return {"ok": True, "kind": kind, "id": lesson_id, "title": expected_title, "deleted": True}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--room-dir", required=True)
    parser.add_argument("--kind", required=True, help="coding-lesson or project-lesson")
    parser.add_argument("--id", required=True, type=int)
    parser.add_argument("--expected-title", required=True)
    args = parser.parse_args()
    try:
        env = substrate_env(Path(args.room_dir).resolve())
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
