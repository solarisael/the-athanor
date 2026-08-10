from __future__ import annotations

import re
import unittest
from pathlib import Path

import query_audio_lessons
import query_project_lessons


ROOT = Path(__file__).resolve().parents[1]
MIGRATION = ROOT / "migrations" / "0008_unified_lessons.sql"
DESIGN_MIGRATION = ROOT / "migrations" / "0011_design_lessons.sql"
DESIGN_RECORDER = ROOT / "record_design_lesson.py"


class RecordingCursor:
    def __init__(self, rows: list[dict] | None = None) -> None:
        self.rows = rows or []
        self.calls: list[tuple[str, object]] = []

    def execute(self, sql: str, params=None) -> None:
        self.calls.append((sql, params))

    def fetchall(self):
        return self.rows


class UnifiedLessonQueryTests(unittest.TestCase):
    def test_project_query_paths_use_the_unified_table_and_project_discriminator(self) -> None:
        rows = [{"id": 7, "project": "athanor", "title": "Keep", "lesson": "Typed"}]
        cursor = RecordingCursor(rows)

        self.assertIs(query_project_lessons.fetch_rows(cursor, "athanor", None, 4), rows)
        self.assertIs(query_project_lessons.fetch_rows(cursor, "athanor", "release", 5), rows)
        self.assertEqual(len(cursor.calls), 2)
        for sql, _ in cursor.calls:
            normalized = " ".join(sql.lower().split())
            self.assertIn("from lessons", normalized)
            self.assertIn("lesson_key = 'project'", normalized)
            self.assertNotIn("project_lessons", normalized)
        self.assertEqual(cursor.calls[0][1], ("athanor", 4))
        self.assertEqual(cursor.calls[1][1], ("release", "release", "athanor", 5))

    def test_audio_query_and_partner_lookup_cannot_cross_lesson_types(self) -> None:
        rows = [{"id": 11, "shape": "spine", "title": "Listen", "lesson": "First"}]
        cursor = RecordingCursor(rows)

        self.assertIs(
            query_audio_lessons.fetch_rows(
                cursor,
                query="noise",
                stages=["denoise"],
                shape="spine",
                limit=3,
                show_all=False,
            ),
            rows,
        )
        self.assertEqual(query_audio_lessons.fetch_by_ids(cursor, [11]), {11: rows[0]})
        self.assertEqual(len(cursor.calls), 2)
        for sql, _ in cursor.calls:
            normalized = " ".join(sql.lower().split())
            self.assertIn("from lessons", normalized)
            self.assertIn("lesson_key = 'audio'", normalized)
            self.assertNotIn("audio_lessons", normalized)
        self.assertEqual(cursor.calls[1][1], ([11],))


class UnifiedLessonMigrationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.sql = MIGRATION.read_text(encoding="utf-8")
        cls.normalized = " ".join(cls.sql.split())

    def test_kind_path_is_a_stored_key_and_normalized_shape_path(self) -> None:
        self.assertRegex(
            self.normalized,
            re.compile(
                r"kind_path TEXT GENERATED ALWAYS AS \( lesson_key \|\| '/' \|\| COALESCE\( "
                r"NULLIF\(BTRIM\(REGEXP_REPLACE\(LOWER\(COALESCE\(shape, ''\)\), "
                r"'\[\^a-z0-9\]\+', '-', 'g'\), '-'\), ''\), 'general' \) \) STORED",
                re.IGNORECASE,
            ),
        )
        self.assertIn("CREATE INDEX lessons_kind_path_idx ON lessons (kind_path, updated_at DESC);", self.normalized)
        self.assertIn("PRIMARY KEY (lesson_key, id)", self.normalized)
        self.assertIn("FOREIGN KEY (lesson_key, negation_of)", self.normalized)
        self.assertIn("REFERENCES lessons(lesson_key, id)", self.normalized)

    def test_each_legacy_type_is_copied_with_its_discriminator_before_drop(self) -> None:
        guard_position = self.sql.index("DO $migration_guard$")
        for lesson_key, legacy_table in {
            "coding": "coding_lessons",
            "project": "project_lessons",
            "writing": "writing_lessons",
            "audio": "audio_lessons",
        }.items():
            copy = re.search(
                rf"INSERT INTO lessons \([^;]*?SELECT\s+'{lesson_key}'[^;]*?FROM {legacy_table};",
                self.sql,
                re.DOTALL,
            )
            self.assertIsNotNone(copy, f"missing typed copy from {legacy_table}")
            assert copy is not None
            self.assertLess(copy.start(), guard_position)
            self.assertGreater(self.sql.index(f"DROP TABLE {legacy_table};"), guard_position)

    def test_count_preservation_guard_runs_before_destructive_cleanup(self) -> None:
        guard_position = self.sql.index("DO $migration_guard$")
        guard_end = self.sql.index("$migration_guard$;", guard_position)
        guard = self.sql[guard_position:guard_end]
        for legacy_table in ("coding_lessons", "project_lessons", "writing_lessons", "audio_lessons"):
            self.assertIn(f"SELECT COUNT(*) FROM {legacy_table}", guard)
        self.assertIn("SELECT COUNT(*) FROM lessons INTO copied", guard)
        self.assertIn("IF copied <> expected THEN", guard)
        self.assertIn("RAISE EXCEPTION 'lesson migration count mismatch", guard)
        first_drop = min(self.sql.index(f"DROP TABLE {name};") for name in (
            "coding_lessons", "project_lessons", "writing_lessons", "audio_lessons"
        ))
        self.assertLess(guard_end, first_drop)
        self.assertLess(self.sql.index("SELECT setval("), first_drop)

    def test_type_specific_identities_are_partial_indexes_on_one_table(self) -> None:
        expected = {
            "coding": "ON lessons (scope, project, title) NULLS NOT DISTINCT",
            "project": "ON lessons (project, title)",
            "writing": "ON lessons (voice, title) NULLS NOT DISTINCT",
            "audio": "ON lessons (title)",
        }
        for lesson_key, identity in expected.items():
            pattern = re.compile(
                rf"CREATE UNIQUE INDEX lessons_{lesson_key}_identity_uidx\s+"
                rf"{re.escape(identity)}\s+WHERE lesson_key = '{lesson_key}';",
                re.MULTILINE,
            )
            self.assertRegex(self.sql, pattern)


class DesignLessonMigrationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.sql = DESIGN_MIGRATION.read_text(encoding="utf-8")

    def test_design_identity_index_is_isolated_to_its_lesson_key(self) -> None:
        self.assertRegex(
            self.sql,
            re.compile(
                r"CREATE UNIQUE INDEX lessons_design_identity_uidx\s+"
                r"ON lessons \(voice, title\) NULLS NOT DISTINCT\s+"
                r"WHERE lesson_key = 'design';",
                re.MULTILINE,
            ),
        )

    def test_design_contract_fields_already_exist_in_unified_lessons(self) -> None:
        unified = MIGRATION.read_text(encoding="utf-8")
        for field in (
            "voice TEXT",
            "register TEXT[]",
            "shape TEXT",
            "proof_pattern TEXT",
            "trigger_context TEXT",
            "example_text TEXT",
            "tags TEXT[]",
        ):
            self.assertIn(field, unified)
        self.assertNotIn("ADD COLUMN", self.sql)

    def test_design_recorder_upsert_is_isolated_to_design_rows(self) -> None:
        recorder = DESIGN_RECORDER.read_text(encoding="utf-8")
        self.assertIn("('design', %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s::jsonb)", recorder)
        self.assertIn("ON CONFLICT (voice, title) WHERE lesson_key = 'design' DO UPDATE SET", recorder)

    def test_design_migration_registers_schema_version_eleven(self) -> None:
        self.assertIn("INSERT INTO schema_migrations (version) VALUES (11)", self.sql)
        self.assertIn("ON CONFLICT (version) DO NOTHING", self.sql)


if __name__ == "__main__":
    unittest.main()
