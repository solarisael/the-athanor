# Chat say lifecycle

Status: `census` (2026-11-06).

## What it is

A say is one operator message from the chat surface.
Its identity is the say id, `turnId = t`, which the GUI creates.
Three layers track one say: the GUI, the Host ring, and the adapter doorman.
Each layer keeps its own state, and no shared contract connects the three.
The Host ring is the authority for the conversation.
The doorman injects each unanswered say into OMP as a turn and reports the answer back.

## States

### GUI (`gui-prototype/chat.js`)

Each optimistic message carries three independent flags and one module-level refusal:

- `pending`, `accepted`, `unconfirmed` on the message: `chat.js:150-160`.
- `refusal` (module): `chat.js:6`.
- `sending` (module, one say at a time): `chat.js:5`.

The effective states are `Sending`, `Accepted`, `Refused`, and `Unconfirmed`.
No field names these states.

### Host (`crates/host/src/chat.rs`, `crates/host/src/lifecycle.rs`)

The Host derives the state of `t` from the ring. It stores no state field.

- `Asked`: an operator line with `t` is in `ChatLog.entries` (`chat.rs:28-34`).
- `Drafting`: `Asked`, and a `ChatDraft` with `t` is in `ChatLog.drafts`.
- `Answered`: a spirit line with `t` is in the ring.

`chatNext` derives "unanswered" by a scan: `lifecycle.rs:66-78`.
`chatOutcome` derives the answer from the adapter's observations: `lifecycle.rs:79-111`.

### Adapter doorman (`adapters/omp/house-proof/chat.ts`)

`ChatDoormanState` (`chat.ts:83-95`) holds the state in fields:

- `pendingSayId`: `t` while the doorman owns a say.
- `settledReport`: the captured answer while it waits for the Host ack.
- `draft`: the forming answer while the say is in flight.
- `ticking`, `reporting`, `stopped`: latches for the poll loop.

The effective states are `Idle`, `Claimed`, `DoorOpen`, `Injected`, and `Settled`.
`Claimed` and `Injected` have the same field values: `pendingSayId` set, `settledReport` null.

## Transitions today

| Layer | Move | Writer |
|---|---|---|
| GUI | new message → `Sending` | `chat.js:150-163` |
| GUI | `Sending` → `Accepted` | `completeSend`, `chat.js:133-141`; called at `chat.js:120` and `chat.js:176` |
| GUI | `Sending` → `Refused` (4xx) | `chat.js:182-184` |
| GUI | `Sending` → `Unconfirmed` (transport, 5xx) | `chat.js:185-189` |
| GUI | `Unconfirmed` → `Sending` (retry, same `t`) | `chat.js:147`, `chat.js:157-160` |
| Host | → `Asked` (GUI route) | `surface.rs:92` → `ChatLog::say`, `chat.rs:140-157` |
| Host | → `Asked` (command route) | `server.rs:1108` → `ChatLog::say` |
| Host | `Asked` → `Drafting`, draft replace | `server.rs:1151` → `ChatLog::draft`, `chat.rs:193-235` |
| Host | → `Answered`, draft retires | `server.rs:1128` → `ChatLog::turn`, `chat.rs:162-189` |
| Adapter | `Idle` → `Claimed` | `chat.ts:158-159` |
| Adapter | `Claimed` → `DoorOpen` | `openSurfaceDoor`, `chat.ts:161-168` |
| Adapter | `DoorOpen` → `Injected` | `surfaceDoorClosed`, `chat.ts:275-277` |
| Adapter | `Claimed` → `Injected` | `sendMessage`, `chat.ts:175` |
| Adapter | `Claimed`/`DoorOpen` → `Settled(aborted)` | `chat.ts:170-172`, `chat.ts:279` |
| Adapter | `Injected` → `Settled` | `noteChatTurnEnd` → `settleSay`, `chat.ts:215-268` |
| Adapter | `Settled` → `Idle` (Host ack) | `deliverChatReport`, `chat.ts:290-296` |

