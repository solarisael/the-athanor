import importlib.util
import unittest
from pathlib import Path
from unittest.mock import patch

MODULE = Path(__file__).with_name("entity-resolution.py")
spec = importlib.util.spec_from_file_location("entity_resolution", MODULE)
resolver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(resolver)

ENTITIES = [
    {"name": "North Star", "kind": "project", "aliases": ["the north-star", "star"]},
    {"name": "Blue Squad", "kind": "group", "aliases": ["blue", "blue team"]},
    {"name": "Alpha", "kind": "person", "aliases": ["A"]},
]


class EntityResolutionTests(unittest.TestCase):
    def test_canonical_and_alias_normalize_case_and_punctuation(self):
        canonical = resolver.resolve_matches("Please check NORTH-STAR.", ENTITIES)
        self.assertEqual(canonical[0]["canonicalName"], "North Star")
        alias = resolver.resolve_matches("the NORTH-STAR", ENTITIES)
        self.assertEqual(alias[0]["matchedAlias"], "the north-star")
    def test_longest_multiword_alias_wins_and_deduplicates(self):
        matches = resolver.resolve_matches("blue team", ENTITIES)
        self.assertEqual(len(matches), 1)
        self.assertEqual(matches[0]["matchedAlias"], "blue team")

    def test_word_boundary_avoids_unrelated_substrings(self):
        self.assertEqual(resolver.resolve_matches("blueness", ENTITIES), [])
        self.assertEqual(resolver.resolve_matches("A plan", ENTITIES), [])

    def test_bounds(self):
        entities = [{"name": f"Name {i}", "kind": "thing", "aliases": []} for i in range(10)]
        self.assertEqual(len(resolver.resolve_matches("name 0 name 1 name 2", entities, limit=2)), 2)

    def test_unavailable_substrate_fails_open(self):
        with patch.object(resolver, "psycopg2", None):
            self.assertEqual(resolver.fetch_matches("anything", "room", "."), [])


if __name__ == "__main__":
    unittest.main()
