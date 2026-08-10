#!/usr/bin/env python3
"""Bounded canonical retrieval across every typed lesson row."""
from __future__ import annotations

import argparse
import json

from substrate_config import substrate_env

try:
    import psycopg2
    import psycopg2.extras
except ImportError:
    psycopg2 = None


LESSON_TYPES = ("coding", "project", "writing", "design", "audio")


def _row(value) -> dict:
    return {
        "id": int(value["id"]),
        "type": value["lesson_key"],
        "kindPath": value["kind_path"],
        "scope": value["scope"],
        "project": value["project"],
        "voice": value["voice"],
        "register": list(value["register"] or []),
        "shape": value["shape"],
        "stage": list(value["stage"] or []),
        "title": value["title"],
        "lesson": value["lesson"],
        "triggerContext": value["trigger_context"],
        "proofPattern": value["proof_pattern"],
        "exampleText": value["example_text"],
        "exampleCommand": value["example_cmd"],
        "writers": list(value["writers"] or []),
        "tools": list(value["tools"] or []),
        "negationOf": value["negation_of"],
        "languageKeys": list(value.get("language_keys") or []),
        "technologyKeys": list(value.get("technology_keys") or []),
        "threadKeys": list(value.get("thread_keys") or []),
        "tags": list(value["tags"] or []),
        "alwaysOn": bool(value["always_on"]),
    }


