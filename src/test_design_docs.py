import importlib.util
import types
import unittest
from datetime import datetime, timezone
from pathlib import Path


spec = importlib.util.spec_from_file_location("design_docs", Path(__file__).with_name("design-docs.py"))
design_docs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(design_docs)
design_docs.psycopg2 = types.SimpleNamespace(extras=types.SimpleNamespace(RealDictCursor=object()))


class Cursor:
    def __init__(self, responses):
        self.responses = list(responses)
        self.calls = []
        self.current = []

    def __enter__(self):
        return self

    def __exit__(self, *_):
        return False

    def execute(self, sql, args):
        self.calls.append((" ".join(sql.split()), tuple(args)))
        self.current = self.responses.pop(0)

    def fetchall(self):
        return self.current


class Connection:
    def __init__(self, responses):
        self.cursor_obj = Cursor(responses)

    def cursor(self, **_):
        return self.cursor_obj


DOCUMENT = {
    "id": 42,
    "system": "solarisael",
    "doc_type": "token",
    "name": "color.surface.canvas",
    "group_name": "color",
    "values": {"hex": "#101010"},
    "body": "The default canvas color.",
    "provenance": {"repo": "solarisael"},
    "tags": ["color", "surface"],
    "superseded_by": None,
    "created_at": datetime(2026, 8, 6, tzinfo=timezone.utc),
    "updated_at": datetime(2026, 8, 6, 1, tzinfo=timezone.utc),
}
TAXONOMY = {"doc_type": "token", "count": 1}


class DesignDocumentQueryTests(unittest.TestCase):
    def test_required_system_is_refused_before_opening_a_cursor(self):
        with self.assertRaisesRegex(ValueError, "system is required"):
            design_docs.fetch_design_documents(
                Connection([]),
                system="",
                doc_type=None,
                name=None,
                group=None,
                query=None,
                include_superseded=False,
                limit=12,
            )

    def test_doc_type_is_checked_before_opening_a_cursor(self):
        with self.assertRaisesRegex(ValueError, "doc_type must be one of"):
            design_docs.fetch_design_documents(
                Connection([]),
                system="solarisael",
                doc_type="palette",
                name=None,
                group=None,
                query=None,
                include_superseded=False,
                limit=12,
            )

    def test_fts_query_filters_current_rows_and_returns_all_document_fields(self):
        conn = Connection([[DOCUMENT], [TAXONOMY]])
        result = design_docs.fetch_design_documents(
            conn,
            system="solarisael",
            doc_type="token",
            name=None,
            group="color",
            query="canvas",
            include_superseded=False,
            limit=5,
        )

        self.assertTrue(result["ok"])
        self.assertEqual(result["documents"][0], {
            "id": 42,
            "system": "solarisael",
            "doc_type": "token",
            "name": "color.surface.canvas",
            "group_name": "color",
            "values": {"hex": "#101010"},
            "body": "The default canvas color.",
            "provenance": {"repo": "solarisael"},
            "tags": ["color", "surface"],
            "superseded_by": None,
            "created_at": "2026-08-06T00:00:00+00:00",
            "updated_at": "2026-08-06T01:00:00+00:00",
        })
        self.assertEqual(result["taxonomy"], [{"doc_type": "token", "count": 1}])
        sql, args = conn.cursor_obj.calls[0]
        self.assertIn("FROM design_documents", sql)
        self.assertIn("doc_type = %s", sql)
        self.assertIn("group_name = %s", sql)
        self.assertIn("superseded_by IS NULL", sql)
        self.assertIn("search_tsv @@ plainto_tsquery('portuguese', %s)", sql)
        self.assertEqual(args[:4], ("solarisael", "token", "color", "canvas"))


if __name__ == "__main__":
    unittest.main()
