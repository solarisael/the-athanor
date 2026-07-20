import importlib.util
import unittest
from pathlib import Path


spec = importlib.util.spec_from_file_location("update_lesson", Path(__file__).with_name("update-lesson.py"))
update_lesson = importlib.util.module_from_spec(spec)
spec.loader.exec_module(update_lesson)


class Cursor:
    def __init__(self, row=("Current title",), update_rowcount=1):
        self.row = row
        self.update_rowcount = update_rowcount
        self.rowcount = 0
        self.calls = []

    def __enter__(self):
        return self

    def __exit__(self, *_):
        return False

    def execute(self, sql, args):
        self.calls.append((sql, args))
        self.rowcount = self.update_rowcount if sql.startswith("UPDATE ") else 0

    def fetchone(self):
        return self.row


class Connection:
    def __init__(self, row=("Current title",), update_rowcount=1):
        self.cursor_obj = Cursor(row, update_rowcount)
        self.context_entries = 0

    def __enter__(self):
        self.context_entries += 1
        return self

    def __exit__(self, *_):
        return False

    def cursor(self, **_):
        return self.cursor_obj


class UpdateLessonTests(unittest.TestCase):
    def test_updates_allowlisted_fields_without_changing_id(self):
        conn = Connection()
        result = update_lesson.update_lesson(
            conn,
            "coding-lesson",
            137,
            "Current title",
            {"lesson": "Revised lesson", "tags": ["tooling"], "negation_of": None},
        )

        self.assertEqual(result, {
            "ok": True,
            "kind": "coding-lesson",
            "id": 137,
            "title": "Current title",
            "updated": True,
        })
        self.assertEqual(conn.cursor_obj.calls[0], (
            "SELECT title FROM coding_lessons WHERE id = %s FOR UPDATE",
            (137,),
        ))
        update_sql, update_args = conn.cursor_obj.calls[1]
        self.assertEqual(
            update_sql,
            "UPDATE coding_lessons SET lesson = %s, tags = %s, negation_of = %s WHERE id = %s",
        )
        self.assertEqual(update_args, ["Revised lesson", ["tooling"], None, 137])

    def test_omitted_negation_link_is_preserved(self):
        conn = Connection()
        update_lesson.update_lesson(
            conn,
            "coding-lesson",
            137,
            "Current title",
            {"lesson": "Revised lesson"},
        )

        update_sql, update_args = conn.cursor_obj.calls[1]
        self.assertNotIn("negation_of", update_sql)
        self.assertEqual(update_args, ["Revised lesson", 137])

    def test_title_mismatch_refuses_before_update(self):
        conn = Connection(row=("Different title",))
        result = update_lesson.update_lesson(
            conn,
            "coding-lesson",
            137,
            "Current title",
            {"lesson": "Revised lesson"},
        )

        self.assertFalse(result["ok"])
        self.assertEqual(result["error"], "title mismatch")
        self.assertEqual(len(conn.cursor_obj.calls), 1)

    def test_missing_row_refuses_before_update(self):
        conn = Connection(row=None)
        result = update_lesson.update_lesson(
            conn,
            "project-lesson",
            7,
            "Current title",
            {"lesson": "Revised lesson"},
        )

        self.assertFalse(result["ok"])
        self.assertEqual(result["error"], "lesson not found")
        self.assertEqual(len(conn.cursor_obj.calls), 1)

    def test_invalid_and_empty_requests_never_open_a_cursor(self):
        cases = [
            ("unknown", 1, "Title", {"lesson": "x"}),
            ("coding-lesson", 0, "Title", {"lesson": "x"}),
            ("coding-lesson", 1, "", {"lesson": "x"}),
            ("coding-lesson", 1, "Title", {}),
            ("project-lesson", 1, "Title", {"voice": "shared"}),
            ("coding-lesson", 1, "Title", {"negation_of": 0}),
        ]
        for args in cases:
            with self.subTest(args=args):
                conn = Connection()
                result = update_lesson.update_lesson(conn, *args)
                self.assertFalse(result["ok"])
                self.assertEqual(conn.cursor_obj.calls, [])

    def test_project_update_uses_project_table(self):
        conn = Connection()
        result = update_lesson.update_lesson(
            conn,
            "project-lesson",
            7,
            "Current title",
            {"project": "solarisael", "proof_pattern": "Focused proof"},
        )

        self.assertTrue(result["ok"])
        self.assertIn("UPDATE project_lessons", conn.cursor_obj.calls[1][0])

    def test_unexpected_rowcount_refuses(self):
        conn = Connection(update_rowcount=0)
        result = update_lesson.update_lesson(
            conn,
            "coding-lesson",
            137,
            "Current title",
            {"lesson": "Revised lesson"},
        )

        self.assertFalse(result["ok"])
        self.assertEqual(result["error"], "update affected an unexpected number of rows")


if __name__ == "__main__":
    unittest.main()
