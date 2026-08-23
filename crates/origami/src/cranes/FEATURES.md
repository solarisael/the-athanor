# origami::cranes

Movement with a destination. A crane carries a pointer, never a body.

### outbox

- `Store` wraps one PostgreSQL pool. A connect refuses a schema older than version 17.
- `claim_next` leases one due row with `FOR UPDATE SKIP LOCKED`. The lease lasts 30 seconds, and the attempt count rises.
- `claim_next` also takes back a leased row when its lease expired.
- `mark_published` writes the published state only while the lease still belongs to the caller.
- `mark_publish_failure` schedules the retry: 1, 5, 15, 30, 60, 120, 300, then 600 seconds.
- Attempt 10 ends the row. The row becomes a dead letter, and the payload goes with it.
- `record_receipt` locks the receipt row, verifies the pointed record, inserts once, then commits before the caller acknowledges.
- A redelivery reads the recorded receipt and replays it. The caller learns `Inserted` or `Replayed`.
- A replay with different pointer data fails with `receipt_conflict`.
- A pointer to a missing record fails with `record_mismatch`.
- The boat.ready lane also compares the body digest, and a difference gives `integrity_mismatch`.
- A dead letter carries the reason code, the payload digest, and the payload size. A repeat only updates the time and the count.
- `health` counts the pending, leased, and published rows, the dead letters, and the receipts.

### broker

- `connect` opens one NATS client and one JetStream context.
- The wire names are compiled constants: two lane streams, one receipt stream, the subjects, and the durable consumers.
- The receipt stream name and subject come from house-protocol, which is their one declaration.
- `configure` gets or creates each stream and consumer. It then compares 14 stream fields and 12 consumer fields, and it fails on any difference.
- Each stream holds files, denies delete, and denies purge. The limits are 100000 messages, 512 MiB, and 7 days.
- One message holds at most 4096 bytes. The duplicate window is 24 hours.
- `publish` sets `Nats-Msg-Id` to the event id, so JetStream collapses a retry.
- `publish_receipt` sends the sanitized projection on the receipt subject.
- Each consumer acknowledges explicitly. The wait is 30 seconds, the limit is 5 deliveries, and the backoff runs from 30 to 600 seconds.
- The lane streams accept one consumer each. The receipt stream accepts 64.
- `health` reports the pending, unacknowledged, and redelivered counts of both lanes.

### envelope

- `CraneEvent` is the pointer: schema version, event id, kind, record id, room, time, and body digest.
- Serde refuses an unknown field. `parse` validates every field after it reads them.
- The record id must be a positive decimal number. The room holds 1 to 128 bytes.
- The digest must be 64 lowercase hexadecimal characters.
- The recipient kind and the recipient key must arrive together, or neither arrives.
- The boat.ready lane refuses addressing, expiry, and lineage fields. Every other kind must name its recipient.
- An event kind holds two or more lowercase segments, and at most 128 bytes.
- `classify_invalid_payload` names the refusal: `malformed_payload`, `private_payload`, or `unknown_event`.
- A payload with a body, title, conversation, message, or content key reads as private, at any depth.
- `is_expired` compares the expiry with now. `event_id_hint` recovers an id from a payload that failed to parse.

### lanes

- A `Lane` is `BoatReady` or `Addressed`.
- `subject()` writes the NATS subject. `from_subject` reads one back, or returns nothing.
- A recipient kind is worker, familiar, room, or reviewer. The name crosses the wire as snake case.
- A recipient key holds 1 to 64 characters: lowercase letters, digits, underscore, and hyphen.
- A recipient key starts with a lowercase letter or a digit.
