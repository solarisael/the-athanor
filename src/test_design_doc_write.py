import importlib.util
import types
import unittest
from pathlib import Path


spec = importlib.util.spec_from_file_location(
    "design_doc_write", Path(__file__).with_name("design-doc-write.py")
)
design_doc_write = importlib.util.module_from_spec(spec)
spec.loader.exec_module(design_doc_write)
design_doc_write.psycopg2 = types.SimpleNamespace(
    extras=types.SimpleNamespace(RealDictCursor=object())
)


class Cursor:
    def __init__(self, responses, update_rowcount=1):
        self.responses = list(responses)
        self.update_rowcount = update_rowcount
        self.rowcount = 0
        self.calls = []

    def __enter__(self):
        return self

    def __exit__(self, *_):
        return False

    def execute(self, sql, args):
        normalized = " ".join(sql.split())
        self.calls.append((normalized, tuple(args)))
        self.rowcount = self.update_rowcount if normalized.startswith("UPDATE ") else 0
        self.current = self.responses.pop(0) if self.responses else None

    def fetchone(self):
        return self.current


class Connection:
    def __init__(self, responses=(), update_rowcount=1):
        self.cursor_obj = Cursor(responses, update_rowcount)
        self.context_entries = 0

    def __enter__(self):
        self.context_entries += 1
        return self

    def __exit__(self, *_):
        return False

    def cursor(self, **_):
        return self.cursor_obj


def write(conn, **overrides):
    args = {
        "system": "solarisael",
        "doc_type": "token",
        "name": "color.surface.canvas",
        "group_name": "color",
        "values": {"hex": "#101010"},
        "body": "The default canvas color.",
        "provenance": {"repo": "solarisael", "path": "tokens.json"},
        "tags": ["color", "surface"],
        "supersedes": None,
        "allow_identity_change": False,
    }
    args.update(overrides)
    return design_doc_write.write_design_document(conn, **args)


PREVIOUS = {
    "id": 8,
    "system": "solarisael",
    "doc_type": "token",
    "name": "color.surface.canvas",
    "superseded_by": None,
}


class DesignDocumentWriteTests(unittest.TestCase):
    def test_required_arguments_are_refused_before_opening_a_cursor(self):
        for changes, expected in [
            ({"system": ""}, "system is required"),
            ({"name": ""}, "name is required"),
        ]:
            with self.subTest(changes=changes):
                conn = Connection()
                result = write(conn, **changes)
                self.assertFalse(result["ok"])
                self.assertEqual(result["error"], expected)
                self.assertEqual(conn.cursor_obj.calls, [])

    def test_doc_type_is_checked_before_opening_a_cursor(self):
        conn = Connection()
        result = write(conn, doc_type="palette")

        self.assertFalse(result["ok"])
        self.assertEqual(result["error"], "doc_type must be one of: token, component, contract, guideline")
        self.assertEqual(conn.cursor_obj.calls, [])

    def test_supersession_inserts_then_marks_the_previous_row_in_one_transaction(self):
        conn = Connection([PREVIOUS, {"id": 9}])
        result = write(conn, supersedes=8)

        self.assertEqual(result, {
            "ok": True,
            "id": 9,
            "system": "solarisael",
            "doc_type": "token",
            "name": "color.surface.canvas",
            "superseded": [8],
        })
        self.assertEqual(conn.context_entries, 1)
        calls = conn.cursor_obj.calls
        self.assertEqual(len(calls), 3)
        self.assertIn("FOR UPDATE", calls[0][0])
        self.assertIn("INSERT INTO design_documents", calls[1][0])
        self.assertIn('"values"', calls[1][0])
        self.assertEqual(calls[2], (
            "UPDATE design_documents SET superseded_by = %s WHERE id = %s AND superseded_by IS NULL",
            (9, 8),
        ))

    def test_already_superseded_target_is_refused_before_insert(self):
        conn = Connection([{**PREVIOUS, "superseded_by": 9}])
        result = write(conn, supersedes=8)

        self.assertFalse(result["ok"])
        self.assertEqual(result["error"], "superseded document is already superseded")
        self.assertEqual(len(conn.cursor_obj.calls), 1)

    def test_identity_change_requires_an_explicit_flag(self):
        conn = Connection([{**PREVIOUS, "name": "color.surface.page"}])
        result = write(conn, supersedes=8)

        self.assertFalse(result["ok"])
        self.assertEqual(
            result["error"],
            "superseded document identity differs; pass --allow-identity-change",
        )
        self.assertEqual(len(conn.cursor_obj.calls), 1)


if __name__ == "__main__":
    unittest.main()
