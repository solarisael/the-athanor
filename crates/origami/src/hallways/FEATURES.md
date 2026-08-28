# origami::hallways

Room letters. PostgreSQL holds every hallway, and the Host projects it. No NATS lane runs behind these calls.

### messages

- `post` appends one message to a hallway, inside one transaction.
- One `UPDATE` with `RETURNING` takes the next sequence, so a torn write cannot skip or share a number.
- `toRooms` may only name rooms that the hallway allows.
- `replyTo` must name a message of the same hallway. The reply inherits the thread of that message.
- A message with no reply joins today's thread. The date comes from PostgreSQL in the house timezone.
- An unknown timezone gives a config error, and it names `ATHANOR_HOUSE_TZ`.
- A repeated idempotency key with the same body, reply, and recipients returns `Duplicate`.
- The same key with different content gets `idempotency_reuse`.
- Each post mints the notifications, then raises the revision of every allowed room.
- `read` returns the messages after a cursor, in sequence order, with a limit and a `hasMore` flag.
- `read` can filter by thread. An unknown thread gets `thread_not_found`.
- `read` moves the presence cursor only when the caller asks, and only for the rows it returned.
- A filtered thread read never moves the presence cursor.
- The room read sequence advances across a contiguous run only. A gap stops it.
- `read` also stamps the returned notifications as read, and it reports that count.
- `inbox` lists every hallway the room may open. Each entry carries the unread count, the mentions, the pending notifications, and the revision.
- Each inbox entry ends with the latest message: the room, the spirit, the time, and 160 characters of the body.

### knocks

- `policy` writes the standing knock policy of one room. The modes are `manual` and `allow_list`.
- A new policy supersedes the previous row, and it takes the next revision number.
- A policy may only name rooms that the hallway allows.
- A session that already sits in a sibling room cannot set this policy.
- A repeated key with a different policy gets `idempotency_reuse`.
- `knock` requests one bounded turn against a message that already exists.
- The message must belong to the asking room, spirit, and session.
- The recipient must be a target of that message. One message may ask one recipient once.
- The recipient policy must be `allow_list`, must name the asking room, and must permit the requested turns.
- A first knock holds turn 1 and lives 15 minutes.
- A child knock inherits the root, the turn limit, and the expiry of its parent.
- A child knock must reverse the two rooms, and it must reply directly inside the parent thread.
- A parent that never started refuses the child. An exhausted exchange refuses it too.
- Each new knock raises the notification revision of the recipient room, or the call fails.
- `claim` first fails every turn of the room that expired, then leases one due knock with `SKIP LOCKED`.
- A claim lease lasts 30 seconds. A claim only sees a knock that the current policy still allows.
- `settle` moves a claimed knock to `started`, `completed`, or `failed`.
- A start needs a live lease. A completion needs a started turn. A failure needs either.
- A repeated settle with the same outcome and reason returns duplicate.

### channels

- `create` opens a hallway with a key, an owner, and the allowed rooms. The creator becomes the first presence.
- A repeated create returns `Duplicate` only when the room, spirit, session, key, and rooms all match.
- Any other reuse gets `idempotency_reuse`.
- `join` binds a room, a spirit, and a session as one presence, and it returns the read cursor.
- A session bound to a different spirit gets `spirit_mismatch`.
- A room outside the allowed rooms gets `room_not_allowed`.
- `ensure_presence` adds a presence lazily inside a running transaction. It tries twice, then it fails.
- `lookup_id` turns a hallway key into an id, or it refuses with `hallway_not_found`.

### bells

- `mint` writes one notification for each named recipient room. A repeat changes nothing.
- `bump_inbox_revisions` raises the revision of every allowed room, and it creates the missing rows.
- `acknowledge` stamps the read time on unread notifications, and it returns the count.

### errors

- `HallwayError` has four shapes: `Invalid`, `Refusal`, `Config`, and `Database`.
- A refusal carries a static code and a static message. Callers route on the code.
- A sqlx error converts into `Database`, and the source stays readable.
- The four shapes match the substrate `AppError`, so the substrate adapter only renames them.

### gaps carried

- The substrate mounts 7 of the 9 hallway calls. `claim` and `settle` reach the House through house-host only.
- The substrate health check never reads a hallway table.
