# delivery

The walking loop of the crane. Every crane mechanic lives in `origami::cranes`. This crate only walks it.

### the loop (lib.rs)

- `DeliveryService` holds one store, one broker, and one lease owner.
- `publish_once` claims one outbox row, parses the payload, checks it against the row, publishes, then marks the row published.
- The mark happens only after the publish acknowledgement. A lost mark lets the lease expire, and PostgreSQL replays the same message id.
- A payload that fails to parse becomes a dead letter, with a classified reason.
- Row columns that disagree with the payload give `record_mismatch`.
- A publish error schedules the retry, or it ends the row at attempt 10.
- `consume_once` drains the boat.ready lane first, then the addressed crane lane. Each fetch takes one message.
- The subject must own the message, and the envelope must belong to that subject. A mismatch terminates the message.
- An addressed envelope on the wrong addressed subject gives `recipient_mismatch`.
- An expired crane terminates before the ledger sees it.
- After the receipt commits, the boat.ready lane publishes the sanitized projection, then acknowledges twice.
- The projection goes out on an insert and on a replay, so a death in that gap heals without a second row.
- A poison error terminates the message: `receipt_conflict`, `record_mismatch`, or `integrity_mismatch`.
- Delivery count 5 terminates as `delivery_exhausted`. Any other error asks for a retry with the shared backoff.
- `once` runs one publish and one consume, and it reports both.
- `run` repeats that pair forever. Two idle results give a sleep of 250 ms.
- `health` joins the database counts and the broker counts, and it names PostgreSQL as the authority.

### the command line (main.rs)

- The commands are `configure`, `publish-once`, `consume-once`, `once`, `run`, `health`, and `help`.
- `DATABASE_URL` is required. `SOLARISAEL_NATS_URL` defaults to `nats://127.0.0.1:4222`.
- `SOLARISAEL_DELIVERY_INSTANCE_ID` sets the lease owner. Without it, each start mints a new UUID.
- `configure` prints the two lanes: the stream, the subject, and the consumer of each.
- `run` writes a readiness file when `ATHANOR_DELIVERY_READY_FILE` is set. It writes a temporary file, then renames it.
- `run` stops on ctrl-c, removes the readiness file, and drains the connection.
- Every command prints one JSON line. An unknown command prints the configuration contract.
- Logs go to standard error. `RUST_LOG` sets the filter, and the default is `info`.

### the shims (broker.rs, model.rs, store.rs)

- Each file holds one `pub use` of `origami::cranes`. They carry no logic.
- They exist so three callers keep compiling: this crate's main.rs, tests/crane_delivery_integration.rs, and host/tests/recall_policy.rs.
- They die when those three callers name origami directly.
