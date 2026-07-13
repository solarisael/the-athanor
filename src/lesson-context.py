#!/usr/bin/env python3
"""Bounded, structured lesson retrieval for coding preflight.

The query is deliberately small and fail-open: callers may use the pure ranking
helpers in tests without a live PostgreSQL substrate, while CLI failures return
an empty context and exit successfully.
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
except Exception:  # pragma: no cover - exercised by unavailable-substrate tests
    psycopg2 = None


def _norm(value: object) -> str:
    return str(value or "").strip().lower()


def _terms(values: object) -> list[str]:
    if isinstance(values, str):
        values = values.replace(",", " ").split()
    return sorted({_norm(v) for v in (values or []) if _norm(v)})


def _row(row) -> dict:
    get = row.get if hasattr(row, "get") else lambda key, default=None: row[key] if key in row else default
    return {
        "id": int(get("id", 0)), "title": get("title", "") or "",
        "lesson": get("lesson", "") or "", "proof_pattern": get("proof_pattern", "") or "",
        "trigger_context": get("trigger_context", "") or "", "scope": get("scope", "") or "",
        "project": get("project", "") or "", "voice": get("voice", "") or "",
        "shape": get("shape", "") or "", "tags": list(get("tags", []) or []),
    }


def _rank(row: dict, terms: list[str], shapes: set[str], projects: set[str]) -> tuple[int, list[str]]:
    trigger = _norm(row.get("trigger_context"))
    tags = {_norm(v) for v in row.get("tags", [])}
    shape = _norm(row.get("shape"))
    project = _norm(row.get("project"))
    matched: list[str] = []
    score = 0
    trigger_tokens = {_norm(v) for v in trigger.replace(",", " ").split()}
    if trigger_tokens.intersection(terms):
        score += 32; matched.append("trigger")
    if tags.intersection(terms):
        score += 24; matched.append("tag")
    if shape and shape in shapes:
        score += 16; matched.append("shape")
    if project and project in projects:
        score += 12; matched.append("project")
    return score, matched


def _compact(row: dict, score: int, matched: list[str]) -> dict:
    return {"id": row["id"], "title": row["title"], "lesson": row["lesson"],
            "proof_pattern": row["proof_pattern"], "trigger_context": row["trigger_context"],
            "scope": row["scope"], "project": row["project"], "shape": row["shape"],
            "tags": row["tags"], "match": {"score": score, "matched": matched}}


def retrieve_lesson_context(conn, room: str, projects=(), shapes=(), terms=(), limit: int = 8) -> dict:
    room = _norm(room) or "shared"
    scopes = ["shared"] if room == "shared" else ["shared", room]
    project_keys = set(_terms(projects)); shape_keys = set(_terms(shapes)); query_terms = _terms(terms)
    limit = max(0, min(int(limit or 0), 50))
    if not limit:
        return {"codingLessons": [], "projectLessons": [], "match": {"scopes": scopes, "projects": sorted(project_keys), "limit": 0}}
    cur = conn.cursor(cursor_factory=psycopg2.extras.DictCursor if psycopg2 else None)
    try:
        cur.execute("""SELECT id,title,lesson,proof_pattern,trigger_context,scope,project,voice,shape,tags
                       FROM coding_lessons WHERE scope = ANY(%s)""", (scopes,))
        coding = [_row(r) for r in cur.fetchall()]
        project = []
        if project_keys:
            cur.execute("""SELECT id,title,lesson,proof_pattern,trigger_context,scope,project,voice,shape,tags
                           FROM project_lessons WHERE project = ANY(%s)""", (sorted(project_keys),))
            project = [_row(r) for r in cur.fetchall()]
    finally:
        cur.close()
    def ranked(rows):
        out = []
        for row in rows:
            score, matched = _rank(row, query_terms, shape_keys, project_keys)
            out.append(_compact(row, score, matched))
        out.sort(key=lambda item: (-item["match"]["score"], item["id"], item["title"].lower()))
        return out[:limit]
    return {"codingLessons": ranked(coding), "projectLessons": ranked(project),
            "match": {"scopes": scopes, "projects": sorted(project_keys), "terms": query_terms,
                       "shapes": sorted(shape_keys), "limit": limit}}




def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--room", required=True); parser.add_argument("--project", action="append", default=[])
    parser.add_argument("--shape", action="append", default=[]); parser.add_argument("--term", action="append", default=[])
    parser.add_argument("--limit", type=int, default=8); parser.add_argument("--room-dir", required=True)
    args = parser.parse_args()
    empty = {"codingLessons": [], "projectLessons": [], "match": {"scopes": ["shared"] if _norm(args.room) == "shared" else ["shared", _norm(args.room)], "projects": [], "limit": 0}}

    try:
        if psycopg2 is None: raise RuntimeError("psycopg2 unavailable")
        root = Path(args.room_dir).resolve().parent
        env = {}
        env_path = root / "house" / "substrate" / ".env"
        if env_path.exists():
            for line in env_path.read_text(encoding="utf-8").splitlines():
                if "=" in line and not line.lstrip().startswith("#"):
                    key, _, value = line.partition("="); env[key.strip()] = value.strip()
        env.update({k: os.environ[k] for k in ("PGHOST", "PGPORT", "PGUSER", "PGPASSWORD", "PGDATABASE") if os.environ.get(k)})
        conn = psycopg2.connect(host=env.get("PGHOST"), port=env.get("PGPORT"), user=env.get("PGUSER"), password=env.get("PGPASSWORD"), dbname=env.get("PGDATABASE"), connect_timeout=2)
        try: result = retrieve_lesson_context(conn, args.room, args.project, args.shape, args.term, args.limit)
        finally: conn.close()
        print(json.dumps(result, ensure_ascii=False))
    except Exception:
        print(json.dumps(empty, ensure_ascii=False))
    return 0

if __name__ == "__main__": raise SystemExit(main())
