BEGIN;

CREATE TABLE IF NOT EXISTS hallway_channels (
    id BIGSERIAL PRIMARY KEY,
    hallway_key TEXT NOT NULL UNIQUE,
    created_by_room TEXT NOT NULL,
    created_by_spirit TEXT NOT NULL,
    created_by_session TEXT NOT NULL,
    create_idempotency_key TEXT NOT NULL,
    create_digest TEXT NOT NULL,
    operator_visible BOOLEAN NOT NULL DEFAULT TRUE,
    wake_policy TEXT NOT NULL DEFAULT 'manual' CHECK (wake_policy IN ('manual')),
    next_sequence BIGINT NOT NULL DEFAULT 1 CHECK (next_sequence > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (created_by_room, created_by_session, create_idempotency_key)
);

CREATE TABLE IF NOT EXISTS hallway_allowed_rooms (
    hallway_id BIGINT NOT NULL REFERENCES hallway_channels(id) ON DELETE CASCADE,
    room TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (hallway_id, room)
);

CREATE TABLE IF NOT EXISTS hallway_presences (
    hallway_id BIGINT NOT NULL REFERENCES hallway_channels(id) ON DELETE CASCADE,
    room TEXT NOT NULL,
    spirit TEXT NOT NULL,
    session_id TEXT NOT NULL,
    join_idempotency_key TEXT NOT NULL,
    join_digest TEXT NOT NULL,
    read_cursor BIGINT NOT NULL DEFAULT 0 CHECK (read_cursor >= 0),
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (hallway_id, room, session_id),
    UNIQUE (hallway_id, room, session_id, join_idempotency_key),
    FOREIGN KEY (hallway_id, room)
        REFERENCES hallway_allowed_rooms(hallway_id, room)
        ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS hallway_messages (
    id BIGSERIAL PRIMARY KEY,
    hallway_id BIGINT NOT NULL REFERENCES hallway_channels(id) ON DELETE CASCADE,
    sequence BIGINT NOT NULL CHECK (sequence > 0),
    room TEXT NOT NULL,
    spirit TEXT NOT NULL,
    session_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    body TEXT NOT NULL,
    body_digest TEXT NOT NULL,
    reply_to BIGINT REFERENCES hallway_messages(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (hallway_id, sequence),
    UNIQUE (hallway_id, room, session_id, idempotency_key),
    FOREIGN KEY (hallway_id, room, session_id)
        REFERENCES hallway_presences(hallway_id, room, session_id)
        ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_hallway_messages_stream
    ON hallway_messages(hallway_id, id);

CREATE INDEX IF NOT EXISTS idx_hallway_presences_spirit
    ON hallway_presences(hallway_id, spirit);

COMMIT;
