# presence

Presence owns the shared domain vocabulary and validation used by the frame and turn capabilities.

- `PresenceAuthority` keeps Canon, identity, memory, lesson, Anamnesis, Paper Boat, and inference distinct.
- `PresenceMaterial` binds one body to its authority, role, source identity, and salience.
- `PresenceDirective` keeps enact, avoid, and guard criteria separate.
- `PresenceFrame`, `PresenceContract`, `PresenceReceipt`, and `PresenceCloseMaterial` are versioned domain results.
- Shared validation bounds every field, list, source reference, and digest.
- This crate performs no input, output, storage, model inference, or session-state work.
