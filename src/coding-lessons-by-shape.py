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
import sys
from substrate_config import substrate_env


try:
    import psycopg2
    import psycopg2.extras
except ImportError:
    psycopg2 = None




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
    parser.add_argument("--room", default="house",
                        help="room key; widens House-wide retrieval with that room's scope")
    args = parser.parse_args()

    try:
        env = substrate_env(args.room_dir)

        scopes = ["house"]
        room_scope = args.room.strip().lower()
        if room_scope and room_scope != "house":
            scopes.append(room_scope)

        if psycopg2 is None:
            raise RuntimeError("psycopg2 is required for coding lesson retrieval")
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
