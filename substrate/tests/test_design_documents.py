from __future__ import annotations

import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MIGRATION = ROOT / "migrations" / "0012_design_documents.sql"


class DesignDocumentsMigrationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.sql = MIGRATION.read_text(encoding="utf-8")

    def test_migration_registers_schema_version_twelve_idempotently(self) -> None:
        self.assertIn("INSERT INTO schema_migrations (version) VALUES (12)", self.sql)
        self.assertIn("ON CONFLICT (version) DO NOTHING", self.sql)

    def test_current_identity_is_a_partial_unique_index(self) -> None:
        self.assertRegex(
            self.sql,
            re.compile(
                r"CREATE UNIQUE INDEX design_documents_current_identity_uidx\s+"
                r"ON design_documents \(system, doc_type, name\)\s+"
                r"WHERE superseded_by IS NULL;",
                re.MULTILINE,
            ),
        )

    def test_supersession_preserves_history_with_a_self_reference(self) -> None:
        self.assertRegex(
            self.sql,
            re.compile(
                r"superseded_by BIGINT NULL REFERENCES design_documents\(id\) ON DELETE SET NULL,",
                re.MULTILINE,
            ),
        )


if __name__ == "__main__":
    unittest.main()
