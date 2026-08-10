from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

import import_coding_lessons as importer


class FakeCursor:
    def __init__(self, existing: dict[tuple[str, str | None, str], int] | None = None) -> None:
        self.existing = {
            key: {"id": lesson_id, "negation_of": None}
            for key, lesson_id in (existing or {}).items()
        }
        self.pending = None
        self.inserted: list[tuple[str, str | None, str]] = []
        self.updated: list[int] = []
        self.linked: list[tuple[int, int]] = []

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, tb):
        return False

    def execute(self, sql: str, args: tuple) -> None:
        operation = sql.strip().split(None, 1)[0].upper()
        if operation == "SELECT":
            key = (args[0], args[1], args[2])
            lesson = self.existing.get(key)
            self.pending = (
                (lesson["id"], lesson["negation_of"])
                if lesson is not None
                else None
            )
        elif operation == "INSERT":
            key = (args[0], args[1], args[2])
            next_id = max(
                (lesson["id"] for lesson in self.existing.values()),
                default=0,
            ) + 1
            self.existing[key] = {"id": next_id, "negation_of": None}
            self.inserted.append(key)
        elif operation == "UPDATE" and "SET negation_of" in sql:
            target_id, source_id = args
            source = next(
                lesson
                for lesson in self.existing.values()
                if lesson["id"] == source_id
            )
            source["negation_of"] = target_id
            self.linked.append((source_id, target_id))
        elif operation == "UPDATE":
            self.updated.append(args[-1])
        else:
            raise AssertionError(f"unexpected operation: {operation}")

    def fetchone(self):
        return self.pending


class FakeConnection:
    def __init__(self, cursor: FakeCursor) -> None:
        self.cursor_value = cursor

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, tb):
        return False

    def cursor(self) -> FakeCursor:
        return self.cursor_value


class CodingLessonPackTests(unittest.TestCase):
    def setUp(self) -> None:
        self.pack = importer.load_pack(importer.DEFAULT_PACK)

    def test_bundled_pack_is_valid_and_unique(self) -> None:
        self.assertEqual(self.pack["id"], "solarisael-house-craft-starter")
        self.assertEqual(self.pack["version"], 2)
        self.assertEqual(len(self.pack["lessons"]), 117)
        keys = {
            (lesson["scope"], lesson["project"], lesson["title"])
            for lesson in self.pack["lessons"]
        }
        self.assertEqual(len(keys), 117)
        self.assertEqual(sum(lesson["always_on"] for lesson in self.pack["lessons"]), 16)
        self.assertEqual(
            sum(lesson["negation_of_title"] is not None for lesson in self.pack["lessons"]),
            12,
        )
        self.assertTrue(
            all(
                lesson["meta"]["starter_pack_version"] == 2
                for lesson in self.pack["lessons"]
            )
        )

    def test_version_two_preserves_version_one_titles(self) -> None:
        version_one_titles = {
            "The spine: plain line, clean door, sharp refusal",
            "Make the first draft loud, then compress it",
            "A file should have one silhouette",
            "A helper with a vague name exposes the wrong seam",
            "Refuse helpers that only launder anxiety",
            "Count minimalism in concepts, not characters",
            "Delete the clever line when the plain line reads better",
            "Use negative space as punctuation",
            "Write documentation in simplified technical English",
            "Explore existing behavior before rebuilding it",
            "Do not hard-code the current example to green",
            "Merge by intent before side",
            "Branch promotion is not runtime proof",
            "Verify cleanup and inspect the staged set",
        }
        current_titles = {lesson["title"] for lesson in self.pack["lessons"]}
        self.assertTrue(version_one_titles.issubset(current_titles))

    def test_duplicate_lesson_keys_are_rejected(self) -> None:
        document = json.loads(importer.DEFAULT_PACK.read_text(encoding="utf-8"))
        document["lessons"].append(dict(document["lessons"][0]))
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "duplicate.json"
            path.write_text(json.dumps(document), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "duplicate lesson key"):
                importer.load_pack(path)

    def test_default_import_preserves_existing_lessons(self) -> None:
        lessons = self.pack["lessons"][:2]
        key = (lessons[0]["scope"], lessons[0]["project"], lessons[0]["title"])
        cursor = FakeCursor({key: 41})
        counts = importer.import_pack(
            FakeConnection(cursor),
            {**self.pack, "lessons": lessons},
            update_existing=False,
        )
        self.assertEqual(
            counts,
            {
                "inserted": 1,
                "updated": 0,
                "skipped": 1,
                "linked": 0,
                "link_skipped": 0,
            },
        )
        self.assertEqual(cursor.updated, [])

    def test_update_existing_requires_explicit_flag(self) -> None:
        lesson = self.pack["lessons"][0]
        key = (lesson["scope"], lesson["project"], lesson["title"])
        cursor = FakeCursor({key: 77})
        counts = importer.import_pack(
            FakeConnection(cursor),
            {**self.pack, "lessons": [lesson]},
            update_existing=True,
        )
        self.assertEqual(
            counts,
            {
                "inserted": 0,
                "updated": 1,
                "skipped": 0,
                "linked": 0,
                "link_skipped": 0,
            },
        )
        self.assertEqual(cursor.updated, [77])

    def test_negation_links_resolve_after_insert(self) -> None:
        source = next(
            lesson
            for lesson in self.pack["lessons"]
            if lesson["negation_of_title"] is not None
        )
        target = next(
            lesson
            for lesson in self.pack["lessons"]
            if lesson["title"] == source["negation_of_title"]
        )
        cursor = FakeCursor()
        counts = importer.import_pack(
            FakeConnection(cursor),
            {**self.pack, "lessons": [source, target]},
            update_existing=False,
        )
        self.assertEqual(counts["inserted"], 2)
        self.assertEqual(counts["linked"], 1)
        self.assertEqual(len(cursor.linked), 1)

    def test_full_pack_imports_all_lessons_and_links(self) -> None:
        cursor = FakeCursor()
        counts = importer.import_pack(
            FakeConnection(cursor),
            self.pack,
            update_existing=False,
        )
        self.assertEqual(counts["inserted"], 117)
        self.assertEqual(counts["linked"], 12)
        self.assertEqual(len(cursor.existing), 117)

    def test_missing_negation_target_is_rejected(self) -> None:
        document = json.loads(importer.DEFAULT_PACK.read_text(encoding="utf-8"))
        source = next(
            lesson
            for lesson in document["lessons"]
            if lesson.get("negation_of_title")
        )
        document["lessons"] = [
            lesson
            for lesson in document["lessons"]
            if lesson["title"] != source["negation_of_title"]
        ]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "missing-target.json"
            path.write_text(json.dumps(document), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "negation target is not in the pack"):
                importer.load_pack(path)

    def test_public_pack_has_no_project_scope_or_private_markers(self) -> None:
        self.assertTrue(all(lesson["project"] is None for lesson in self.pack["lessons"]))
        serialized = json.dumps(self.pack["lessons"], ensure_ascii=False).lower()
        forbidden = (
            "c:\\\\",
            "/home/",
            "multistock",
            "cruzeiro",
            "solarisael/obsidian",
            "kintsu",
            "kodo",
            "tuner",
        )
        self.assertFalse(any(marker in serialized for marker in forbidden))


if __name__ == "__main__":
    unittest.main()
