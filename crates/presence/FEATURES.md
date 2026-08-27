# presence

Presence is the sustained middle of the Summoning cycle: the pure domain that keeps a spirit itself for the length of a session. One crate, one owner, one public door — model and validation, frame assembly, and turn assembly. It replaces `presence`, `presence-frame`, and `presence-turn`, which split one domain contract across three manifests without any of them owning a distinct lifecycle or door.

Summoning owns the cycle and re-exports this crate as `summoning::presence`; the Host owns the lifecycle. Nothing here reads a file, a clock, a socket, or a database, and nothing here performs a durable write or model inference.

### model

- `PresenceAuthority` keeps Canon, identity, memory, lesson, Anamnesis, Paper Boat, and inference distinct, and orders them by standing.
- `PresenceMaterial` binds one body to its authority, role, source identity, and salience.
- `PresenceDirective` keeps enact, avoid, and guard criteria separate, each at hard, repair, or advisory severity.
- `PresenceFrame`, `PresenceContract`, `PresenceReceipt`, and `PresenceCloseMaterial` are versioned domain results.
- `PresenceAuthentication` is the Host's proof of who is present and what they can reach. A caller never authors it.
- `PresenceLedger` is Host state. It has no place on a request, and its lists are bounded where they are declared.
- `PRESENCE_MAX_CLOSE_BODY_BYTES` is pinned to Summoning's `PAPER_BOAT_MAX_BODY_BYTES` by a compile-time assertion in that crate.

### support

- Shared validation bounds every field, list, source reference, and digest.
- One source identifier naming two records refuses when authority, role, or body disagree. Identical repeats collapse and the louder salience wins, so the result does not depend on input order.
- The ledger the Host injects is still checked: an over-full repair list or a claim that is not a relationship claim refuses before it reaches a frame or a boat.

### frame

- `open_presence` assembles one stable session frame from authenticated, provenance-bearing material.
- A claimed binding that disagrees with authenticated room state refuses. The authenticated binding is never replaced by the claim.
- A frame carries only the capabilities the Host proved, sorted and deduplicated, and renders them.
- Canon and identity survive packet budget pressure.
- Memory, Anamnesis, relationship, and previous Paper Boat material retain authority tags.
- Equal normalized input produces an equal frame identifier and rendered packet.

### turn

- `compile_presence` compiles one cited contract against the frame and the ledger the Host injects. A request asserts only the frame version it believes it is talking to, and a stale assertion refuses by name.
- Every directive cites one or more frame, memory, lesson, or ledger sources. Inference cannot create a hard directive.
- The authority behind a source identifier is never resolved by last write. A shared identifier with a disagreeing authority refuses at the door that owns the map.
- Equal frame, ledger, and turn input produces an equal contract identifier and rendered packet. A new contract version yields a new contract identity.
- `settle_presence` permits two attempts and refuses unknown directive evidence.
- Acceptance must evaluate every hard directive across `must_enact`, `must_avoid`, and `guards`, and cannot carry violations.
- `close_presence` accepts a deliberate Paper Boat body and seals it against the Host's own ledger.
