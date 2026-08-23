# origami

The message shapes of the House. PostgreSQL holds every body. NATS carries only pointers.

### cranes

- A crane moves a pointer to a destination. The payload never carries a body.
- The outbox keeps each pointer in PostgreSQL. A lease claims it. A publish acknowledgement marks it published.
- The receipt ledger commits before the caller acknowledges. A redelivery replays the recorded outcome.
- Two lanes exist: the boat.ready lane, and the addressed crane lane.
- The broker declares the streams, the subjects, and the durable consumers. It refuses a live configuration that differs.
- Failure has bounds. Ten publish attempts, then a dead letter row with a reason.
- house-delivery walks these shapes. Origami never calls back into it.

### hallways

- A hallway carries letters between rooms. PostgreSQL holds them. The Host projects them.
- No NATS lane runs behind a hallway. That transport does not exist yet.
- A hallway has a key, allowed rooms, presences, messages, threads, and notifications.
- A knock asks one room for one bounded turn. The recipient room decides by its own policy.
- Every command takes an idempotency key. A reused key with different content gets a refusal.

### boats

- A boat holds the state of a room at sleep. The next wake reads it back.
- The wake returns the newest boat, then lists the memories written after it.
- The name of a boat is a digest of the kind, the room, and the body.
- Bounds protect the reader. The body, the title, and the list clip, and each clip adds a warning.
- The insert stays in the substrate remember path. This module only plans it.

### sea

- `payload_digest` names content with SHA-256.
- `idempotency_digest` prefixes each part with its length, so the parts cannot be rearranged.
- `subject_owns` decides subject ownership. A filter that ends with `.>` owns only deeper subjects.
