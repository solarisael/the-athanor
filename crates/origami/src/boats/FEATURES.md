# origami::boats

Stasis with a return point. A boat waits in the Sea until the room wakes.

### wake

- `wake(pool, room)` returns the newest paper boat of the room, or nothing.
- The order is `created_at DESC, id DESC`. One row comes back.
- `unboated_tail` lists the memories that arrived after that boat. The keyset is `(created_at, id)`.
- The tail stops at `PAPER_BOAT_MAX_UNBOATED` records, and it reports the cut with a flag.
- The body clips to `PAPER_BOAT_MAX_BODY_BYTES`. Each clip adds a warning.
- A boat with no title reads as "untitled". A row identifier of zero or less is an error.
- The result carries the record and the warnings together.

### sleep

- `plan(room, body, now)` builds the source path, the title, the date, the thread, and the metadata.
- The title is `paper boat — ` and the date. The thread key is one canonical string.
- The metadata records the origin, the time of the record, and the identity digest.
- `ready_pointer(tx, memory_id)` reads the event id that the 0016 trigger put into `crane_outbox`.
- That read runs inside the transaction the caller still holds. This is the one place a boat touches a crane.
- `write_boat_tx` is a `todo!`. The insert stays in the generic memories write in remember.rs.

### identity

- `source_identity(room, body)` returns `db-only/paper-boats/sha256-<digest>.md`.
- The digest covers the kind, the room, and the body. A NUL byte separates the three parts.
- `digest_of(source_path)` reads the digest back out of the path.
- Two tests pin the digests byte for byte. One changed byte renames every boat in the database.

### the module door (mod.rs)

- `MEMORY_KIND` is `paper-boat`. Every query filters on it.
- `EVENT_KIND` is `boat.ready`. `CREASE_PATTERN` is `boat.ready.v1`.
- `THREAD_KEY` and `SLEEP_ORIGIN` name the thread and the origin of a sleep.
- The strings stay bare until the kind registry arrives. Consumers then route on behavior.

### record

- `positive_id` refuses a zero or negative database identifier.
- `bounded_utf8` cuts on a character boundary, and it reports the cut.
- Four limits guard the receipt: title 512, source path 2048, kind 128, warning 4096 bytes.
- The substrate applies the warning limit. This module applies the other three.

### error

- `BoatError` has two shapes: `Invalid` and `Database`.
- A sqlx error converts into `Database`, and the source stays readable.
- `BoatResult<T>` is the return type of every fallible call here.
