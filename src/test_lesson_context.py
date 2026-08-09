import importlib.util
import unittest
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))

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


def row(i, scope="house", project="", shape="process", tags=None, trigger="", lesson_key="coding",
        stage=None, register="", language_keys=None, technology_keys=None):
    return {"id": i, "lesson_key": lesson_key, "title": f"lesson {i}", "lesson": "text", "proof_pattern": "proof",
            "trigger_context": trigger, "scope": scope, "project": project, "voice": "generic",
            "register": register, "shape": shape, "stage": stage or [], "tags": tags or [],
            "language_keys": language_keys or [], "technology_keys": technology_keys or []}


class LessonContextTests(unittest.TestCase):
    def test_ranking_precedence_and_scope(self):
        conn = Conn([row(9, scope="other-room"), row(2, trigger="deploy"), row(1, tags=["deploy"]), row(3, shape="deploy")], [])
        result = lesson_context.retrieve_lesson_context(conn, "sample-room", shapes=["deploy"], terms=["deploy"], limit=3)
        self.assertEqual([x["id"] for x in result["codingLessons"]], [2, 1, 3])
        self.assertEqual(result["match"]["scopes"], ["house", "sample-room"])
        self.assertEqual(conn.cursor_obj.calls[0][1], (["house", "sample-room"],))

        conn = Conn([row(9, project="other"), row(2, project="app"), row(1, project="app")], [row(4, project="app", lesson_key="project"), row(3, project="app2", lesson_key="project")])
        result = lesson_context.retrieve_lesson_context(conn, "house", projects=["app"], limit=1)
        self.assertEqual([x["id"] for x in result["projectLessons"]], [4])
        self.assertEqual([x["id"] for x in result["codingLessons"]], [1])
        self.assertEqual(conn.cursor_obj.calls[1][1], (["app"],))

    def test_project_lessons_are_filtered_to_explicit_project_contract(self):
        conn = Conn([row(1)], [row(4, project="app", lesson_key="project"), row(5, project="other", lesson_key="project")])
        result = lesson_context.retrieve_lesson_context(conn, "sample-room", projects=["app"], limit=10)
        self.assertEqual([x["id"] for x in result["projectLessons"]], [4])


    def test_authority_rails_filter_before_ranking(self):
        conn = Conn([
            row(1, project="other", trigger="deploy"),
            row(2, stage=["release"], trigger="deploy"),
            row(3, register="ops", trigger="deploy"),
            row(4, trigger="deploy"),
        ], [row(5, project="app", lesson_key="project"), row(6, project="other", lesson_key="project")])
        result = lesson_context.retrieve_lesson_context(
            conn, "room", projects=["app"], terms=["deploy"],
            stages=["release"], registers=["ops"], limit=10,
        )
        self.assertEqual([item["id"] for item in result["codingLessons"]], [2, 3, 4])
        self.assertEqual([item["id"] for item in result["projectLessons"]], [5])
    def test_eligibility_keys_are_silent_for_wrong_context(self):
        conn = Conn([
            row(1),
            row(2, language_keys=["rust"]),
            row(3, language_keys=["python"]),
            row(4, technology_keys=["postgresql"]),
            row(5, technology_keys=["godot"]),
        ], [])
        result = lesson_context.retrieve_lesson_context(
            conn, "room", languages=["rust"], technologies=["postgresql"], limit=10,
        )
        self.assertEqual([item["id"] for item in result["codingLessons"]], [1, 2, 4])
        self.assertEqual(result["match"]["languages"], ["rust"])
        self.assertEqual(result["match"]["technologies"], ["postgresql"])

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
        payload = lesson_context.json.loads(output.getvalue())
        self.assertEqual(payload["codingLessons"], [])
        # Fail-open must stay loud. A silent empty context is how a broken
        # SELECT stayed invisible for weeks: every turn injected zero lessons
        # and exited 0. The exit code stays 0; the reason has to come with it.
        self.assertIn("psycopg2 unavailable", payload["error"])

if __name__ == "__main__": unittest.main()
