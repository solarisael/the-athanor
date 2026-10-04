# origami

The message shapes of the House. PostgreSQL holds every body. NATS carries only pointers.

Not re-verified at a6ab453. The 2026-10-04 census did not walk `crates/origami`; the claims below keep their earlier date, except the hallway note.

Each module keeps its own feature list:

- [`src/cranes/FEATURES.md`](./src/cranes/FEATURES.md)
- [`src/hallways/FEATURES.md`](./src/hallways/FEATURES.md)
- [`src/boats/FEATURES.md`](./src/boats/FEATURES.md)

The Host consumes a Hallway JetStream subject (`crates/host/src/server.rs:2855-2894`), so a NATS lane does run behind a hallway today. The hallways list carries the detail.

### sea

- `payload_digest` names content with SHA-256.
- `idempotency_digest` prefixes each part with its length, so the parts cannot be rearranged.
- `subject_owns` decides subject ownership. A filter that ends with `.>` owns only deeper subjects.
