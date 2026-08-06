import importlib.util
import unittest
from pathlib import Path


spec = importlib.util.spec_from_file_location("delete_lesson", Path(__file__).with_name("delete-lesson.py"))
delete_lesson = importlib.util.module_from_spec(spec)
spec.loader.exec_module(delete_lesson)


class Cursor:
    def __init__(self, row=("Current title",), delete_rowcount=1):
        self.row = row
        self.delete_rowcount = delete_rowcount
        self.rowcount = 0
        self.calls = []

    def __enter__(self):
        return self

    def __exit__(self, *_):
        return False

    def execute(self, sql, args):
        self.calls.append((sql, args))
        self.rowcount = self.delete_rowcount if sql.startswith("DELETE ") else 0

    def fetchone(self):
        return self.row


class Connection:
    def __init__(self, row=("Current title",), delete_rowcount=1):
        self.cursor_obj = Cursor(row, delete_rowcount)

    def __enter__(self):
        return self

    def __exit__(self, *_):
        return False

    def cursor(self, **_):
        return self.cursor_obj


class DeleteLessonTests(unittest.TestCase):
    def test_deletes_exact_writing_lesson_row(self):
        conn = Connection()
        result = delete_lesson.delete_lesson(
            conn,
            "writing-lesson",
            22,
            "Current title",
        )

        self.assertEqual(result, {
            "ok": True,
            "kind": "writing-lesson",
            "id": 22,
            "title": "Current title",
            "deleted": True,
        })
        self.assertEqual(conn.cursor_obj.calls, [
            (
                "SELECT title FROM lessons WHERE lesson_key = %s AND id = %s FOR UPDATE",
                ("writing", 22),
            ),
            (
                "DELETE FROM lessons WHERE lesson_key = %s AND id = %s AND title = %s",
                ("writing", 22, "Current title"),
            ),
        ])

    def test_title_mismatch_refuses_before_delete(self):
        conn = Connection(row=("Different title",))
        result = delete_lesson.delete_lesson(
            conn,
            "writing-lesson",
            22,
            "Current title",
        )

        self.assertFalse(result["ok"])
        self.assertEqual(result["error"], "title mismatch")
        self.assertEqual(len(conn.cursor_obj.calls), 1)

    def test_missing_row_refuses_before_delete(self):
        conn = Connection(row=None)
        result = delete_lesson.delete_lesson(
            conn,
            "writing-lesson",
            22,
            "Current title",
        )

        self.assertFalse(result["ok"])
        self.assertEqual(result["error"], "lesson not found")
        self.assertEqual(len(conn.cursor_obj.calls), 1)

    def test_invalid_requests_never_open_a_cursor(self):
        cases = [
            ("unknown", 1, "Title"),
            ("writing-lesson", 0, "Title"),
            ("writing-lesson", 1, ""),
        ]
        for args in cases:
            with self.subTest(args=args):
                conn = Connection()
                result = delete_lesson.delete_lesson(conn, *args)
                self.assertFalse(result["ok"])
                self.assertEqual(conn.cursor_obj.calls, [])

    def test_unexpected_rowcount_refuses(self):
        conn = Connection(delete_rowcount=0)
        result = delete_lesson.delete_lesson(
            conn,
            "writing-lesson",
            22,
            "Current title",
        )

        self.assertFalse(result["ok"])
        self.assertEqual(result["error"], "delete affected an unexpected number of rows")


if __name__ == "__main__":
    unittest.main()
