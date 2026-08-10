import unittest
from pathlib import Path


class LessonEligibilityMigrationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.sql = (Path(__file__).parents[1] / "migrations" / "0013_lesson_eligibility_keys.sql").read_text(encoding="utf-8")

    def test_migration_adds_keys_to_unified_lessons_table(self) -> None:
        self.assertIn("ALTER TABLE lessons", self.sql)
        self.assertIn("language_keys TEXT[] NOT NULL DEFAULT '{}'", self.sql)
        self.assertIn("technology_keys TEXT[] NOT NULL DEFAULT '{}'", self.sql)
        self.assertNotIn("ALTER TABLE coding_lessons", self.sql)
        self.assertNotIn("ALTER TABLE project_lessons", self.sql)

    def test_migration_indexes_and_registers_schema_thirteen(self) -> None:
        self.assertIn("USING GIN (language_keys)", self.sql)
        self.assertIn("USING GIN (technology_keys)", self.sql)
        self.assertIn("INSERT INTO schema_migrations (version) VALUES (13)", self.sql)
        self.assertIn("ON CONFLICT (version) DO NOTHING", self.sql)


if __name__ == "__main__":
    unittest.main()