def fetch_lessons(conn, *, lesson_type: str, room: str, shape: str | None,
                  project: str | None, register: str | None, stage: str | None,
                  query: str | None, limit: int, language_keys: list[str] = [],
                  technology_keys: list[str] = []) -> dict:
    if lesson_type not in LESSON_TYPES:
        raise ValueError(f"type must be one of: {', '.join(LESSON_TYPES)}")
    if lesson_type == "project" and not project:
        raise ValueError("project lessons require --project")
    scopes = ["house"] if room == "house" else ["house", room]
    clauses = ["lesson_key = %s"]
    values: list[object] = [lesson_type]
    if lesson_type == "coding":
        clauses.append("scope = ANY(%s)")
        values.append(scopes)
    if project:
        clauses.append("project = %s")
        values.append(project)
    if shape:
        clauses.append("shape = %s")
        values.append(shape)
    if register:
        clauses.append("%s = ANY(register)")
        values.append(register)
    if stage:
        clauses.append("%s = ANY(stage)")
        values.append(stage)
    clauses.append(
        "(cardinality(language_keys) = 0 OR language_keys && %s)"
        if language_keys else "cardinality(language_keys) = 0"
    )
    if language_keys:
        values.append(language_keys)
    clauses.append(
        "(cardinality(technology_keys) = 0 OR technology_keys && %s)"
        if technology_keys else "cardinality(technology_keys) = 0"
    )
    if technology_keys:
        values.append(technology_keys)
    if query:
        clauses.append(
            "lesson_tsv @@ plainto_tsquery("
            "CASE WHEN lesson_key = 'audio' THEN 'english'::regconfig "
            "ELSE 'portuguese'::regconfig END, %s)"
        )
        values.append(query)
    where = " AND ".join(clauses)
    rank_query = query or ""
    with conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as cur:
        cur.execute(
            f"""
            SELECT id,lesson_key,kind_path,scope,project,voice,register,shape,stage,
                   title,lesson,trigger_context,proof_pattern,example_text,example_cmd,
                   writers,tools,negation_of,language_keys,technology_keys,tags,thread_keys,
                   always_on
            FROM lessons
            WHERE {where}
            ORDER BY
              always_on DESC,
              CASE WHEN %s <> '' THEN
                ts_rank(
                  lesson_tsv,
                  plainto_tsquery(
                    CASE WHEN lesson_key = 'audio' THEN 'english'::regconfig
                         ELSE 'portuguese'::regconfig END,
                    %s
                  )
                )
              ELSE 0 END DESC,
              updated_at DESC,
              id
            LIMIT %s
            """,
            (*values, rank_query, rank_query, limit),
        )
        rows = [_row(row) for row in cur.fetchall()]
        expansion_keys = sorted({
            thread_key
            for row in rows
            for thread_key in row["threadKeys"]
            if isinstance(thread_key, str) and thread_key.strip()
        })
        if expansion_keys and len(rows) < 50:
            expansion_clauses = [
                "lesson_key = %s",
                "thread_keys && %s",
                "NOT (id = ANY(%s))",
            ]
            expansion_values: list[object] = [
                lesson_type,
                expansion_keys,
                [row["id"] for row in rows],
            ]
            if lesson_type == "coding":
                expansion_clauses.append("scope = ANY(%s)")
                expansion_values.append(scopes)
            if project:
                expansion_clauses.append("project = %s")
                expansion_values.append(project)
            expansion_clauses.append(
                "(cardinality(language_keys) = 0 OR language_keys && %s)"
                if language_keys else "cardinality(language_keys) = 0"
            )
            if language_keys:
                expansion_values.append(language_keys)
            expansion_clauses.append(
                "(cardinality(technology_keys) = 0 OR technology_keys && %s)"
                if technology_keys else "cardinality(technology_keys) = 0"
            )
            if technology_keys:
                expansion_values.append(technology_keys)
            cur.execute(
                f"""
                SELECT id,lesson_key,kind_path,scope,project,voice,register,shape,stage,
                       title,lesson,trigger_context,proof_pattern,example_text,example_cmd,
                       writers,tools,negation_of,language_keys,technology_keys,tags,thread_keys,
                       always_on
                FROM lessons
                WHERE {' AND '.join(expansion_clauses)}
                ORDER BY always_on DESC, updated_at DESC, id
                LIMIT %s
                """,
                (*expansion_values, 50 - len(rows)),
            )
            rows.extend(_row(row) for row in cur.fetchall())

        taxonomy_clauses = ["lesson_key = %s"]
        taxonomy_values: list[object] = [lesson_type]
        if lesson_type == "coding":
            taxonomy_clauses.append("scope = ANY(%s)")
            taxonomy_values.append(scopes)
        if project:
            taxonomy_clauses.append("project = %s")
            taxonomy_values.append(project)
        taxonomy_clauses.append(
            "(cardinality(language_keys) = 0 OR language_keys && %s)"
            if language_keys else "cardinality(language_keys) = 0"
        )
        if language_keys:
            taxonomy_values.append(language_keys)
        taxonomy_clauses.append(
            "(cardinality(technology_keys) = 0 OR technology_keys && %s)"
            if technology_keys else "cardinality(technology_keys) = 0"
        )
        if technology_keys:
            taxonomy_values.append(technology_keys)
        cur.execute(
            f"""
            SELECT kind_path,shape,COUNT(*) AS count,
                   COUNT(*) FILTER (WHERE always_on) AS always_on_count
            FROM lessons
            WHERE {' AND '.join(taxonomy_clauses)}
            GROUP BY kind_path,shape
            ORDER BY count DESC,kind_path
            """,
            taxonomy_values,
        )
        taxonomy = [{
            "kindPath": row["kind_path"],
            "shape": row["shape"],
            "count": int(row["count"]),
            "alwaysOnCount": int(row["always_on_count"]),
        } for row in cur.fetchall()]
    return {
        "ok": True,
        "type": lesson_type,
        "filters": {
            "room": room,
            "scopes": scopes if lesson_type == "coding" else [],
            "shape": shape,
            "project": project,
            "register": register,
            "stage": stage,
            "languageKeys": language_keys,
            "technologyKeys": technology_keys,
            "query": query,
            "limit": limit,
        },
        "lessons": rows,
        "taxonomy": taxonomy,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--room-dir", required=True)
    parser.add_argument("--room", default="house")
    parser.add_argument("--type", required=True, choices=LESSON_TYPES)
    parser.add_argument("--shape")
    parser.add_argument("--project")
    parser.add_argument("--register")
    parser.add_argument("--stage")
    parser.add_argument("--language-key", action="append", default=[])
    parser.add_argument("--technology-key", action="append", default=[])
    parser.add_argument("--query")
    parser.add_argument("--limit", type=int, default=12)
    args = parser.parse_args()
    try:
        if psycopg2 is None:
            raise RuntimeError("psycopg2 is required for lesson retrieval")
        if not 1 <= args.limit <= 50:
            raise ValueError("--limit must be between 1 and 50")
        env = substrate_env()
        conn = psycopg2.connect(
            host=env.get("PGHOST"),
            port=env.get("PGPORT"),
            user=env.get("PGUSER"),
            password=env.get("PGPASSWORD"),
            dbname=env.get("PGDATABASE"),
            connect_timeout=2,
        )
        try:
            result = fetch_lessons(
                conn,
                lesson_type=args.type,
                room=args.room.strip().lower() or "house",
                shape=args.shape,
                project=args.project,
                register=args.register,
                stage=args.stage,
                language_keys=sorted(set(args.language_key)),
                technology_keys=sorted(set(args.technology_key)),
                query=args.query,
                limit=args.limit,
            )
        finally:
            conn.close()
        print(json.dumps(result, ensure_ascii=False))
    except Exception as error:
        print(json.dumps({
            "ok": False,
            "type": args.type,
            "lessons": [],
            "taxonomy": [],
            "error": f"{type(error).__name__}: {error}"[:500],
        }, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
