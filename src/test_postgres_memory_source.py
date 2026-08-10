import importlib.util
import tempfile
import unittest
from pathlib import Path


MODULE = Path(__file__).with_name("postgres-memory-source.py")
spec = importlib.util.spec_from_file_location("postgres_memory_source", MODULE)
source = importlib.util.module_from_spec(spec)
spec.loader.exec_module(source)


class PostgresMemorySourceRoomTests(unittest.TestCase):
    def test_custom_room_is_preserved(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            room_dir = Path(temp_dir) / "custom-room"
            room_dir.mkdir()
            self.assertEqual(source.resolve_room_name("custom-room", room_dir), "custom-room")

    def test_invalid_room_is_rejected_instead_of_mapped(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            room_dir = Path(temp_dir) / "custom-room"
            room_dir.mkdir()
            for invalid in ("Custom-Room", "custom_room", "custom room", "../other-room"):
                with self.subTest(invalid=invalid):
                    with self.assertRaisesRegex(ValueError, "invalid room key"):
                        source.resolve_room_name(invalid, room_dir)

    def test_reserved_house_room_is_rejected(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            room_dir = Path(temp_dir) / "custom-room"
            room_dir.mkdir()
            with self.assertRaisesRegex(ValueError, "invalid room key"):
                source.resolve_room_name("house", room_dir)


class RetrievalCursor:
    def __init__(self, coding_rows=None, project_rows=None):
        self.calls = []
        self.coding_rows = coding_rows or []
        self.project_rows = project_rows or []

    def __enter__(self):
        return self

    def __exit__(self, *_):
        return False

    def execute(self, sql, args):
        self.calls.append((sql, args))

    def fetchall(self):
        if len(self.calls) <= 3:
            return []
        if len(self.calls) == 4:
            return self.coding_rows
        return self.project_rows


class RetrievalConn:
    def __init__(self, coding_rows=None, project_rows=None):
        self.cursor_obj = RetrievalCursor(coding_rows, project_rows)

    def cursor(self, **_):
        return self.cursor_obj


class ResonanceCursor:
    def __init__(self):
        self.calls = []

    def __enter__(self):
        return self

    def __exit__(self, *_):
        return False

    def execute(self, sql, args):
        self.calls.append((sql, args))

    def fetchall(self):
        if len(self.calls) == 1:
            return [{"id": 7, "label": "active cluster", "member_count": 1, "activation": 0.9}]
        return [{"source_path": "active.md", "heading_path": "Active", "sim": 0.8}]


class ResonanceConn:
    def __init__(self):
        self.cursor_obj = ResonanceCursor()

    def cursor(self, **_):
        return self.cursor_obj


class PostgresMemorySourceIsolationTests(unittest.TestCase):
    def test_candidate_search_scopes_coding_lessons_and_omits_projects_without_explicit_contract(self):
        coding = [{
            "id": 9,
            "scope": "other-room",
            "project": "other-project",
            "title": "foreign lesson",
            "lesson": "deploy elsewhere",
            "shape": "deploy",
            "tags": [],
            "raw_rank": 2.0,
            "title_hit": True,
        }, {
            "id": 1,
            "scope": "active-room",
            "project": "other-project",
            "title": "active lesson",
            "lesson": "deploy safely",
            "shape": "deploy",
            "tags": [],
            "raw_rank": 1.0,
            "title_hit": False,
        }]
        conn = RetrievalConn(coding_rows=coding)
        _, candidates = source.load_search_candidates(
            conn,
            rooms=("active-room", "house"),
            lesson_scopes=("house", "active-room"),
            query="deploy",
            top_k=12,
        )
        self.assertEqual([candidate["source_id"] for candidate in candidates], [1])
        coding_sql, coding_args = conn.cursor_obj.calls[3]
        self.assertIn("scope = ANY(%s)", coding_sql)
        self.assertEqual(coding_args[2], ["active-room", "house"])
        self.assertFalse(any("project_lessons" in sql for sql, _ in conn.cursor_obj.calls))

    def test_cluster_resonance_scopes_profile_and_hot_chunks_to_active_and_shared_rooms(self):
        conn = ResonanceConn()
        result = source.load_cluster_resonance(
            conn,
            query_vec=[0.1, 0.2],
            rooms=("active-room", "house"),
        )
        self.assertEqual(result["hot"][0]["chunks"][0]["source_path"], "active.md")
        profile_sql, profile_args = conn.cursor_obj.calls[0]
        hot_sql, hot_args = conn.cursor_obj.calls[1]
        self.assertIn("m.room = ANY(%s)", profile_sql)
        self.assertIn("m.room = ANY(%s)", hot_sql)
        self.assertIn("COALESCE(m.type, '') <> 'paper-boat'", profile_sql)
        self.assertIn("COALESCE(m.type, '') <> 'paper-boat'", hot_sql)
        self.assertIn("mc.label NOT ILIKE 'paper boat%%'", profile_sql)
        self.assertEqual(profile_args[1], ["active-room", "house"])
        self.assertEqual(hot_args[2], ["active-room", "house"])

if __name__ == "__main__":
    unittest.main()
