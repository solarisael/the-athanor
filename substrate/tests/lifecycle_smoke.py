#!/usr/bin/env python3
"""Exercise the real write/embed/wake/delete lifecycle against a configured database."""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import uuid
from pathlib import Path

import psycopg2

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

import state_paths  # noqa: E402  (needs ROOT on sys.path)


def load_dotenv(path: Path) -> None:
    for raw in path.read_text(encoding="utf-8").splitlines() if path.exists() else []:
        line = raw.strip()
        if line and not line.startswith("#") and "=" in line:
            key, value = line.split("=", 1)
            os.environ.setdefault(key.strip(), value.strip())


def run(script: str, *args: str, stdin: str = "") -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(ROOT / script), *args],
        input=stdin,
        text=True,
        capture_output=True,
        check=True,
        timeout=180,
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--env-file", type=Path, default=state_paths.default_dotenv_path())
    args = parser.parse_args()
    load_dotenv(args.env_file)
    token = uuid.uuid4().hex[:12]
    room = f"smoke-{token}"
    source = f"db-only/substrate-smoke-{token}.md"
    body = f"Substrate lifecycle smoke token {token}."
    result = run(
        "record_memory.py",
        "--room", room,
        "--type", "paper-boat",
        "--title", f"Lifecycle smoke {token}",
        "--source-path", source,
        "--body-stdin",
        "--thread", "substrate / lifecycle / smoke",
        "--no-backup",
        stdin=body,
    )
    match = re.search(r"id=(\d+)", result.stdout)
    if not match:
        raise RuntimeError(f"record_memory did not return an id: {result.stdout!r}")
    memory_id = int(match.group(1))

    conn = psycopg2.connect(
        host=os.environ["PGHOST"], port=os.environ["PGPORT"],
        user=os.environ["PGUSER"], password=os.environ["PGPASSWORD"],
        dbname=os.environ["PGDATABASE"],
    )
    try:
        with conn, conn.cursor() as cur:
            cur.execute(
                "SELECT count(*), count(body_embedding) FROM memory_chunks WHERE memory_id = %s",
                (memory_id,),
            )
            chunks, embedded = cur.fetchone()
        if chunks < 1 or embedded != chunks:
            raise RuntimeError(f"memory {memory_id} has chunks={chunks}, embedded={embedded}")

        boat = json.loads(run("catch_boat.py", "--room", room).stdout)
        if boat.get("id") != memory_id or boat.get("body") != body:
            raise RuntimeError(f"wake mismatch: {boat}")

        print(json.dumps({
            "ok": True,
            "memoryId": memory_id,
            "chunks": chunks,
            "embeddedChunks": embedded,
            "wake": True,
        }))
    finally:
        with conn, conn.cursor() as cur:
            cur.execute("DELETE FROM memories WHERE id = %s", (memory_id,))
        conn.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
