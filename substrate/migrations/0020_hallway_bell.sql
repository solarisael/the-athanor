-- 0020: Hallway Bell — daily threads, room-stable read state, targeted
-- notifications. House memory #3676 is the architecture. Idempotent: safe
-- against fresh, partially-applied, and fully-applied state.
--
-- Bell geometry: Hallway stores the message. Bell records who has pending
-- attention. Reading acknowledges and quiets the Bell. Ordinary unread is
-- DERIVED (next_sequence - 1 - read_sequence), never stored per message.

BEGIN;

-- Daily threads: a persistent Hallway contains lightweight day tables.
CREATE TABLE IF NOT EXISTS hallway_threads (
    id BIGSERIAL PRIMARY KEY,
    hallway_id BIGINT NOT NULL REFERENCES hallway_channels(id) ON DELETE CASCADE,
    thread_key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (hallway_id, thread_key)
);

-- Messages carry their thread and their structured recipients. to_rooms is
-- resolved by the composer into stable room keys; body text is never parsed
-- for mentions.
ALTER TABLE hallway_messages
    ADD COLUMN IF NOT EXISTS thread_id BIGINT REFERENCES hallway_threads(id);
ALTER TABLE hallway_messages
    ADD COLUMN IF NOT EXISTS to_rooms TEXT[] NOT NULL DEFAULT '{}';

-- Room-stable read state, in per-hallway SEQUENCE space (gapless), distinct
-- from hallway_presences.read_cursor which stays session-scoped in id space.
CREATE TABLE IF NOT EXISTS hallway_room_state (
    hallway_id BIGINT NOT NULL REFERENCES hallway_channels(id) ON DELETE CASCADE,
    room TEXT NOT NULL,
    read_sequence BIGINT NOT NULL DEFAULT 0 CHECK (read_sequence >= 0),
    notification_revision BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (hallway_id, room),
    FOREIGN KEY (hallway_id, room)
        REFERENCES hallway_allowed_rooms(hallway_id, room) ON DELETE CASCADE
);

-- Durable Bell rows: targeted attention events. Displaying or delivering
-- never clears one; only an actual covering read sets read_at.
CREATE TABLE IF NOT EXISTS hallway_notifications (
    id BIGSERIAL PRIMARY KEY,
    hallway_id BIGINT NOT NULL REFERENCES hallway_channels(id) ON DELETE CASCADE,
    message_id BIGINT NOT NULL REFERENCES hallway_messages(id) ON DELETE CASCADE,
    recipient_room TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'mention' CHECK (kind IN ('mention')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    read_at TIMESTAMPTZ,
    UNIQUE (message_id, recipient_room)
);

CREATE INDEX IF NOT EXISTS idx_hallway_notifications_pending
    ON hallway_notifications (hallway_id, recipient_room)
    WHERE read_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_hallway_messages_thread
    ON hallway_messages (thread_id);

-- ---------------------------------------------------------------------------
-- Backfills, all guarded. House timezone for legacy day assignment:
-- America/Sao_Paulo (operator-confirmed default; runtime authority is the
-- SOLARISAEL_HOUSE_TZ config key).

-- Root messages (non-replies) name the day threads they need.
INSERT INTO hallway_threads (hallway_id, thread_key)
SELECT DISTINCT
    m.hallway_id,
    to_char(m.created_at AT TIME ZONE 'America/Sao_Paulo', 'YYYY-MM-DD')
FROM hallway_messages m
WHERE m.thread_id IS NULL AND m.reply_to IS NULL
ON CONFLICT (hallway_id, thread_key) DO NOTHING;

UPDATE hallway_messages m
SET thread_id = t.id
FROM hallway_threads t
WHERE m.thread_id IS NULL
  AND m.reply_to IS NULL
  AND t.hallway_id = m.hallway_id
  AND t.thread_key = to_char(m.created_at AT TIME ZONE 'America/Sao_Paulo', 'YYYY-MM-DD');

-- Replies inherit the parent's thread even across midnight; iterate until
-- every reply chain is resolved.
DO $$
DECLARE
    touched BIGINT;
BEGIN
    LOOP
        UPDATE hallway_messages child
        SET thread_id = parent.thread_id
        FROM hallway_messages parent
        WHERE child.reply_to = parent.id
          AND child.thread_id IS NULL
          AND parent.thread_id IS NOT NULL;
        GET DIAGNOSTICS touched = ROW_COUNT;
        EXIT WHEN touched = 0;
    END LOOP;
END $$;

-- Every allowed room gets a state row. read_sequence starts at the room's
-- best-known coverage: the highest sequence at or below the furthest cursor
-- any of the room's sessions has advanced. Rooms with no presence start at 0
-- so no room wakes mailbox-blind about history it never saw.
INSERT INTO hallway_room_state (hallway_id, room, read_sequence)
SELECT
    ar.hallway_id,
    ar.room,
    COALESCE((
        SELECT MAX(m.sequence)
        FROM hallway_messages m
        WHERE m.hallway_id = ar.hallway_id
          AND m.id <= COALESCE((
              SELECT MAX(p.read_cursor)
              FROM hallway_presences p
              WHERE p.hallway_id = ar.hallway_id AND p.room = ar.room
          ), 0)
    ), 0)
FROM hallway_allowed_rooms ar
ON CONFLICT (hallway_id, room) DO NOTHING;

COMMIT;
