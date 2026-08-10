#!/usr/bin/env python3
"""Bounded retrieval from the House design-system document catalogue."""
from __future__ import annotations

import argparse
import json

from substrate_config import substrate_env

try:
    import psycopg2
    import psycopg2.extras
except ImportError:
    psycopg2 = None


DOCUMENT_TYPES = ("token", "component", "contract", "guideline")


def _timestamp(value):
    return value.isoformat() if hasattr(value, "isoformat") else value


def _row(value) -> dict:
    return {
        "id": int(value["id"]),
        "system": value["system"],
        "doc_type": value["doc_type"],
        "name": value["name"],
        "group_name": value["group_name"],
        "values": value["values"],
        "body": value["body"],
        "provenance": value["provenance"],
        "tags": list(value["tags"] or []),
        "superseded_by": value["superseded_by"],
        "created_at": _timestamp(value["created_at"]),
        "updated_at": _timestamp(value["updated_at"]),
    }


def fetch_design_documents(conn, *, system: str, doc_type: str | None,
                           name: str | None, group: str | None, query: str | None,
                           include_superseded: bool, limit: int) -> dict:
    """Return matching catalogue rows and a document-type taxonomy."""
    if not isinstance(system, str) or not system.strip():
        raise ValueError("system is required")
    if doc_type is not None and doc_type not in DOCUMENT_TYPES:
        raise ValueError(f"doc_type must be one of: {', '.join(DOCUMENT_TYPES)}")
    if not 1 <= limit <= 50:
        raise ValueError("limit must be between 1 and 50")

    clauses = ["system = %s"]
    values: list[object] = [system]
    if doc_type:
        clauses.append("doc_type = %s")
        values.append(doc_type)
    if name:
        clauses.append("name = %s")
        values.append(name)
    if group:
        clauses.append("group_name = %s")
        values.append(group)
    if not include_superseded:
        clauses.append("superseded_by IS NULL")
    if query:
        clauses.append("search_tsv @@ plainto_tsquery('portuguese', %s)")
        values.append(query)
    where = " AND ".join(clauses)
    rank_query = query or ""

    with conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as cur:
        cur.execute(
            f"""
            SELECT id,system,doc_type,name,group_name,"values",body,provenance,tags,
                   superseded_by,created_at,updated_at
            FROM design_documents
            WHERE {where}
            ORDER BY
              CASE WHEN %s <> '' THEN
                ts_rank(search_tsv, plainto_tsquery('portuguese', %s))
              ELSE 0 END DESC,
              updated_at DESC,
              id
            LIMIT %s
            """,
            (*values, rank_query, rank_query, limit),
        )
        rows = [_row(row) for row in cur.fetchall()]

        taxonomy_clauses = ["system = %s"]
        taxonomy_values: list[object] = [system]
        if doc_type:
            taxonomy_clauses.append("doc_type = %s")
            taxonomy_values.append(doc_type)
        if not include_superseded:
            taxonomy_clauses.append("superseded_by IS NULL")
        cur.execute(
            f"""
            SELECT doc_type,COUNT(*) AS count
            FROM design_documents
            WHERE {' AND '.join(taxonomy_clauses)}
            GROUP BY doc_type
            ORDER BY count DESC,doc_type
            """,
            taxonomy_values,
        )
        taxonomy = [
            {"doc_type": row["doc_type"], "count": int(row["count"])}
            for row in cur.fetchall()
        ]

    return {
        "ok": True,
        "system": system,
        "filters": {
            "doc_type": doc_type,
            "name": name,
            "group": group,
            "query": query,
            "include_superseded": include_superseded,
            "limit": limit,
        },
        "documents": rows,
        "taxonomy": taxonomy,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--room-dir", required=True)
    parser.add_argument("--system", required=True)
    parser.add_argument("--doc-type")
    parser.add_argument("--name")
    parser.add_argument("--group")
    parser.add_argument("--query")
    parser.add_argument("--include-superseded", action="store_true")
    parser.add_argument("--limit", type=int, default=12)
    args = parser.parse_args()
    try:
        if psycopg2 is None:
            raise RuntimeError("psycopg2 is required for design document retrieval")
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
            result = fetch_design_documents(
                conn,
                system=args.system.strip(),
                doc_type=args.doc_type,
                name=args.name,
                group=args.group,
                query=args.query,
                include_superseded=args.include_superseded,
                limit=args.limit,
            )
        finally:
            conn.close()
        print(json.dumps(result, ensure_ascii=False))
    except Exception as error:
        print(json.dumps({
            "ok": False,
            "system": args.system,
            "documents": [],
            "taxonomy": [],
            "error": f"{type(error).__name__}: {error}"[:500],
        }, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
