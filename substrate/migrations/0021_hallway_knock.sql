-- 0021: PostgreSQL-authoritative Hallway Knock policy and bounded turn leases.
-- Re-applicable against fresh, partially-applied, and fully-applied state.
-- A Knock references structured Hallway state only; message prose never enters it.

BEGIN;

-- Policy rows are an append-only command history. The partial unique index
-- identifies the one current policy while retaining every idempotency digest.
CREATE TABLE IF NOT EXISTS hallway_knock_policies (
    id BIGSERIAL PRIMARY KEY,
    hallway_id BIGINT NOT NULL REFERENCES hallway_channels(id) ON DELETE CASCADE,
    room TEXT NOT NULL,
    spirit TEXT NOT NULL,
    session_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_digest TEXT NOT NULL,
    mode TEXT NOT NULL DEFAULT 'manual' CHECK (mode IN ('manual', 'allow_list')),
    allowed_rooms TEXT[] NOT NULL DEFAULT '{}',
    max_turns SMALLINT NOT NULL DEFAULT 4 CHECK (max_turns BETWEEN 1 AND 8),
    revision BIGINT NOT NULL CHECK (revision > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    superseded_at TIMESTAMPTZ,
    UNIQUE (hallway_id, room, session_id, idempotency_key),
    FOREIGN KEY (hallway_id, room)
        REFERENCES hallway_allowed_rooms(hallway_id, room) ON DELETE CASCADE,
    CHECK (cardinality(allowed_rooms) <= 32),
    CHECK (mode <> 'manual' OR cardinality(allowed_rooms) = 0)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_hallway_knock_policies_current
    ON hallway_knock_policies (hallway_id, room)
    WHERE superseded_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_hallway_knock_policies_lookup
    ON hallway_knock_policies (hallway_id, room, superseded_at);

CREATE TABLE IF NOT EXISTS hallway_knocks (
    knock_id UUID PRIMARY KEY,
    hallway_id BIGINT NOT NULL REFERENCES hallway_channels(id) ON DELETE CASCADE,
    message_id BIGINT NOT NULL REFERENCES hallway_messages(id) ON DELETE RESTRICT,
    from_room TEXT NOT NULL,
    from_spirit TEXT NOT NULL,
    request_session TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_digest TEXT NOT NULL,
    recipient_room TEXT NOT NULL,
    parent_knock_id UUID REFERENCES hallway_knocks(knock_id) ON DELETE RESTRICT,
    root_knock_id UUID NOT NULL REFERENCES hallway_knocks(knock_id) ON DELETE RESTRICT,
    turn_index SMALLINT NOT NULL CHECK (turn_index BETWEEN 1 AND 8),
    max_turns SMALLINT NOT NULL CHECK (max_turns BETWEEN 1 AND 8),
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'claimed', 'started', 'completed', 'failed')),
    claimed_by_room TEXT,
    claimed_by_spirit TEXT,
    claimed_by_session TEXT,
    lease_expires_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ NOT NULL,
    started_at TIMESTAMPTZ,
    started_reason TEXT,
    settled_at TIMESTAMPTZ,
    settled_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (hallway_id, from_room, request_session, idempotency_key),
    UNIQUE (message_id, recipient_room),
    FOREIGN KEY (hallway_id, recipient_room)
        REFERENCES hallway_allowed_rooms(hallway_id, room) ON DELETE CASCADE,
    CHECK (turn_index <= max_turns),
    CHECK (
        (turn_index = 1 AND parent_knock_id IS NULL AND root_knock_id = knock_id)
        OR (turn_index > 1 AND parent_knock_id IS NOT NULL)
    ),
    CHECK (started_reason IS NULL OR octet_length(started_reason) <= 2048),
    CHECK (settled_reason IS NULL OR octet_length(settled_reason) <= 2048),
    CHECK (
        (status IN ('pending', 'claimed') AND started_at IS NULL AND settled_at IS NULL)
        OR (status = 'started' AND started_at IS NOT NULL AND settled_at IS NULL)
        OR (status = 'completed' AND started_at IS NOT NULL AND settled_at IS NOT NULL)
        OR (status = 'failed' AND settled_at IS NOT NULL)
    ),
    CHECK (
        (status = 'pending' AND claimed_by_room IS NULL AND claimed_by_spirit IS NULL
            AND claimed_by_session IS NULL AND lease_expires_at IS NULL)
        OR
        (status <> 'pending' AND claimed_by_room IS NOT NULL AND claimed_by_spirit IS NOT NULL
            AND claimed_by_session IS NOT NULL AND lease_expires_at IS NOT NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_hallway_knocks_claim
    ON hallway_knocks (recipient_room, created_at, knock_id)
    WHERE status IN ('pending', 'claimed');

CREATE INDEX IF NOT EXISTS idx_hallway_knocks_parent
    ON hallway_knocks (parent_knock_id);

COMMIT;
