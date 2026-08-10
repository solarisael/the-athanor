#!/usr/bin/env python3
"""Append a design-system document, optionally superseding one current row."""
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


DOCUMENT_TYPES = ("token", "component", "contract", "guideline")


class _WriteRefusal(Exception):
    pass


def _refusal(system, doc_type, name, error) -> dict:
    return {
        "ok": False,
        "system": system,
        "doc_type": doc_type,
        "name": name,
        "superseded": [],
        "error": error,
    }


def _row_value(row, key, position):
    return row[key] if isinstance(row, dict) else row[position]


def _validate(system, doc_type, name, group_name, values, body, provenance,
              tags, supersedes, allow_identity_change) -> str | None:
    if not isinstance(system, str) or not system.strip():
        return "system is required"
    if doc_type not in DOCUMENT_TYPES:
        return f"doc_type must be one of: {', '.join(DOCUMENT_TYPES)}"
    if not isinstance(name, str) or not name.strip():
        return "name is required"
    if group_name is not None and not isinstance(group_name, str):
        return "group_name must be a string or null"
    if not isinstance(values, dict):
        return "values must be a JSON object"
    if not isinstance(body, str):
        return "body must be a string"
    if not isinstance(provenance, dict):
        return "provenance must be a JSON object"
    if not isinstance(tags, list) or any(not isinstance(tag, str) for tag in tags):
        return "tags must be an array of strings"
    if supersedes is not None and (
        isinstance(supersedes, bool) or not isinstance(supersedes, int) or supersedes <= 0
    ):
        return "supersedes must be a positive integer"
    if not isinstance(allow_identity_change, bool):
        return "allow_identity_change must be a boolean"
    return None


def write_design_document(conn, *, system: str, doc_type: str, name: str,
                          group_name: str | None, values: dict, body: str,
                          provenance: dict, tags: list[str], supersedes: int | None,
                          allow_identity_change: bool = False) -> dict:
    """Insert an immutable catalogue revision and atomically mark its predecessor."""
    error = _validate(
        system, doc_type, name, group_name, values, body, provenance, tags,
        supersedes, allow_identity_change,
    )
    if error:
        return _refusal(system, doc_type, name, error)

    system = system.strip()
    name = name.strip()
    try:
        # The connection context commits on normal exit and rolls back on errors.
        with conn:
            with conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as cur:
                if supersedes is not None:
                    cur.execute(
                        """
                        SELECT id,system,doc_type,name,superseded_by
                        FROM design_documents
                        WHERE id = %s
                        FOR UPDATE
                        """,
                        (supersedes,),
                    )
                    previous = cur.fetchone()
                    if previous is None:
                        raise _WriteRefusal("superseded document not found")
                    if _row_value(previous, "superseded_by", 4) is not None:
                        raise _WriteRefusal("superseded document is already superseded")
                    same_identity = (
                        _row_value(previous, "system", 1) == system
                        and _row_value(previous, "doc_type", 2) == doc_type
                        and _row_value(previous, "name", 3) == name
                    )
                    if not same_identity and not allow_identity_change:
                        raise _WriteRefusal(
                            "superseded document identity differs; pass --allow-identity-change"
                        )

                cur.execute(
                    """
                    INSERT INTO design_documents
                      (system,doc_type,name,group_name,"values",body,provenance,tags)
                    VALUES (%s,%s,%s,%s,%s::jsonb,%s,%s::jsonb,%s)
                    RETURNING id
                    """,
                    (
                        system,
                        doc_type,
                        name,
                        group_name,
                        json.dumps(values, ensure_ascii=False),
                        body,
                        json.dumps(provenance, ensure_ascii=False),
                        tags,
                    ),
                )
                inserted = cur.fetchone()
                if inserted is None:
                    raise _WriteRefusal("insert did not return an id")
                document_id = int(_row_value(inserted, "id", 0))

                if supersedes is not None:
                    cur.execute(
                        """
                        UPDATE design_documents
                        SET superseded_by = %s
                        WHERE id = %s AND superseded_by IS NULL
                        """,
                        (document_id, supersedes),
                    )
                    if cur.rowcount != 1:
                        raise _WriteRefusal("supersession affected an unexpected number of rows")
    except _WriteRefusal as error:
        return _refusal(system, doc_type, name, str(error))

    return {
        "ok": True,
        "id": document_id,
        "system": system,
        "doc_type": doc_type,
        "name": name,
        "superseded": [supersedes] if supersedes is not None else [],
    }


def _json_object(text: str, option: str) -> dict:
    try:
        value = json.loads(text)
    except json.JSONDecodeError as error:
        raise ValueError(f"{option} must be valid JSON: {error.msg}") from error
    if not isinstance(value, dict):
        raise ValueError(f"{option} must be a JSON object")
    return value


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--room-dir", required=True)
    parser.add_argument("--system", required=True)
    parser.add_argument("--doc-type", required=True)
    parser.add_argument("--name", required=True)
    parser.add_argument("--group", dest="group_name")
    parser.add_argument("--values", default="{}")
    parser.add_argument("--provenance", default="{}")
    parser.add_argument("--tag", dest="tags", action="append", default=[])
    parser.add_argument("--supersedes", type=int)
    parser.add_argument("--allow-identity-change", action="store_true")
    body = parser.add_mutually_exclusive_group(required=True)
    body.add_argument("--body")
    body.add_argument("--body-stdin", action="store_true")
    args = parser.parse_args()
    try:
        if psycopg2 is None:
            raise RuntimeError("psycopg2 is required for design document writes")
        values = _json_object(args.values, "values")
        provenance = _json_object(args.provenance, "provenance")
        document_body = sys.stdin.read() if args.body_stdin else args.body
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
            result = write_design_document(
                conn,
                system=args.system,
                doc_type=args.doc_type,
                name=args.name,
                group_name=args.group_name,
                values=values,
                body=document_body,
                provenance=provenance,
                tags=args.tags,
                supersedes=args.supersedes,
                allow_identity_change=args.allow_identity_change,
            )
        finally:
            conn.close()
        print(json.dumps(result, ensure_ascii=False))
    except Exception as error:
        print(json.dumps(
            _refusal(args.system, args.doc_type, args.name, str(error)),
            ensure_ascii=False,
        ))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
