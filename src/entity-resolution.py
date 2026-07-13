#!/usr/bin/env python3
"""Resolve named entities from the House PostgreSQL substrate.

The resolver is deliberately small and lexical: the substrate supplies canonical
names and aliases, while this module supplies normalization, boundary-safe
matching, ranking, and a fail-open CLI seam for plugin callers.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path
from typing import Any, Iterable

try:
    import psycopg2
    import psycopg2.extras
except Exception:  # pragma: no cover - exercised by unavailable-substrate tests
    psycopg2 = None

DEFAULT_LIMIT = 8
_MAX_LIMIT = 32
_SPACE_RE = re.compile(r"[^\W_]+", re.UNICODE)


def normalize(value: Any) -> str:
    """Case-fold and turn punctuation/separators into single spaces."""
    text = str(value or "").casefold().replace("’", "'")
    return " ".join(_SPACE_RE.findall(text))


def _aliases(value: Any) -> list[str]:
    if isinstance(value, str):
        try:
            parsed = json.loads(value)
            value = parsed
        except (TypeError, ValueError):
            value = [value]
    if not isinstance(value, (list, tuple, set)):
        return []
    return [str(item) for item in value if str(item or "").strip()]


def resolve_matches(query: str, entities: Iterable[dict[str, Any]], limit: int = DEFAULT_LIMIT) -> list[dict[str, str]]:
    """Return bounded, deduplicated canonical entity matches for *query*."""
    query_norm = normalize(query)
    if not query_norm:
        return []
    try:
        bound = max(0, min(int(limit), _MAX_LIMIT))
    except (TypeError, ValueError):
        bound = DEFAULT_LIMIT
    if not bound:
        return []

    candidates: list[tuple[int, int, int, dict[str, str]]] = []
    for entity_index, entity in enumerate(entities or []):
        if not isinstance(entity, dict):
            continue
        name = str(entity.get("name") or entity.get("canonicalName") or "").strip()
        canonical = normalize(name)
        if not canonical:
            continue
        labels = [name, *_aliases(entity.get("aliases"))]
        seen_labels: set[str] = set()
        for alias_index, label in enumerate(labels):
            alias_norm = normalize(label)
            if not alias_norm or alias_norm in seen_labels:
                continue
            if len(alias_norm.replace(" ", "")) < 3:
                continue
            seen_labels.add(alias_norm)
            # Spaces on both sides make matches token-boundary safe.
            at = f" {query_norm} ".find(f" {alias_norm} ")
            if at < 0:
                continue
            matched = label.strip() or name
            result = {
                "canonicalName": name,
                "kind": str(entity.get("kind") or ""),
                "matchedAlias": matched,
            }
            # Sort longest first, then query position and stable substrate order.
            candidates.append((len(alias_norm.split()), at, entity_index * 1000 + alias_index, result))

    candidates.sort(key=lambda item: (-item[0], item[1], item[2]))
    output: list[dict[str, str]] = []
    seen_entities: set[tuple[str, str]] = set()
    for _, _, _, result in candidates:
        key = (normalize(result["canonicalName"]), result["kind"])
        if key in seen_entities:
            continue
        seen_entities.add(key)
        output.append(result)
        if len(output) >= bound:
            break
    return output


def _substrate_env(room_dir: str | Path) -> dict[str, str]:
    root = Path(room_dir).resolve().parent
    values: dict[str, str] = {}
    env_path = root / "house" / "substrate" / ".env"
    try:
        for line in env_path.read_text(encoding="utf-8").splitlines():
            key, sep, value = line.partition("=")
            if sep and key.strip():
                values[key.strip()] = value.strip().strip('"').strip("'")
    except OSError:
        pass
    values.update({key: os.environ[key] for key in ("PGHOST", "PGPORT", "PGUSER", "PGPASSWORD", "PGDATABASE") if os.environ.get(key)})
    return values


def fetch_matches(query: str, room: str, room_dir: str | Path, limit: int = DEFAULT_LIMIT) -> list[dict[str, str]]:
    if psycopg2 is None:
        return []
    env = _substrate_env(room_dir)
    conn = None
    try:
        conn = psycopg2.connect(host=env.get("PGHOST"), port=env.get("PGPORT"), user=env.get("PGUSER"), password=env.get("PGPASSWORD"), dbname=env.get("PGDATABASE"), connect_timeout=2)
        with conn.cursor(cursor_factory=psycopg2.extras.DictCursor) as cur:
            cur.execute(
                "SELECT name, kind, aliases FROM named_entities WHERE room = ANY(%s) ORDER BY name",
                ([room, "house", "shared"],),
            )
            return resolve_matches(query, [dict(row) for row in cur.fetchall()], limit)
    except Exception:
        return []
    finally:
        if conn is not None:
            try:
                conn.close()
            except Exception:
                pass


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--room", required=True)
    parser.add_argument("--room-dir", required=True)
    parser.add_argument("--query-stdin", action="store_true")
    parser.add_argument("--query", default="")
    parser.add_argument("--limit", type=int, default=DEFAULT_LIMIT)
    args = parser.parse_args(argv)
    query = sys.stdin.read() if args.query_stdin else args.query
    print(json.dumps({"matches": fetch_matches(query, args.room, args.room_dir, args.limit)}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
