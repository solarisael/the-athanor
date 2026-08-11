#!/usr/bin/env python3
"""Seed solarisael_memory.memories from a room's markdown corpus.

LEGACY (2026-05-19 single-writer migration): use `record_memory.py` for new
writes. This script remains as a one-shot rescue tool — e.g., re-importing
from disk after a substrate restore, or backfilling a legacy markdown corpus
that predates the single-writer flow. NOT part of the normal authoring path.

Idempotent: re-running upserts on (room, source_path). Does NOT chunk+embed
(use `embed_4b_pass.py` after, or — better — re-author via record_memory.py
which handles all of write+chunk+embed atomically).
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
from collections import defaultdict
from datetime import date as date_t
from pathlib import Path

import psycopg2
import psycopg2.extras

from backup_runner import run_backup

import state_paths

DATE_RE = re.compile(r"(\d{4}-\d{2}-\d{2})")
H1_RE = re.compile(r"^#\s+(.+)$", re.MULTILINE)


def env(name: str, default: str | None = None) -> str:
    v = os.environ.get(name, default)
    if v is None:
        sys.exit(f"missing env var: {name}")
    return v


def load_dotenv(path: Path) -> None:
    if not path.exists():
        return
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        k, _, v = line.partition("=")
        os.environ.setdefault(k.strip(), v.strip())


def parse_date(filename: str, fallback: str | None) -> date_t | None:
    m = DATE_RE.search(filename)
    src = m.group(1) if m else fallback
    if not src:
        return None
    try:
        return date_t.fromisoformat(src)
    except ValueError:
        return None


def extract_title(body: str, fallback: str) -> str:
    m = H1_RE.search(body)
    return m.group(1).strip() if m else fallback


def build_threads_for_file(threads_index: dict) -> dict:
    """Invert threads → {file_path: [(thread_key, lines, context), ...]}."""
    by_file: dict[str, list] = defaultdict(list)
    for raw_thread, entries in threads_index.items():
        thread = raw_thread.strip()
        if not thread:
            sys.exit("thread keys in index.json must be nonblank")
        for entry in entries:
            by_file[entry["file"]].append(
                {
                    "thread": thread,
                    "lines": entry.get("lines", []),
                    "context": entry.get("context", ""),
                }
            )
    return by_file


def discover_files(room_root: Path, files_index: dict, *, include_unindexed: bool = False) -> list[tuple[str, dict]]:
    """Yield (relative_path, metadata) for files known to the index, in path order."""
    items = []
    for rel, meta in files_index.items():
        full = room_root / rel
        if not full.exists():
            print(f"  SKIP missing: {rel}", file=sys.stderr)
            continue
        items.append((rel, meta))
    if include_unindexed:
        indexed = set(files_index.keys())
        memory_root = room_root / "memory"
        if memory_root.is_dir():
            for full in sorted(memory_root.rglob("*.md")):
                rel = full.relative_to(room_root).as_posix()
                if rel in indexed:
                    continue
                items.append((rel, {
                    "type": "legacy-unindexed",
                    "one_line": "Unindexed legacy room memory imported for Postgres full-text search.",
                }))
    items.sort()
    return items


def upsert_memory(cur, *, room: str, source_path: str, body: str, title: str,
                  date_: date_t | None, type_: str, threads: list[str], meta: dict,
                  thread_refs: list[dict]) -> int:
    """Upsert a memory and its registry events and detailed refs. Returns id."""
    cur.execute(
        """
        INSERT INTO memories (room, type, date, title, source_path, body, threads, meta)
        VALUES (%s, %s, %s, %s, %s, %s, %s, %s::jsonb)
        ON CONFLICT (room, source_path) DO UPDATE SET
            type = EXCLUDED.type,
            date = EXCLUDED.date,
            title = EXCLUDED.title,
            body = EXCLUDED.body,
            threads = EXCLUDED.threads,
            meta = EXCLUDED.meta,
            updated_at = NOW()
        RETURNING id
        """,
        (room, type_, date_, title, source_path, body, threads,
         json.dumps(meta, ensure_ascii=False)),
    )
    memory_id = cur.fetchone()[0]

    # Upsert one registry event per distinct thread, then replace only its
    # detailed refs. Repeated refs are intentionally retained.
    refs_by_thread: dict[str, list[dict]] = defaultdict(list)
    for ref in thread_refs:
        refs_by_thread[ref["thread"]].append(ref)
    event_ids: list[int] = []
    for thread in threads:
        cur.execute(
            """
            INSERT INTO threads (room, thread_key)
            VALUES (%s, %s)
            ON CONFLICT (room, thread_key) DO UPDATE
            SET thread_key = EXCLUDED.thread_key
            RETURNING id
            """,
            (room, thread),
        )
        thread_id = cur.fetchone()[0]
        cur.execute(
            """
            INSERT INTO thread_events (thread_id, memory_id)
            VALUES (%s, %s)
            ON CONFLICT (thread_id, memory_id) DO UPDATE
            SET memory_id = EXCLUDED.memory_id
            RETURNING id
            """,
            (thread_id, memory_id),
        )
        event_id = cur.fetchone()[0]
        event_ids.append(event_id)
        cur.execute("DELETE FROM memory_thread_refs WHERE event_id = %s", (event_id,))
        refs = refs_by_thread[thread]
        if refs:
            cur.executemany(
                """
                INSERT INTO memory_thread_refs
                    (event_id, lines_start, lines_end, context)
                VALUES (%s, %s, %s, %s)
                """,
                [
                    (
                        event_id,
                        (ref.get("lines") or [None, None])[0],
                        (ref.get("lines") or [None, None])[1]
                        if len(ref.get("lines") or []) >= 2 else None,
                        ref.get("context", ""),
                    )
                    for ref in refs
                ],
            )

    if event_ids:
        cur.execute(
            "DELETE FROM thread_events "
            "WHERE memory_id = %s AND NOT (id = ANY(%s))",
            (memory_id, event_ids),
        )
    else:
        cur.execute(
            "DELETE FROM thread_events WHERE memory_id = %s",
            (memory_id,),
        )

    return memory_id


def import_room(room: str, room_root: Path, *, dry_run: bool, include_unindexed: bool = False) -> tuple[int, int]:
    index_path = room_root / "memory" / "index.json"
    if not index_path.exists():
        sys.exit(f"no index.json at {index_path}")

    index = json.loads(index_path.read_text(encoding="utf-8"))
    files_index: dict = index.get("files", {})
    threads_index: dict = index.get("threads", {})
    by_file = build_threads_for_file(threads_index)

    items = discover_files(room_root, files_index, include_unindexed=include_unindexed)
    print(f"room={room}  files_indexed={len(files_index)}  resolved={len(items)}")

    if dry_run:
        for rel, meta in items[:5]:
            print(f"  [dry] {rel}  type={meta.get('type')}  threads={len(by_file.get(rel, []))}")
        print("  [dry] ...")
        return len(items), 0

    conn = psycopg2.connect(
        host=env("PGHOST"),
        port=env("PGPORT"),
        user=env("PGUSER"),
        password=env("PGPASSWORD"),
        dbname=env("PGDATABASE"),
    )
    inserted = 0
    try:
        with conn, conn.cursor() as cur:
            for rel, fmeta in items:
                full = room_root / rel
                body = full.read_text(encoding="utf-8")
                title = extract_title(body, fallback=Path(rel).stem)
                file_threads = by_file.get(rel, [])
                thread_keys = list(dict.fromkeys(t["thread"] for t in file_threads))
                meta = {
                    "one_line": fmeta.get("one_line", ""),
                    "thread_refs": file_threads,  # preserve line ranges + contexts
                }
                upsert_memory(
                    cur,
                    room=room,
                    source_path=rel,
                    body=body,
                    title=title,
                    date_=parse_date(rel, fmeta.get("date")),
                    type_=fmeta.get("type", "session"),
                    threads=thread_keys,
                    meta=meta,
                    thread_refs=file_threads,
                )
                inserted += 1
    finally:
        conn.close()
    print(f"  upserted={inserted}")
    return len(items), inserted


def main() -> None:
    parser = argparse.ArgumentParser(description="Import a room's markdown corpus into solarisael_memory.")
    parser.add_argument("--room", required=True, help="stable room key")
    parser.add_argument("--root", required=True, help="path to the room root (the dir containing memory/index.json)")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--include-unindexed", action="store_true",
                        help="also import memory/**/*.md files not listed in memory/index.json as legacy-unindexed rows")
    parser.add_argument("--env-file", default=str(state_paths.default_dotenv_path()))
    parser.add_argument("--no-backup", dest="backup", action="store_false",
                        help="skip the Rust-owned backup after success (default: backup runs)")
    parser.set_defaults(backup=True)
    args = parser.parse_args()

    load_dotenv(Path(args.env_file))
    room_root = Path(args.root).resolve()
    if not room_root.is_dir():
        sys.exit(f"not a directory: {room_root}")

    resolved, inserted = import_room(
        args.room,
        room_root,
        dry_run=args.dry_run,
        include_unindexed=args.include_unindexed,
    )
    if not args.dry_run:
        print(f"done: {inserted}/{resolved}")
        if args.backup and inserted > 0:
            run_backup(__file__)


if __name__ == "__main__":
    main()
