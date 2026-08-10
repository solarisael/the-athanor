#!/usr/bin/env python3
"""Bounded, structured lesson retrieval for coding preflight.

The query is deliberately small and fail-open: callers may use the pure ranking
helpers in tests without a live PostgreSQL substrate, while CLI failures return
an empty context and exit successfully.
"""
from __future__ import annotations

import argparse
import json
import sys

from substrate_config import substrate_env

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
        "id": int(get("id", 0)), "type": get("lesson_key", "") or "",
        "title": get("title", "") or "", "lesson": get("lesson", "") or "",
        "proof_pattern": get("proof_pattern", "") or "",
        "trigger_context": get("trigger_context", "") or "", "scope": get("scope", "") or "",
        "project": get("project", "") or "", "voice": get("voice", "") or "",
        "register": get("register", "") or "", "shape": get("shape", "") or "",
        "stage": list(get("stage", []) or []), "tags": list(get("tags", []) or []),
        "language_keys": list(get("language_keys", []) or []),
        "technology_keys": list(get("technology_keys", []) or []),
    }


def _eligible(row: dict, *, lesson_type: str, scopes: set[str], projects: set[str],
              stages: set[str], registers: set[str], languages: set[str],
              technologies: set[str]) -> bool:
    """Apply authority rails before any lexical or semantic rank is considered."""
    if _norm(row.get("type")) != lesson_type:
        return False
    if lesson_type == "coding":
        if _norm(row.get("scope")) not in scopes:
            return False
        # A project-scoped coding lesson is never portable across projects.
        lesson_project = _norm(row.get("project"))
        if lesson_project and lesson_project not in projects:
            return False
    elif _norm(row.get("project")) not in projects:
        return False
    declared_stages = set(_terms(row.get("stage")))
    if declared_stages and not declared_stages.intersection(stages):
        return False
    declared_register = _norm(row.get("register"))
    if declared_register and declared_register not in registers:
        return False
    declared_languages = set(_terms(row.get("language_keys")))
    if declared_languages and not declared_languages.intersection(languages):
        return False
    declared_technologies = set(_terms(row.get("technology_keys")))
    if declared_technologies and not declared_technologies.intersection(technologies):
        return False
    return True


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
    return {
        "id": row["id"], "type": row["type"], "title": row["title"], "lesson": row["lesson"],
        "proof_pattern": row["proof_pattern"], "trigger_context": row["trigger_context"],
        "scope": row["scope"], "project": row["project"], "register": row["register"],
        "shape": row["shape"], "stage": row["stage"], "tags": row["tags"],
        "language_keys": row["language_keys"], "technology_keys": row["technology_keys"],
        "match": {"score": score, "matched": matched},
    }


def retrieve_lesson_context(conn, room: str, projects=(), shapes=(), terms=(), *,
                            stages=(), registers=(), languages=(), technologies=(),
                            limit: int = 8) -> dict:
    room = _norm(room) or "house"
    scopes = ["house"] if room == "house" else ["house", room]
    scope_keys = set(scopes)
    project_keys = set(_terms(projects)); shape_keys = set(_terms(shapes)); query_terms = _terms(terms)
    stage_keys = set(_terms(stages)); register_keys = set(_terms(registers))
    language_keys = set(_terms(languages)); technology_keys = set(_terms(technologies))
    limit = max(0, min(int(limit or 0), 50))
    match = {
        "scopes": scopes, "projects": sorted(project_keys), "terms": query_terms,
        "shapes": sorted(shape_keys), "stages": sorted(stage_keys),
        "registers": sorted(register_keys), "languages": sorted(language_keys),
        "technologies": sorted(technology_keys), "limit": limit,
    }
    if not limit:
        return {"codingLessons": [], "projectLessons": [], "match": match}
    cur = conn.cursor(cursor_factory=psycopg2.extras.DictCursor if psycopg2 else None)
    try:
        cur.execute("""SELECT id,lesson_key,title,lesson,proof_pattern,trigger_context,scope,project,
                              voice,register,shape,stage,tags,language_keys,technology_keys
                       FROM lessons
                       WHERE lesson_key = 'coding' AND scope = ANY(%s)""", (scopes,))
        coding = [_row(r) for r in cur.fetchall()]
        coding = [row for row in coding if _eligible(
            row, lesson_type="coding", scopes=scope_keys, projects=project_keys,
            stages=stage_keys, registers=register_keys, languages=language_keys,
            technologies=technology_keys,
        )]
        project = []
        if project_keys:
            cur.execute("""SELECT id,lesson_key,title,lesson,proof_pattern,trigger_context,scope,project,
                                  voice,register,shape,stage,tags,language_keys,technology_keys
                           FROM lessons
                           WHERE lesson_key = 'project' AND project = ANY(%s)""", (sorted(project_keys),))
            project = [_row(r) for r in cur.fetchall()]
            project = [row for row in project if _eligible(
                row, lesson_type="project", scopes=scope_keys, projects=project_keys,
                stages=stage_keys, registers=register_keys, languages=language_keys,
                technologies=technology_keys,
            )]
    finally:
        cur.close()

    def ranked(rows):
        out = []
        for row in rows:
            score, matched = _rank(row, query_terms, shape_keys, project_keys)
            out.append(_compact(row, score, matched))
        out.sort(key=lambda item: (-item["match"]["score"], item["id"], item["title"].lower()))
        return out[:limit]

    return {"codingLessons": ranked(coding), "projectLessons": ranked(project), "match": match}




def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--room", required=True); parser.add_argument("--project", action="append", default=[])
    parser.add_argument("--shape", action="append", default=[]); parser.add_argument("--term", action="append", default=[])
    parser.add_argument("--stage", action="append", default=[]); parser.add_argument("--register", action="append", default=[])
    parser.add_argument("--language", action="append", default=[]); parser.add_argument("--technology", action="append", default=[])
    parser.add_argument("--limit", type=int, default=8); parser.add_argument("--room-dir", required=True)
    args = parser.parse_args()
    empty = {"codingLessons": [], "projectLessons": [], "match": {"scopes": ["house"] if _norm(args.room) == "house" else ["house", _norm(args.room)], "projects": [], "limit": 0}}

    try:
        if psycopg2 is None: raise RuntimeError("psycopg2 unavailable")
        env = substrate_env()
        conn = psycopg2.connect(host=env.get("PGHOST"), port=env.get("PGPORT"), user=env.get("PGUSER"), password=env.get("PGPASSWORD"), dbname=env.get("PGDATABASE"), connect_timeout=2)
        try: result = retrieve_lesson_context(
            conn, args.room, args.project, args.shape, args.term,
            stages=args.stage, registers=args.register, languages=args.language,
            technologies=args.technology, limit=args.limit,
        )
        finally: conn.close()
        print(json.dumps(result, ensure_ascii=False))
    except Exception as error:
        # Fail-open by contract: lessons must never block a turn. Keep the
        # database failure visible without changing the successful exit.
        empty["error"] = f"{type(error).__name__}: {error}"[:300]
        print(json.dumps(empty, ensure_ascii=False))
    return 0

if __name__ == "__main__": raise SystemExit(main())
