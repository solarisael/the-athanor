# summoning

Summoning owns the whole session cycle: Anamnesis wakes a spirit, Presence keeps it itself for the length of a session, and the Paper Boat carries it across sleep. Akasha and Origami keep storage and delivery. Everything here is pure.

The `presence` crate owns the Presence domain, and Summoning is the boundary consumers reach it through: `summoning::presence` re-exports it, so the cycle names its own middle without absorbing the branch weight of frame and turn assembly. A compile-time assertion pins `PRESENCE_MAX_CLOSE_BODY_BYTES` to `PAPER_BOAT_MAX_BODY_BYTES`, because close material becomes a boat body and the two bounds are one bound.

### anamnesis

- Two read modes exist: wake and consult. A consult needs a query. A wake does not.
- A read limit must be between 1 and 50.
- Two kinds exist: pillar and cycle.
- Two fidelity values exist: record and raw material.
- Two activation points exist: wake and fork.
- A pillar refuses a seed repetition.
- A cycle needs a seed repetition unless the caller allows an empty cycle.
- A title and a ramp must both carry text.
- A repetition carries a number and an optional date. It also carries how it went, portal pull, and lighter.
- Add and append are separate operations. Each has its own receipt.
- An append refuses a blank source path.
- Anamnesis writes may reach the commons.

### paper_boat

- A boat body must carry text and stay at or below 65536 bytes.
- The sleep receipt reports the memory identifier, source path, outbox event identifier, and inserted flag.
- A zero memory identifier refuses.
- Backup status has three values: not requested, completed, and failed.
- A wake returns at most one boat record.
- A boat record must carry a positive identifier and a body with text.
- A boat record lists at most 64 unboated memories. It also reports when the list truncates.