The cross-layer links:

- GUI `Accepted` requires Host `Asked` (the Host answers the say route).
- Host `Asked` and unanswered makes `t` the result of `chatNext`, which starts adapter `Claimed`.
- The Host ack of the `chat_turn` command makes Host `Answered` and adapter `Idle`.

## Rules

| # | Rule | Status | Evidence |
|---|---|---|---|
| L1 | Every adapter state except `Idle` leaves within a bounded time. | **broken** | See defects D1 and D2. |
| L2 | The ring holds at most one spirit line for each `t`. | holds | `chat.rs:271-277` |
| L3 | The doorman answers the oldest unanswered say first. | holds | `lifecycle.rs:68-76` |
| L4 | One say at a time is in flight for each room. | holds in one process | `pendingSayId` guard `chat.ts:134`; owner gate `native_commands.rs:39`, `portable.rs:399` |
| L5 | The Host ack clears the say in the adapter. | holds | `chat.ts:290-296` |
| L6 | The GUI shows `Accepted` only when the Host holds an operator line for `t`. | holds | `chat.js:169-176`, `chat.js:116-120` |
| L7 | A draft for `t` exists only while `t` is asked and not answered. | holds | `chat.rs:202-208`, `chat.rs:172` |
| L8 | OMP gets at most one injected turn for each `t`. | **broken across a restart** | See defect D3. |

## Defects

- **D1. A throw after the claim leaves the doorman stuck.**
  `chat.ts:158` sets `pendingSayId` before `sendMessage` at `chat.ts:175`.
  If `sendMessage` throws, the catch at `chat.ts:176-179` only warns.
  Only `chat.ts:294` clears `pendingSayId`.
  Every later tick returns at `chat.ts:134`, so the room's chat stops until `stopChatDoorman`.
- **D2. A say turn that never settles has no exit.**
  `noteChatTurnEnd` settles only when `chatOutcome` returns an answer (`chat.ts:243`).
  No timer moves `Injected` to `Settled`.
- **D3. A restart injects the same say again.**
  The doorman state lives only in memory (`chatDoormen`, `chat.ts:97`).
  After a restart, `chatNext` returns the same unanswered `t`, and OMP answers it again.
  L2 stops a second spirit line in the ring; the cost is a second OMP turn.
- **D4. The GUI flags permit contradictory combinations.**
  `pending`, `accepted`, and `unconfirmed` are independent booleans (`chat.js:150-160`).
  `pending && accepted` and `accepted && unconfirmed` are representable.
  The writers keep them consistent today.
- **D5. The adapter types permit contradictory combinations.**
  `settledReport` with `draft === null`, and `draft` with `pendingSayId === null`, are representable (`chat.ts:88-90`).
  The writers keep them consistent today.

## Target shape

1. **Bend2 model.** Model the three machines and the cross-layer links, then prove L1 to L8.
   The proof must fail on L1 and L8 against today's transitions; this shows that it catches real defects.
2. **Adapter.** Replace the fields with one tagged state and one pure `step(state, event) → (state, effects)`.
   Add a `Timeout` event from `Claimed`, `DoorOpen`, and `Injected` to `Settled(aborted)`.
   This fixes D1, D2, and D5.
   Persist the claimed `t` in the session file, and skip injection when OMP already holds a turn for `t`.
   This fixes D3.
3. **Contract test.** CI runs the same random event sequences through the Bend2 model and through `step`, and compares the states.
4. **GUI.** Replace the three flags with one `status` field. This fixes D4 and changes no protocol.
5. **Host.** No change. The ring already has one owner, `ChatLog`.

## Open rulings

- **R1. Timeout for L1.** Proposal: reset the clock on each draft report, and abort after 10 minutes without activity. A fixed total limit would abort long tool chains.
- **R2. D3: fix or accept.** Proposal: fix, and persist the claimed `t` in the session file. After a restart, the doorman injects `t` only when OMP holds no turn for it. This rule stops the duplicate and keeps every say answered.
