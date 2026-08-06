import importlib.util
import types
import unittest
from pathlib import Path


spec = importlib.util.spec_from_file_location("lessons", Path(__file__).with_name("lessons.py"))
lessons = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lessons)
lessons.psycopg2 = types.SimpleNamespace(extras=types.SimpleNamespace(RealDictCursor=object()))


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


LESSON = {
    "id": 27,
    "lesson_key": "coding",
    "kind_path": "coding/process",
    "scope": "house",
    "project": None,
    "voice": "kintsu",
    "register": [],
    "shape": "process",
    "stage": [],
    "title": "First draft loud",
    "lesson": "Tell the truth before compressing it.",
    "trigger_context": None,
    "proof_pattern": "The seam remains visible.",
    "example_text": None,
    "example_cmd": None,
    "writers": [],
    "tools": [],
    "negation_of": None,
    "tags": ["process"],
    "always_on": False,
}
TAXONOMY = {"kind_path": "coding/process", "shape": "process", "count": 1, "always_on_count": 0}


class CanonicalLessonsTests(unittest.TestCase):
    def test_coding_query_is_typed_room_scoped_and_returns_kind_path(self):
        conn = Connection([[LESSON], [TAXONOMY]])
        result = lessons.fetch_lessons(
            conn,
            lesson_type="coding",
            room="kintsu",
            shape="process",
            project=None,
            register=None,
            stage=None,
            query=None,
            limit=12,
        )

        self.assertTrue(result["ok"])
        self.assertEqual(result["type"], "coding")
        self.assertEqual(result["lessons"][0]["kindPath"], "coding/process")
        query, args = conn.cursor_obj.calls[0]
        self.assertIn("FROM lessons", query)
        self.assertIn("lesson_key = %s", query)
        self.assertIn("scope = ANY(%s)", query)
        self.assertEqual(args[:3], ("coding", ["house", "kintsu"], "process"))

    def test_project_type_requires_an_explicit_project(self):
        with self.assertRaisesRegex(ValueError, "require --project"):
            lessons.fetch_lessons(
                Connection([]),
                lesson_type="project",
                room="kintsu",
                shape=None,
                project=None,
                register=None,
                stage=None,
                query=None,
                limit=12,
            )

    def test_audio_stage_query_uses_the_same_typed_table(self):
        audio = {**LESSON, "lesson_key": "audio", "kind_path": "audio/mixing", "scope": "house", "shape": "mixing", "stage": ["mix"]}
        taxonomy = {"kind_path": "audio/mixing", "shape": "mixing", "count": 1, "always_on_count": 0}
        conn = Connection([[audio], [taxonomy]])
        lessons.fetch_lessons(
            conn,
            lesson_type="audio",
            room="kintsu",
            shape=None,
            project=None,
            register=None,
            stage="mix",
            query="headroom",
            limit=5,
        )

        query, args = conn.cursor_obj.calls[0]
        self.assertIn("FROM lessons", query)
        self.assertIn("%s = ANY(stage)", query)
        self.assertEqual(args[0:3], ("audio", "mix", "headroom"))


    def test_design_query_is_typed_uses_register_and_portuguese_fts(self):
        design = {
            **LESSON,
            "lesson_key": "design",
            "kind_path": "design/component-contract",
            "voice": "solarisael",
            "register": ["general"],
            "shape": "component-contract",
        }
        taxonomy = {
            "kind_path": "design/component-contract",
            "shape": "component-contract",
            "count": 1,
            "always_on_count": 0,
        }
        conn = Connection([[design], [taxonomy]])
        result = lessons.fetch_lessons(
            conn,
            lesson_type="design",
            room="kintsu",
            shape="component-contract",
            project=None,
            register="general",
            stage=None,
            query="navegação",
            limit=5,
        )

        self.assertEqual(result["type"], "design")
        self.assertEqual(result["lessons"][0]["kindPath"], "design/component-contract")
        query, args = conn.cursor_obj.calls[0]
        self.assertIn("lesson_key = %s", query)
        self.assertIn("%s = ANY(register)", query)
        self.assertIn("ELSE 'portuguese'::regconfig", query)
        self.assertNotIn("scope = ANY(%s)", query)
        self.assertEqual(args[:4], ("design", "component-contract", "general", "navegação"))

if __name__ == "__main__":
    unittest.main()
