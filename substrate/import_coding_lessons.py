#!/usr/bin/env python3
"""Validate and import a coding-lesson pack into the Athanor substrate."""
from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Any

from backup_runner import run_backup

import state_paths


DEFAULT_PACK = Path(__file__).parent / "starter-packs" / "coding-lessons.json"
REQUIRED_TEXT_FIELDS = ("title", "lesson", "shape")
OPTIONAL_TEXT_FIELDS = (
    "project",
    "voice",
    "trigger_context",
    "proof_pattern",
    "negation_of_title",
)


def load_dotenv(path: Path) -> None:
    if not path.exists():
        return
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        os.environ.setdefault(key.strip(), value.strip())


def env(name: str, default: str | None = None) -> str:
    value = os.environ.get(name, default)
    if value is None:
        raise ValueError(f"missing environment variable: {name}")
    return value


def require_object(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValueError(f"{label} must be a JSON object")
    return value


def require_text(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{label} must be a non-empty string")
    return value.strip()


def optional_text(value: Any, label: str) -> str | None:
    if value is None:
        return None
    return require_text(value, label)


def normalize_lesson(raw: Any, index: int, pack_meta: dict[str, Any]) -> dict[str, Any]:
    lesson = require_object(raw, f"lessons[{index}]")
    normalized: dict[str, Any] = {
        "scope": require_text(lesson.get("scope", "shared"), f"lessons[{index}].scope"),
        "always_on": lesson.get("always_on", False),
        "tags": lesson.get("tags", []),
        "thread_keys": lesson.get("thread_keys", []),
    }
    for field in REQUIRED_TEXT_FIELDS:
        normalized[field] = require_text(lesson.get(field), f"lessons[{index}].{field}")
    for field in OPTIONAL_TEXT_FIELDS:
        normalized[field] = optional_text(lesson.get(field), f"lessons[{index}].{field}")

    if not isinstance(normalized["always_on"], bool):
        raise ValueError(f"lessons[{index}].always_on must be a boolean")
    for field in ("tags", "thread_keys"):
        values = normalized[field]
        if not isinstance(values, list) or any(
            not isinstance(value, str) or not value.strip() for value in values
        ):
            raise ValueError(
                f"lessons[{index}].{field} must be an array of non-empty strings"
            )
        normalized[field] = list(dict.fromkeys(value.strip() for value in values))

    lesson_meta = lesson.get("meta", {})
    if not isinstance(lesson_meta, dict):
        raise ValueError(f"lessons[{index}].meta must be a JSON object")
    normalized["meta"] = {**lesson_meta, **pack_meta}
    return normalized


def load_pack(path: Path) -> dict[str, Any]:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise ValueError(f"pack does not exist: {path}") from exc
    except json.JSONDecodeError as exc:
        raise ValueError(f"pack is not valid JSON: {exc}") from exc

    root = require_object(document, "pack document")
    if root.get("schema_version") != 1:
        raise ValueError("schema_version must be 1")

    pack = require_object(root.get("pack"), "pack")
    pack_id = require_text(pack.get("id"), "pack.id")
    version = pack.get("version")
    if not isinstance(version, int) or version < 1:
        raise ValueError("pack.version must be a positive integer")
    title = require_text(pack.get("title"), "pack.title")

    raw_lessons = root.get("lessons")
    if not isinstance(raw_lessons, list) or not raw_lessons:
        raise ValueError("lessons must be a non-empty array")

    pack_meta = {
        "starter_pack": pack_id,
        "starter_pack_version": version,
    }
    lessons = [normalize_lesson(raw, index, pack_meta) for index, raw in enumerate(raw_lessons)]

    keys: set[tuple[str, str | None, str]] = set()
    for lesson in lessons:
        key = (lesson["scope"], lesson["project"], lesson["title"])
        if key in keys:
            raise ValueError(f"duplicate lesson key: {key}")
        keys.add(key)

    for lesson in lessons:
        target_title = lesson["negation_of_title"]
        if target_title is None:
            continue
        source_key = (lesson["scope"], lesson["project"], lesson["title"])
        target_key = (lesson["scope"], lesson["project"], target_title)
        if target_key == source_key:
            raise ValueError(f"lesson cannot negate itself: {lesson['title']}")
        if target_key not in keys:
            raise ValueError(
                f"negation target is not in the pack: {lesson['title']} -> {target_title}"
            )

    return {
        "id": pack_id,
        "version": version,
        "title": title,
        "description": optional_text(pack.get("description"), "pack.description"),
        "lessons": lessons,
    }


def find_lesson(
    cur: Any,
    scope: str,
    project: str | None,
    title: str,
) -> tuple[int, int | None] | None:
    cur.execute(
        """
        SELECT id, negation_of
        FROM lessons
        WHERE lesson_key = 'coding'
          AND scope = %s
          AND project IS NOT DISTINCT FROM %s
          AND title = %s
        """,
        (scope, project, title),
    )
    row = cur.fetchone()
    if not row:
        return None
    return int(row[0]), int(row[1]) if row[1] is not None else None


def find_existing(cur: Any, lesson: dict[str, Any]) -> int | None:
    row = find_lesson(cur, lesson["scope"], lesson["project"], lesson["title"])
    return row[0] if row else None


def insert_lesson(cur: Any, lesson: dict[str, Any]) -> None:
    cur.execute(
        """
        INSERT INTO lessons
          (lesson_key, scope, project, title, lesson, trigger_context, proof_pattern,
           thread_keys, tags, meta, shape, voice, always_on)
        VALUES
          ('coding', %s, %s, %s, %s, %s, %s, %s, %s, %s::jsonb, %s, %s, %s)
        """,
        (
            lesson["scope"],
            lesson["project"],
            lesson["title"],
            lesson["lesson"],
            lesson["trigger_context"],
            lesson["proof_pattern"],
            lesson["thread_keys"],
            lesson["tags"],
            json.dumps(lesson["meta"], ensure_ascii=False),
            lesson["shape"],
            lesson["voice"],
            lesson["always_on"],
        ),
    )


def update_lesson(cur: Any, lesson_id: int, lesson: dict[str, Any]) -> None:
    cur.execute(
        """
        UPDATE lessons
        SET lesson = %s,
            trigger_context = %s,
            proof_pattern = %s,
            thread_keys = %s,
            tags = %s,
            meta = meta || %s::jsonb,
            shape = %s,
            voice = %s,
            always_on = %s
        WHERE lesson_key = 'coding' AND id = %s
        """,
        (
            lesson["lesson"],
            lesson["trigger_context"],
            lesson["proof_pattern"],
            lesson["thread_keys"],
            lesson["tags"],
            json.dumps(lesson["meta"], ensure_ascii=False),
            lesson["shape"],
            lesson["voice"],
            lesson["always_on"],
            lesson_id,
        ),
    )


def link_negations(
    cur: Any,
    lessons: list[dict[str, Any]],
    update_existing: bool,
) -> dict[str, int]:
    counts = {"linked": 0, "link_skipped": 0}
    for lesson in lessons:
        target_title = lesson["negation_of_title"]
        if target_title is None:
            continue
        source = find_lesson(cur, lesson["scope"], lesson["project"], lesson["title"])
        target = find_lesson(cur, lesson["scope"], lesson["project"], target_title)
        if source is None or target is None:
            raise ValueError(
                f"cannot resolve negation link: {lesson['title']} -> {target_title}"
            )
        source_id, current_target_id = source
        target_id, _ = target
        if current_target_id == target_id:
            counts["link_skipped"] += 1
            continue
        if current_target_id is not None and not update_existing:
            counts["link_skipped"] += 1
            continue
        cur.execute(
            "UPDATE lessons SET negation_of = %s WHERE lesson_key = 'coding' AND id = %s",
            (target_id, source_id),
        )
        counts["linked"] += 1
    return counts


def import_pack(conn: Any, pack: dict[str, Any], update_existing: bool) -> dict[str, int]:
    counts = {
        "inserted": 0,
        "updated": 0,
        "skipped": 0,
        "linked": 0,
        "link_skipped": 0,
    }
    with conn, conn.cursor() as cur:
        for lesson in pack["lessons"]:
            lesson_id = find_existing(cur, lesson)
            if lesson_id is None:
                insert_lesson(cur, lesson)
                counts["inserted"] += 1
            elif update_existing:
                update_lesson(cur, lesson_id, lesson)
                counts["updated"] += 1
            else:
                counts["skipped"] += 1
        counts.update(link_negations(cur, pack["lessons"], update_existing))
    return counts


def connect() -> Any:
    try:
        import psycopg2
    except ImportError as exc:
        raise ValueError("psycopg2 is required for lesson imports") from exc
    return psycopg2.connect(
        host=env("PGHOST", "127.0.0.1"),
        port=int(env("PGPORT", "5432")),
        dbname=env("PGDATABASE"),
        user=env("PGUSER"),
        password=env("PGPASSWORD"),
    )


def main() -> int:
    parser = argparse.ArgumentParser(description="Import a coding-lesson pack.")
    parser.add_argument("--pack", default=str(DEFAULT_PACK))
    parser.add_argument("--env-file", default=str(state_paths.default_dotenv_path()))
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--update-existing", action="store_true")
    parser.add_argument("--no-backup", action="store_true")
    args = parser.parse_args()

    try:
        pack = load_pack(Path(args.pack).resolve())
        print(
            f"pack={pack['id']} version={pack['version']} "
            f"title={pack['title']!r} lessons={len(pack['lessons'])}"
        )
        if args.dry_run:
            for lesson in pack["lessons"]:
                print(f"  [dry] shape={lesson['shape']} title={lesson['title']}")
            return 0

        load_dotenv(Path(args.env_file))
        conn = connect()
        try:
            counts = import_pack(conn, pack, args.update_existing)
        finally:
            conn.close()
        print(
            f"inserted={counts['inserted']} updated={counts['updated']} "
            f"skipped={counts['skipped']} linked={counts['linked']} "
            f"link_skipped={counts['link_skipped']}"
        )
        if counts["inserted"] + counts["updated"] + counts["linked"] > 0:
            run_backup(__file__, skip=args.no_backup)
        return 0
    except (OSError, ValueError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
