#!/usr/bin/env python3
"""Apply ordered Athanor substrate migrations exactly once."""
from __future__ import annotations

import argparse
import os
from pathlib import Path

import psycopg2

import state_paths


def load_dotenv(path: Path) -> None:
    if not path.exists():
        return
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        value = value.strip()
        if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
            value = value[1:-1]
        os.environ.setdefault(key.strip(), value)


def connect(database: str | None = None):
    return psycopg2.connect(
        host=os.environ.get("PGHOST", "127.0.0.1"),
        port=os.environ.get("PGPORT", "5432"),
        user=os.environ["PGUSER"],
        password=os.environ["PGPASSWORD"],
        dbname=database or os.environ["PGDATABASE"],
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--database")
    parser.add_argument("--env-file")
    args = parser.parse_args()
    env_file = Path(args.env_file) if args.env_file else state_paths.default_dotenv_path()
    load_dotenv(env_file)

    migrations = sorted(Path(__file__).with_name("migrations").glob("[0-9][0-9][0-9][0-9]_*.sql"))
    if not migrations:
        raise SystemExit("no migrations found")

    with connect(args.database) as conn:
        with conn.cursor() as cur:
            cur.execute("CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW())")
            cur.execute("SELECT version FROM schema_migrations")
            applied = {row[0] for row in cur.fetchall()}
        for migration in migrations:
            version = int(migration.name[:4])
            if version in applied:
                print(f"skip {migration.name}")
                continue
            sql = migration.read_text(encoding="utf-8")
            with conn.cursor() as cur:
                cur.execute(sql)
                # Record it. This INSERT was missing outright: migrations were
                # applied and never written down, so `applied` above could only
                # ever be filled by some other writer, and every run re-applied
                # everything. Same cursor and transaction as the migration body
                # — a recorded migration is one that actually ran.
                cur.execute(
                    "INSERT INTO schema_migrations (version) VALUES (%s) "
                    "ON CONFLICT (version) DO NOTHING",
                    (version,),
                )
            print(f"applied {migration.name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
