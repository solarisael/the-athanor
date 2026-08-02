#!/usr/bin/env python3
"""Update exactly one coding or project lesson after title confirmation.

The operation is deliberately narrow: kind selects one of two allowlisted
lesson types in the unified table, the numeric id identifies one typed row,
and expected-title must match that row before an update is attempted. Errors are JSON
and the CLI remains fail-open for OMP callers.
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


LESSON_KEYS = {"coding-lesson": "coding", "project-lesson": "project"}
COMMON_FIELDS = ("title", "lesson", "shape", "trigger_context", "tags")
CODING_FIELDS = COMMON_FIELDS + ("voice", "scope", "project", "proof_pattern", "negation_of")
PROJECT_FIELDS = COMMON_FIELDS + ("project", "proof_pattern")
_MISSING = object()




def _refusal(kind, lesson_id, error, **extra) -> dict:
    result = {"ok": False, "kind": kind, "id": lesson_id, "updated": False, "error": error}
    result.update(extra)
    return result


def _allowed_fields(kind: str):
    return CODING_FIELDS if kind == "coding-lesson" else PROJECT_FIELDS


def update_lesson(conn, kind: str, lesson_id: int, expected_title: str, patch: dict) -> dict:
    """Update one exact row, preserving all fields omitted from *patch*."""
    lesson_key = LESSON_KEYS.get(kind)
    if lesson_key is None:
        return _refusal(kind, lesson_id, "kind must be coding-lesson or project-lesson")
    if isinstance(lesson_id, bool) or not isinstance(lesson_id, int) or lesson_id <= 0:
        return _refusal(kind, lesson_id, "id must be a positive integer")
    if not isinstance(expected_title, str) or not expected_title:
        return _refusal(kind, lesson_id, "expected_title is required")
    if not isinstance(patch, dict) or not patch:
        return _refusal(kind, lesson_id, "at least one field must be provided")
    allowed = set(_allowed_fields(kind))
    unknown = [field for field in patch if field not in allowed]
    if unknown:
        return _refusal(kind, lesson_id, f"field not allowed for {kind}: {unknown[0]}")
    if "title" in patch and (not isinstance(patch["title"], str) or not patch["title"]):
        return _refusal(kind, lesson_id, "title must be a non-empty string")
    if "negation_of" in patch and patch["negation_of"] is not None:
        value = patch["negation_of"]
        if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
            return _refusal(kind, lesson_id, "negation_of must be null or a positive integer")

    # The connection context commits on normal exit and rolls back on errors.
    with conn:
        cursor_factory = psycopg2.extras.DictCursor if psycopg2 is not None else None
        with conn.cursor(cursor_factory=cursor_factory) as cur:
            cur.execute(
                "SELECT title FROM lessons WHERE lesson_key = %s AND id = %s FOR UPDATE",
                (lesson_key, lesson_id),
            )
            row = cur.fetchone()
            if row is None:
                return _refusal(kind, lesson_id, "lesson not found")
            actual_title = str(row["title"] if isinstance(row, dict) else row[0])
            if actual_title != expected_title:
                return _refusal(kind, lesson_id, "title mismatch", actual_title=actual_title)
            columns = list(patch)
            assignments = ", ".join(f"{column} = %s" for column in columns)
            values = [patch[column] for column in columns] + [lesson_key, lesson_id]
            cur.execute(
                f"UPDATE lessons SET {assignments} WHERE lesson_key = %s AND id = %s",
                values,
            )
            if cur.rowcount != 1:
                return _refusal(kind, lesson_id, "update affected an unexpected number of rows")
    return {"ok": True, "kind": kind, "id": lesson_id, "title": patch.get("title", expected_title), "updated": True}


def _parse_patch(args) -> dict:
    patch = {}
    for field in COMMON_FIELDS + ("voice", "scope", "project", "proof_pattern", "negation_of"):
        value = getattr(args, field, _MISSING)
        if value is not _MISSING and not (field == "tags" and value is None):
            patch[field] = value
    if getattr(args, "clear_negation_of", False):
        patch["negation_of"] = None
    if getattr(args, "lesson_stdin", False):
        patch["lesson"] = sys.stdin.read()
    if "negation_of" in patch and isinstance(patch["negation_of"], str):
        if patch["negation_of"].lower() == "null":
            patch["negation_of"] = None
        else:
            try:
                patch["negation_of"] = int(patch["negation_of"])
            except ValueError:
                pass
    if isinstance(patch.get("tags"), str):
        patch["tags"] = json.loads(patch["tags"])
    return patch


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--room-dir", required=True)
    parser.add_argument("--kind", required=True, help="coding-lesson or project-lesson")
    parser.add_argument("--id", required=True)
    parser.add_argument("--expected-title", required=True)
    for field in COMMON_FIELDS + ("voice", "scope", "project", "proof_pattern", "negation_of"):
        if field == "tags":
            continue
        option = "--" + field.replace("_", "-")
        parser.add_argument(option, dest=field, default=_MISSING)
    parser.add_argument("--tag", dest="tags", action="append", default=None)
    parser.add_argument("--clear-negation-of", dest="clear_negation_of", action="store_true")
    parser.add_argument("--lesson-stdin", action="store_true")
    args = parser.parse_args()
    try:
        try:
            lesson_id = int(args.id)
        except (TypeError, ValueError):
            lesson_id = args.id
        patch = _parse_patch(args)
        env = substrate_env(args.room_dir)
        if psycopg2 is None:
            raise RuntimeError("psycopg2 is required for lesson updates")
        conn = psycopg2.connect(host=env.get("PGHOST"), port=env.get("PGPORT"), user=env.get("PGUSER"), password=env.get("PGPASSWORD"), dbname=env.get("PGDATABASE"), connect_timeout=2)
        try:
            result = update_lesson(conn, args.kind, lesson_id, args.expected_title, patch)
        finally:
            conn.close()
        print(json.dumps(result, ensure_ascii=False))
    except Exception as exc:
        print(json.dumps(_refusal(getattr(args, "kind", None), locals().get("lesson_id", getattr(args, "id", None)), str(exc)), ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
