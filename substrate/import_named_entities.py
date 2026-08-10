#!/usr/bin/env python3
"""Import important_index.json named entities into solarisael_memory.named_entities.

LEGACY (2026-05-19 single-writer migration): postgres is now authoritative for
canon. Update `named_entities` rows directly (or via `record_memory.py`'s
`--canon-touches` flag to append pointer_files). This script remains as a
one-shot rescue for re-importing an entire important_index.json after a
substrate restore. NOT part of the normal authoring path.

Idempotent upsert on (room, name). Re-running picks up edits.
"""
from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

import psycopg2

from backup_runner import run_backup

import state_paths


def env(name: str) -> str:
    v = os.environ.get(name)
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


def upsert(cur, *, room: str, name: str, entry: dict) -> None:
    cur.execute(
        """
        INSERT INTO named_entities
            (room, name, kind, summary, aliases, search_boost, weighty, pointer_files, meta)
        VALUES (%s, %s, %s, %s, %s, %s, %s, %s::jsonb, %s::jsonb)
        ON CONFLICT (room, name) DO UPDATE SET
            kind = EXCLUDED.kind,
            summary = EXCLUDED.summary,
            aliases = EXCLUDED.aliases,
            search_boost = EXCLUDED.search_boost,
            weighty = EXCLUDED.weighty,
            pointer_files = EXCLUDED.pointer_files,
            meta = EXCLUDED.meta,
            updated_at = NOW()
        """,
        (
            room,
            name,
            entry.get("type", "unknown"),
            entry.get("summary", ""),
            entry.get("aliases", []) or [],
            entry.get("search_boost"),
            bool(entry.get("weighty", False)),
            json.dumps(entry.get("files", []) or [], ensure_ascii=False),
            json.dumps({k: v for k, v in entry.items()
                       if k not in {"type", "summary", "aliases",
                                    "search_boost", "weighty", "files"}},
                      ensure_ascii=False),
        ),
    )


def import_room(room: str, room_root: Path, *, dry_run: bool) -> int:
    idx_path = room_root / "memory" / "important_index.json"
    if not idx_path.exists():
        print(f"no important_index.json at {idx_path}, skipping")
        return 0
    data = json.loads(idx_path.read_text(encoding="utf-8"))
    entries = data.get("entries", {})
    print(f"room={room}  named_entities={len(entries)}")

    if dry_run:
        for i, (name, entry) in enumerate(list(entries.items())[:5]):
            print(f"  [dry] {name}  kind={entry.get('type')}  weighty={entry.get('weighty', False)}  aliases={len(entry.get('aliases', []) or [])}")
        if len(entries) > 5:
            print(f"  [dry] ... +{len(entries)-5} more")
        return 0

    conn = psycopg2.connect(
        host=env("PGHOST"), port=env("PGPORT"),
        user=env("PGUSER"), password=env("PGPASSWORD"),
        dbname=env("PGDATABASE"),
    )
    n = 0
    try:
        with conn, conn.cursor() as cur:
            for name, entry in entries.items():
                if not isinstance(entry, dict):
                    print(f"  SKIP non-dict entry: {name}")
                    continue
                upsert(cur, room=room, name=name, entry=entry)
                n += 1
    finally:
        conn.close()
    print(f"  upserted={n}")
    return n


def main() -> None:
    p = argparse.ArgumentParser(description="Import important_index.json into solarisael_memory.named_entities.")
    p.add_argument("--room", required=True)
    p.add_argument("--root", required=True,
                   help="path to room root (the dir containing memory/important_index.json)")
    p.add_argument("--dry-run", action="store_true")
    p.add_argument("--env-file", default=str(state_paths.default_dotenv_path()))
    p.add_argument("--no-backup", dest="backup", action="store_false")
    p.set_defaults(backup=True)
    args = p.parse_args()

    load_dotenv(Path(args.env_file))
    room_root = Path(args.root).resolve()
    if not room_root.is_dir():
        sys.exit(f"not a directory: {room_root}")

    n = import_room(args.room, room_root, dry_run=args.dry_run)
    if not args.dry_run and n > 0 and args.backup:
        run_backup(__file__)


if __name__ == "__main__":
    main()
