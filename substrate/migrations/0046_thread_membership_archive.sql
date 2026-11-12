-- 0046: Thread membership archive. A memory leaves one thread and its history stays.
--
-- `memory_thread_leave` moves one row out of `thread_events` into this table.
-- The archive row keeps the original event ID, the join time, the reason, and the operator.
-- It also keeps the predecessor memory, the successor memories, and the thread references at archive time.
-- The successors then continue from the predecessor in `thread_event_links`.
--
-- Active thread reads, Recall, and `memories.threads` read only active rows.
-- They need no archive filter.
--
-- Re-applicable against fresh state.

BEGIN;

CREATE TABLE IF NOT EXISTS thread_event_archive (
    event_id           BIGINT       PRIMARY KEY,
    thread_id          BIGINT       NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    memory_id          BIGINT       NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    joined_at          TIMESTAMPTZ  NOT NULL,
    previous_memory_id BIGINT,
    next_memory_ids    BIGINT[]     NOT NULL DEFAULT '{}',
    refs               JSONB        NOT NULL DEFAULT '[]',
    archived_at        TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    archived_by        TEXT,
    reason             TEXT         NOT NULL,
    CONSTRAINT thread_event_archive_reason_check CHECK (
        reason = btrim(reason) AND char_length(reason) BETWEEN 1 AND 500
    )
);

CREATE INDEX IF NOT EXISTS thread_event_archive_thread_memory_idx
    ON thread_event_archive (thread_id, memory_id);
CREATE INDEX IF NOT EXISTS thread_event_archive_memory_idx
    ON thread_event_archive (memory_id);

COMMIT;
