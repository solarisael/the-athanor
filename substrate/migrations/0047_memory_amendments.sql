-- 0047: Memory amendments. A newer memory corrects or updates an older memory.
--
-- An amendment is an edge from the older memory to the newer memory.
-- `changed` tells what the newer memory corrects or updates.
-- `keeps` tells what only the older memory still holds.
-- The older memory keeps its retrieval authority. An amendment never sets
-- `superseded_by`, never archives a memory, and never changes a title or a body.
--
-- Re-applicable against fresh state.

BEGIN;

CREATE TABLE IF NOT EXISTS memory_amendments (
    older_id   BIGINT       NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    newer_id   BIGINT       NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    changed    TEXT         NOT NULL,
    keeps      TEXT,
    created_at TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    created_by TEXT,
    PRIMARY KEY (older_id, newer_id),
    CONSTRAINT memory_amendments_not_self CHECK (older_id <> newer_id),
    CONSTRAINT memory_amendments_changed_check CHECK (
        changed = btrim(changed) AND char_length(changed) BETWEEN 1 AND 500
    ),
    CONSTRAINT memory_amendments_keeps_check CHECK (
        keeps IS NULL OR (keeps = btrim(keeps) AND char_length(keeps) BETWEEN 1 AND 500)
    )
);

-- Recall reads the edges from both ends.
CREATE INDEX IF NOT EXISTS memory_amendments_newer_idx
    ON memory_amendments (newer_id);

COMMIT;
