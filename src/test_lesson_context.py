import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("lesson_context", Path(__file__).with_name("lesson-context.py"))
lesson_context = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lesson_context)


class Cursor:
    def __init__(self, rows): self.rows, self.calls = rows, []
    def execute(self, sql, args): self.calls.append((sql, args))
    def fetchall(self): return self.rows.pop(0)
    def close(self): pass


class Conn:
    def __init__(self, coding, project): self.cursor_obj = Cursor([coding, project])
    def cursor(self, **_): return self.cursor_obj


def row(i, scope="shared", project="", shape="process", tags=None, trigger=""):
    return {"id": i, "title": f"lesson {i}", "lesson": "text", "proof_pattern": "proof",
            "trigger_context": trigger, "scope": scope, "project": project, "voice": "kodo",
            "shape": shape, "tags": tags or []}


class LessonContextTests(unittest.TestCase):
    def test_ranking_precedence_and_scope(self):
        conn = Conn([row(2, trigger="deploy"), row(1, tags=["deploy"]), row(3, shape="deploy")], [])
        result = lesson_context.retrieve_lesson_context(conn, "kintsu", shapes=["deploy"], terms=["deploy"], limit=3)
        self.assertEqual([x["id"] for x in result["codingLessons"]], [2, 1, 3])
        self.assertEqual(result["match"]["scopes"], ["shared", "kintsu"])
        self.assertEqual(conn.cursor_obj.calls[0][1], (["shared", "kintsu"],))

    def test_exact_projects_and_bounded_deterministic_order(self):
        conn = Conn([row(9, project="other"), row(2, project="app"), row(1, project="app")], [row(4, project="app"), row(3, project="app2")])
        result = lesson_context.retrieve_lesson_context(conn, "shared", projects=["app"], limit=1)
        self.assertEqual([x["id"] for x in result["projectLessons"]], [4])
        self.assertEqual([x["id"] for x in result["codingLessons"]], [1])
        self.assertEqual(conn.cursor_obj.calls[1][1], (["app"],))

    def test_unavailable_substrate_fails_open(self):
        import io
        from contextlib import redirect_stdout
        from unittest.mock import patch
        import sys
        argv = ["lesson-context.py", "--room", "room", "--room-dir", "."]
        with patch.object(sys, "argv", argv), patch.object(lesson_context, "psycopg2", None):
            output = io.StringIO()
            with redirect_stdout(output):
                self.assertEqual(lesson_context.main(), 0)
        self.assertEqual(lesson_context.json.loads(output.getvalue())["codingLessons"], [])

if __name__ == "__main__": unittest.main()
