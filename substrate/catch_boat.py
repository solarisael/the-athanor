#!/usr/bin/env python3
"""Catch the latest paper boat — the wake (anamnesis) read.

Prints the most recent type='paper-boat' memory for a room as JSON, or
{"found": false}. Single job: the latest-row lookup wake needs to surface
yesterday's word + reminders. No ranking, no embed — that is the retrieval
reader's job, not this one.
"""
from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

import psycopg2

import state_paths


def load_dotenv(path: Path) -> None:
    if not path.exists():
        return
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        k, _, v = line.partition("=")
        os.environ[k.strip()] = v.strip()


def main() -> int:
    p = argparse.ArgumentParser(description="Catch the latest paper boat for a room.")
    p.add_argument("--room", required=True)
    p.add_argument("--env-file", default=str(state_paths.default_dotenv_path()))
    args = p.parse_args()

    load_dotenv(Path(args.env_file))
    conn = psycopg2.connect(
        host=os.environ["PGHOST"], port=os.environ["PGPORT"],
        user=os.environ["PGUSER"], password=os.environ["PGPASSWORD"],
        dbname=os.environ["PGDATABASE"], connect_timeout=10,
    )
    try:
        with conn.cursor() as cur:
            cur.execute(
                """
                SELECT id, title, body, date, created_at
                FROM memories
                WHERE room = %s AND type = 'paper-boat'
                ORDER BY created_at DESC
                LIMIT 1
                """,
                (args.room.lower(),),
            )
            row = cur.fetchone()
    finally:
        conn.close()

    if not row:
        print(json.dumps({"found": False}))
        return 0

    print(json.dumps({
        "found": True,
        "id": row[0],
        "title": row[1],
        "body": row[2],
        "date": row[3].isoformat() if row[3] else None,
        "created_at": row[4].isoformat() if row[4] else None,
    }, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
