import importlib.util
import unittest
from datetime import date, datetime, timezone
from pathlib import Path


SOURCE = Path(__file__).parents[1] / "src" / "postgres-memory-source.py"
SPEC = importlib.util.spec_from_file_location("postgres_memory_source", SOURCE)
MEMORY_SOURCE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MEMORY_SOURCE)

COLUMNS = {"archived_at": True, "superseded_by": True}
ARCHIVED_AT = datetime(2026, 7, 18, 12, 0, tzinfo=timezone.utc)


class FakeCursor:
    def __init__(self, responses, executions):
        self.responses = responses
        self.executions = executions
        self.rows = []

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, tb):
        return False

    def execute(self, sql, params=None):
        self.executions.append((" ".join(sql.split()), params))
        self.rows = self.responses.pop(0) if self.responses else []

    def fetchall(self):
        return self.rows

    def fetchone(self):
        return self.rows[0] if self.rows else None


class FakeConnection:
    def __init__(self, *responses):
        self.responses = list(responses)
        self.executions = []
        self.cursor_instance = FakeCursor(self.responses, self.executions)

    def cursor(self, **_kwargs):
        return self.cursor_instance


def where_clause(sql):
    return sql.split(" WHERE ", 1)[1]


def historical_chunk(**overrides):
    row = {
        "memory_id": 41,
        "source_path": "memory/old-retrieval.md",
        "room": "kintsu",
        "chunk_index": 2,
        "heading_path": "Authority",
        "body": "old retrieval authority",
        "char_start": 10,
        "char_end": 33,
        "archived_at": ARCHIVED_AT,
        "superseded_by": 84,
        "superseded": True,
    }
    row.update(overrides)
    return row


class AuthorityFilterTests(unittest.TestCase):
    def test_filter_modes_preserve_archived_opt_in_and_require_historical_for_superseded(self):
        self.assertEqual(
            MEMORY_SOURCE.erasure_filter(COLUMNS),
            "m.archived_at IS NULL AND m.superseded_by IS NULL",
        )
        self.assertEqual(
            MEMORY_SOURCE.erasure_filter(COLUMNS, include_archived=True),
            "m.superseded_by IS NULL",
        )
        self.assertEqual(
            MEMORY_SOURCE.erasure_filter(COLUMNS, include_historical=True),
            "TRUE",
        )

    def test_default_predicates_precede_each_retrieval_lane_scoring_clause(self):
        lexical = FakeConnection([], [])
        MEMORY_SOURCE.load_index(lexical, erasure_columns=COLUMNS)
        for sql, _ in lexical.executions:
            where = where_clause(sql)
            self.assertIn("m.archived_at IS NULL", where)
            self.assertIn("m.superseded_by IS NULL", where)

        candidates = FakeConnection([], [], [], [], [])
        MEMORY_SOURCE.load_search_candidates(
            candidates,
            rooms=("kintsu", "house"),
            query="retrieval authority",
            top_k=5,
            erasure_columns=COLUMNS,
        )
        memory_queries = [
            sql for sql, _ in candidates.executions
            if "JOIN memories m" in sql or "FROM memories m, q" in sql
        ]
        self.assertEqual(len(memory_queries), 2)
        for sql in memory_queries:
            where = where_clause(sql)
            scoring_clause = (
                "mt.thread_key ILIKE" if "FROM memory_threads mt" in sql
                else "m.body_tsv @@"
            )
            self.assertLess(
                where.index("m.archived_at IS NULL"), where.index(scoring_clause)
            )
            self.assertLess(
                where.index("m.superseded_by IS NULL"), where.index(scoring_clause)
            )

        semantic = FakeConnection([])
        MEMORY_SOURCE.load_semantic_chunks(
            semantic,
            rooms=("kintsu",),
            query_vec=[0.1, 0.2],
            top_k=5,
            min_sim=0.4,
            erasure_columns=COLUMNS,
        )
        semantic_where = where_clause(semantic.executions[0][0])
        self.assertLess(
            semantic_where.index("m.superseded_by IS NULL"),
            semantic_where.index("mc.body_embedding IS NOT NULL"),
        )

        content = FakeConnection([])
        MEMORY_SOURCE.load_content_chunks(
            content,
            rooms=("kintsu",),
            query="retrieval authority",
            top_k=5,
            min_sim=0.3,
            erasure_columns=COLUMNS,
        )
        content_where = where_clause(content.executions[0][0])
        self.assertLess(
            content_where.index("m.superseded_by IS NULL"),
            content_where.index("mc.body ILIKE"),
        )
        self.assertLess(
            content_where.index("m.superseded_by IS NULL"),
            content_where.index("word_similarity"),
        )

        dated = FakeConnection([])
        MEMORY_SOURCE.load_date_matches(
            dated,
            rooms=("kintsu",),
            query_dates=[date(2026, 7, 18)],
            top_k=5,
            excerpt_chars=200,
            erasure_columns=COLUMNS,
        )
        date_where = where_clause(dated.executions[0][0])
        self.assertLess(
            date_where.index("m.superseded_by IS NULL"),
            date_where.index("m.dates &&"),
        )


