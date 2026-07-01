#!/usr/bin/env python3
"""Fetch coding_lessons rows for a given shape, scoped to shared+room.

Returns JSON: { lessons: [{ id, title, lesson, proof_pattern, scope, voice,
shape, negation_of, tags }, ...] }

Used by the OpenCode plugin's tool.execute.before hook to surface
shape-relevant lessons immediately before risky tool calls fire.

Stays fail-open: prints `{ "lessons": [] }` and exits 0 on any error so the
hook never breaks tool execution.
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
except Exception as e:
    print(json.dumps({"lessons": [], "error": f"psycopg2 import failed: {e}"}))
    sys.exit(0)


def read_env_file(path: Path) -> dict[str, str]:
    out: dict[str, str] = {}
    if not path.exists():
        return out

    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, _, value = line.partition("=")
        out[key.strip()] = value.strip()

    return out


def substrate_env(room_dir: Path) -> dict[str, str]:
    shared_root = room_dir.parent
    values: dict[str, str] = {}
    for env_path in (
        shared_root / "house" / "substrate" / ".env",
        shared_root / "kodo" / "substrate" / ".env",
    ):
        values = read_env_file(env_path)
        if values:
            break

    for key in ("PGHOST", "PGPORT", "PGUSER", "PGPASSWORD", "PGDATABASE"):
        if os.environ.get(key):
            values[key] = os.environ[key]

    return values


def fetch_lessons(conn, shape: str, scopes: list[str]) -> list[dict]:
    with conn.cursor(cursor_factory=psycopg2.extras.DictCursor) as cur:
        cur.execute(
            """
            SELECT id, title, lesson, proof_pattern, trigger_context,
                   scope, voice, shape, negation_of, tags
            FROM coding_lessons
            WHERE shape = %s
              AND scope = ANY(%s)
            ORDER BY
              CASE WHEN negation_of IS NULL THEN 0 ELSE 1 END,
              id
            """,
            (shape, scopes),
        )
        rows = []
        for row in cur.fetchall():
            rows.append({
                "id": row["id"],
                "title": row["title"],
                "lesson": row["lesson"],
                "proof_pattern": row["proof_pattern"],
                "trigger_context": row["trigger_context"],
                "scope": row["scope"],
                "voice": row["voice"],
                "shape": row["shape"],
                "negation_of": row["negation_of"],
                "tags": list(row["tags"] or []),
            })

        return rows

def fetch_taxonomy(conn, scopes: list[str]) -> dict:
    with conn.cursor(cursor_factory=psycopg2.extras.DictCursor) as cur:
        cur.execute(
            """
            SELECT shape,
                   COUNT(*) AS count,
                   COUNT(*) FILTER (WHERE always_on) AS always_on_count
            FROM coding_lessons
            WHERE scope = ANY(%s)
            GROUP BY shape
            ORDER BY count DESC, shape
            """,
            (scopes,),
        )
        shapes = [
            {
                "shape": row["shape"] or "unknown",
                "count": int(row["count"]),
                "always_on_count": int(row["always_on_count"] or 0),
            }
            for row in cur.fetchall()
        ]
        return {"scopes": scopes, "shapes": shapes}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--room-dir", required=True)
    parser.add_argument("--shape", required=True)
    parser.add_argument("--room", default="shared",
                        help="agent room (kodo|kintsu|shared); used to widen scope")
    args = parser.parse_args()

    try:
        room_dir = Path(args.room_dir).resolve()
        env = substrate_env(room_dir)

        scopes = ["shared"]
        if args.room.lower() in ("kodo", "kintsu"):
            scopes.append(args.room.lower())

        conn = psycopg2.connect(
            host=env.get("PGHOST"),
            port=env.get("PGPORT"),
            user=env.get("PGUSER"),
            password=env.get("PGPASSWORD"),
            dbname=env.get("PGDATABASE"),
            connect_timeout=2,
        )
        try:
            lessons = fetch_lessons(conn, args.shape, scopes)
            taxonomy = fetch_taxonomy(conn, scopes)
        finally:
            conn.close()

        print(json.dumps({"lessons": lessons, "taxonomy": taxonomy}, ensure_ascii=False))
    except Exception as e:
        # Fail open — empty result, exit 0, hook stays soft.
        print(json.dumps({"lessons": [], "error": str(e)}))

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
