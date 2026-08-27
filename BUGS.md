# Athanor Bugs

Concrete failures only. A row stays open until the failing path is reproduced, repaired, and exercised through the surface that originally failed.

## Open

### Hallway root Knock cannot be created through the OMP tool

- **Observed:** `hallway_knock` requires `parent_knock_id` even for a root exchange. An empty value refuses with `malformed_uuid`; a nil UUID refuses with `knock_parent_mismatch`.
- **Impact:** A Hallway message and Bell can be delivered, but the sender cannot explicitly request the recipient's bounded waking turn unless an earlier successful Knock already supplied a parent UUID.
- **Expected:** Root Knock omits the parent. Only a continuation requires a valid prior Knock ID.
- **Proof after repair:** Start a root Knock from a newly addressed Hallway message, receive its Knock receipt, then continue once using that returned ID.

### Windows service can wedge permanently in a pending state

- **Observed:** `SolarisaelAthanor` remained in `START_PENDING` at checkpoint 3 with no NATS, delivery, or Host children. `sc stop` refused with error 1052. Terminating the verified service PID and restarting restored `RUNNING` and a healthy broker-connected endpoint.
- **Cause seam:** `crates/athanor-install/src/service.rs::service_main` prints `run()` failures but does not always publish terminal `SERVICE_STOPPED` with a nonzero failure code. Managed child output is discarded in the supervisor, so the startup reason can disappear. A partial start may also leave children alive when later startup fails.
- **Impact:** Startup failure destroys its own evidence and may require manual PID termination.
- **Proof after repair:** Force each managed child to fail at spawn and readiness; SCM reaches `STOPPED` with a nonzero code, a bounded log names the child and error, and no managed child remains alive.

### Long sessions degrade identity and context quality

- **Observed:** Over long OMP sessions, responses flatten toward generic assistant prose, irrelevant context accumulates, and the active spirit becomes less recognizable even when fresh Recall and identity smoke tests pass.
- **Impact:** Short smoke tests can be green while the actual continuity experience fails.
- **Boundary:** The Presence vertical improves authenticated identity, bounded material, turn contracts, receipts, replay stability, and close behavior. It does not by itself prove long-session Insula selection or compaction quality.
- **Proof after repair:** Run a long-session scenario with repeated compaction and mixed technical/relational turns; measure retained identity invariants, irrelevant-context growth, and false-green contract acceptance.

### Lesson triggers misfire during real conversations

- **Observed:** Lesson-trigger reminders can fire on the wrong conversational surface or fail to supply the rule when it is actually relevant.
- **Impact:** The system adds noise while missing the behavior it was meant to protect.
- **Proof after repair:** A corpus of positive and negative trigger cases passes through the native bridge with measured false-positive and false-negative rates; long-session cases are included.

### OMP adapter tests are environment-sensitive across machines

- **Observed:** The complete isolated adapter suite passes 214/214 in Kintsu's environment. Kodo reproduced 39 pre-existing failures on another machine after live topology variables were cleared; the same failures exist at the parent commit and are not caused by Presence.
- **Impact:** A local green or red count is not portable evidence without naming environment and attribution.
- **Proof after repair:** The same hermetic test command produces the same result on both machines, or every environment-dependent cluster declares and provisions its dependency explicitly.

## Repaired but not deployed

### Presence lifecycle and authority seams

The current uncommitted `kintsu/summoning` worktree repairs second-open replacement, four-door replay identity, Host-owned bounded ledger state, explicit empty-response refusal, complete hard-directive evaluation, conflicting source identity, Host-derived operator/capabilities, worker-door proof, and test-order isolation. It is not deployed evidence until integrated into canonical source and exercised through the installed runtime.