class HistoricalOutputTests(unittest.TestCase):
    def assert_historical_contract(self, item, *, scored=False):
        self.assertEqual(item["authority_state"], "historical")
        self.assertTrue(item["historical"])
        self.assertTrue(item["archived"])
        self.assertEqual(item["archived_at"], ARCHIVED_AT.isoformat())
        self.assertTrue(item["superseded"])
        self.assertEqual(item["superseded_by"], 84)
        if scored:
            self.assertIn("archived", item.get("reasons", []))
            self.assertIn("superseded", item.get("reasons", []))

    def test_historical_opt_in_covers_candidate_semantic_content_and_date_outputs(self):
        candidate_row = {
            "id": 41,
            "room": "kintsu",
            "title": "Old retrieval authority",
            "source_path": "memory/old-retrieval.md",
            "type": "state-claim",
            "excerpt": "old retrieval authority",
            "raw_rank": 0.8,
            "title_hit": True,
            "path_hit": True,
            "thread_text": "retrieval",
            "archived_at": ARCHIVED_AT,
            "superseded_by": 84,
            "superseded": True,
        }
        candidates = FakeConnection([], [], [candidate_row], [], [])
        _, candidate_items = MEMORY_SOURCE.load_search_candidates(
            candidates,
            rooms=("kintsu",),
            query="retrieval authority",
            top_k=5,
            erasure_columns=COLUMNS,
            include_historical=True,
        )
        self.assertEqual(len(candidate_items), 1)
        self.assert_historical_contract(candidate_items[0], scored=True)
        for sql, _ in candidates.executions:
            if "JOIN memories m" in sql or "FROM memories m, q" in sql:
                self.assertNotIn("archived_at IS NULL", sql)
                self.assertNotIn("superseded_by IS NULL", sql)

        semantic_row = historical_chunk(sim=0.8)
        semantic = FakeConnection([semantic_row])
        semantic_items = MEMORY_SOURCE.load_semantic_chunks(
            semantic,
            rooms=("kintsu",),
            query_vec=[0.1, 0.2],
            top_k=5,
            min_sim=0.4,
            erasure_columns=COLUMNS,
            include_historical=True,
        )
        self.assert_historical_contract(semantic_items[0], scored=True)

        content_row = historical_chunk(ws=0.7)
        content = FakeConnection([content_row])
        content_items = MEMORY_SOURCE.load_content_chunks(
            content,
            rooms=("kintsu",),
            query="retrieval authority",
            top_k=5,
            min_sim=0.3,
            erasure_columns=COLUMNS,
            include_historical=True,
        )
        self.assert_historical_contract(content_items[0], scored=True)

        date_row = {
            "id": 41,
            "room": "kintsu",
            "source_path": "memory/old-retrieval.md",
            "title": "Old retrieval authority",
            "type": "state-claim",
            "date": date(2026, 7, 18),
            "dates": [date(2026, 7, 18)],
            "threads": ["retrieval"],
            "body_excerpt": "old retrieval authority",
            "body_full_chars": 500,
            "archived_at": ARCHIVED_AT,
            "superseded_by": 84,
            "superseded": True,
        }
        dated = FakeConnection([date_row])
        date_items = MEMORY_SOURCE.load_date_matches(
            dated,
            rooms=("kintsu",),
            query_dates=[date(2026, 7, 18)],
            top_k=5,
            excerpt_chars=200,
            erasure_columns=COLUMNS,
            include_historical=True,
        )
        self.assert_historical_contract(date_items[0], scored=True)

    def test_lexical_historical_opt_in_reports_authority_and_successor_id(self):
        file_row = {
            "memory_id": 41,
            "room": "kintsu",
            "source_path": "memory/old-retrieval.md",
            "date": date(2026, 7, 18),
            "type": "state-claim",
            "one_line": "old authority",
            "archived_at": ARCHIVED_AT,
            "superseded_by": 84,
            "superseded": True,
        }
        thread_row = {
            "memory_id": 41,
            "room": "kintsu",
            "thread_key": "retrieval",
            "file": "memory/old-retrieval.md",
            "lines_start": 3,
            "lines_end": 8,
            "context": "historical context",
            "archived_at": ARCHIVED_AT,
            "superseded_by": 84,
            "superseded": True,
        }
        conn = FakeConnection([file_row], [thread_row])
        index = MEMORY_SOURCE.load_index(
            conn,
            erasure_columns=COLUMNS,
            include_historical=True,
        )
        self.assert_historical_contract(index["files"]["memory/old-retrieval.md"])
        self.assert_historical_contract(index["threads"]["retrieval"][0])
        for sql, _ in conn.executions:
            self.assertNotIn("archived_at IS NULL", sql)
            self.assertNotIn("superseded_by IS NULL", sql)

    def test_explicit_handle_fetches_historical_row_with_lifecycle_and_successor_provenance(self):
        handle_row = {
            "id": 41,
            "room": "house",
            "type": "state-claim",
            "date": date(2026, 7, 18),
            "title": "Old authority",
            "source_path": "memory/old-authority.md",
            "body": "historical body",
            "created_at": datetime(2026, 7, 18, 11, 0, tzinfo=timezone.utc),
            "archived_at": ARCHIVED_AT,
            "superseded_by": 84,
            "superseded": True,
            "successor_id": 84,
            "successor_room": "house",
            "successor_title": "Current authority",
            "successor_source_path": "memory/current-authority.md",
        }
        conn = FakeConnection([handle_row], [{"thread_key": "authority"}])
        result = MEMORY_SOURCE.fetch_memory(
            conn,
            memory_id=41,
            claimed_room="kintsu",
            erasure_columns=COLUMNS,
        )
        self.assertTrue(result["found"])
        self.assertEqual(len(result["warnings"]), 1)
        memory = result["memory"]
        self.assert_historical_contract(memory)
        self.assertEqual(memory["threads"], ["authority"])
        self.assertEqual(
            memory["successor"],
            {
                "id": 84,
                "room": "house",
                "title": "Current authority",
                "source_path": "memory/current-authority.md",
            },
        )
        fetch_sql = conn.executions[0][0]
        self.assertIn("LEFT JOIN memories successor", fetch_sql)
        self.assertNotIn("archived_at IS NULL", fetch_sql)
        self.assertNotIn("superseded_by IS NULL", fetch_sql)


if __name__ == "__main__":
    unittest.main()
