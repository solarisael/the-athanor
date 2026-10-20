BEGIN;

-- Old omitted root requests hashed 4 but stored 10 after migration 0031.
-- Match that digest before changing it. Explicit budgets and children stay unchanged.
-- sea::idempotency_digest prefixes each UTF-8 part with its eight-byte big-endian byte length.
WITH candidates AS (
    SELECT k.knock_id, k.max_turns,
           string_agg(
               int8send(octet_length(convert_to(part, 'UTF8'))::bigint)
                   || convert_to(part, 'UTF8'),
               ''::bytea ORDER BY ordinal
           ) AS prefix
    FROM hallway_knocks k
    JOIN hallway_channels c ON c.id = k.hallway_id
    CROSS JOIN LATERAL unnest(ARRAY[
        c.hallway_key, k.from_room, k.from_spirit, k.request_session,
        k.message_id::text, k.recipient_room, ''
    ]) WITH ORDINALITY AS parts(part, ordinal)
    WHERE k.parent_knock_id IS NULL
      AND k.root_knock_id = k.knock_id
      AND k.turn_index = 1
      AND k.max_turns = 10
    GROUP BY k.knock_id, k.max_turns
)
UPDATE hallway_knocks k
SET request_digest = encode(sha256(
    c.prefix || int8send(octet_length(c.max_turns::text)::bigint)
        || convert_to(c.max_turns::text, 'UTF8')
), 'hex')
FROM candidates c
WHERE k.knock_id = c.knock_id
  AND k.request_digest = encode(sha256(
      c.prefix || int8send(1::bigint) || convert_to('4', 'UTF8')
  ), 'hex');

COMMIT;
