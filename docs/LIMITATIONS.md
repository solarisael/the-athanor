# Limitations

Status: current. Verified against the code at commit `a6ab453` on 2026-10-04. Each limit names the code that imposes it. The architecture that these limits belong to is in [`ARCHITECTURE.md`](./ARCHITECTURE.md). The work that removes them is in [`ROADMAP.md`](./ROADMAP.md).

## 1. Supported install

- The only supported target is Windows 11 x64 with OMP as the harness. The release manifest requires platform `windows-x64` (`crates/athanor-install/src/manifest.rs:104-106`). The release workflow has one job, `windows-x64` (`.github/workflows/release.yml:17-18`).
- Installation requires Administrator (`installer/athanor.iss:17`).
- The installer bundles PostgreSQL 18.4-2, pgvector 0.8.6, and NATS 2.14.4 (`installer/dependencies.json`). It needs no WSL, Python, Bun, Cargo, or Rust at run time.
- The installer does not carry the Pulse window. `scripts/install-pulse.ps1` builds and places `pulse.exe` separately (`scripts/install-pulse.ps1:18-61`).
- The product version is `0.5.4` (`package.json:3`). Older labels such as `0.9.6` and `1.0.0-rc.3` are history.

## 2. Other platforms

| Platform | State |
|---|---|
| Windows 11 x64 | installer target and reference workstation |
| Windows 10 x64 | installer target, not re-proved for the current source |
| Linux | the substrate, Host, adapter, and keeper are portable in source; the Host cannot start, see 3 |
| macOS | unsupported |
| Harnesses other than OMP | unsupported; the adapter registry refuses any `driver` field (`crates/athanor-install/src/harness/config.rs:44-45`) |

## 3. Linux today

- The Host cannot start. `athanor` with no arguments calls `service::ensure_running` before it binds, and the non-Windows path fails (`crates/athanor-install/src/app.rs:88`; `service.rs:319-322`). `athanor start` reports the Host refused after 20 seconds (`cli/start.rs:119-137`).
- The installed OMP loader requires platform `windows-x64` and `USERPROFILE` (`adapters/omp/installed-loader.ts:376,565`). A Linux session must load `adapters/omp/index.ts` directly.
- Fixed Windows paths. `serve.ts:17-18`, `gui-desktop/src/main.rs:54-56`, and `adapters/omp/rust-transport.ts:60` read `C:/ProgramData/Solarisael/Athanor/...`. On Linux the proxies refuse to start and the adapter fills no NATS URL.
- The service, the installer, ACL hardening, and the release are Windows only. Off Windows, ACL hardening reports success and does nothing (`crates/athanor-install/src/boundaries.rs:377-380,431-435`).
- The keeper lock does not exclude a second keeper off Windows, and a kill reaches only the direct child (`crates/omp-keeper/src/keeper.rs:82-87,128-136`).
- The automatic Recall timeout is 2,000 ms off Windows and 8,000 ms on Windows (`adapters/omp/house-proof/constants.ts:16`).
- A headless relaunch never verifies or continues a restart. The successor returns silently unless `ctx.mode === 'tui'` (`adapters/omp/house-proof/restart-door.ts:481`).

The substrate has run on Linux before: the organ matrix of 2026-08-30 ran on a NixOS laptop. That run used hand-written units, not this code path.

## 4. Identity

- No person identity exists anywhere. One bearer token serves every room of a House and proves reach, not identity (`crates/host/src/house.rs:205-234`; `server.rs:282,301-323`).
- A chat say is `{room, text, say_id}`. The Host stamps `author_name` from the room file's single `operator` (`crates/protocol/src/host.rs:743-748`; `crates/host/src/surface.rs:85`). Two people in one room get the same name.
- `CommandMeta` has `sender_room`, `sender_spirit`, and `sender_session`, and no `sender_operator`. `sender_session` is caller-chosen (`protocol/src/host.rs:789-809`; `server.rs:2567-2581`).
- Each room has exactly one operator. The adapter derives room, spirit, and operator from the working directory and room files, and `set_room_state` rewrites them (`adapters/omp/house-proof/room.ts:99-127`; `tools.ts:818-847`). This is self-asserted identity.
- Presence tells the spirit to "meet `<operator>`" and the boat door names `Sol` unless a room overrides it (`presence.ts:130,199`; `boat-door.ts:44-67`).
- Pulse has no login. The proxy injects the shared bearer for any local caller (`serve.ts:60-63`; `gui-desktop/src/proxy.rs:99`).

## 5. Rooms and sessions

- One Host process serves every room. One Pulse process serves one room (`serve.ts:34,41`; `proxy.rs:32`). `app.js` holds one connected room (`app.js:1514-1528`).
- Adding a room means building and installing a new release: full payload copy, database backup, service stop and start, migration. The installer refuses an already-installed version and never writes `harnesses.json` (`crates/athanor-install/src/installer.rs:217-223,240-274`; `cli/manage.rs:32-134`). Hand edits plus a Host restart work.
- The chat ring and drafts are lost on Host restart, and the sequence restarts at 0 (`crates/host/src/chat.rs:6-7,25-30`; `server.rs:218`).
- No list-sessions or resume-session command exists on the wire (`crates/protocol/src/restart/mod.rs:36-38`). Pulse shows `New session unavailable` (`app.js:741`). The keeper's first spawn is always fresh (`keeper.rs:188,812-823`).
- Pulse mirrors only chat-born turns. Turns typed in the OMP terminal never enter the ring.
- With no logged-in Windows user, only PostgreSQL and NATS run. The Host, the keepers, OMP, and Pulse are user processes (`cli/start.rs:119-125`; `harness/config.rs:25-29`).

## 6. Network

- Everything binds loopback. The Host refuses a non-loopback bind (`crates/host/src/config.rs:92-120`). The adapter refuses a non-loopback Host URL (`host.ts:88-90`). Both proxies bind `127.0.0.1`.
- No TLS anywhere. A public door needs a TLS reverse proxy in front of the Pulse proxy.
- The Pulse proxies check no `Origin` or `Host` header and set no CSP (`proxy.rs:131-157`; `gui-desktop/tauri.conf.json:8`). A remote page open in a local browser can send simple POSTs to `/live/chat/say` or `/local/repair/start`, which raises a UAC prompt. Fix this before any network exposure.
- `/health` is unauthenticated and exposes `state_hash`, version, sequence, and Insula health (`server.rs:252,261-275`).
- The bearer is checked once at the WebSocket upgrade. Frames are not re-authenticated (`server.rs:282,432-483`).
- The harness control door cannot be reached from outside the Host process. Its token is random per run and never exported (`app.rs:92`; `harness/control.rs:17-18`).

## 7. Doors that need the Host

Six tools fail without a live Host: `recall`, `house_lane_status`, `familiar_status`, `familiar_dispatch`, `house_dispatch`, `recall_policy` (`adapters/omp/house-proof/tools.ts:526-532`). `sleep` degrades. Knock claim and delivery and the inbox Bell projection need the Host (`knock.ts:77-114`; `hallway.ts:26-30`). All seven `hallway_*` tools and all five `quest_*` tools go to the substrate child and work without the Host.

Retrieval is fail-open for PostgreSQL and embeddings. It is not fail-open for a missing Host: `recall` discards the substrate result and returns an error (`tools.ts:516,526-532`).

## 8. Known defects in the current code

- `LogConversation` creates directories and appends files under a caller-chosen `room_dir` with no check (`server.rs:2117-2127,2131-2235`). The comment at `server.rs:2064-2066` claims otherwise.
- `room/state` returns every presence `session` and `operator` to any bearer holder (`surface.rs:118-121,133`).
- Pulse sediment sends `room = item.room` for foreign shelves (`gui-prototype/sediment/index.js:112`). Host enforcement of that room is unknown.
- `serve.ts:3-6` says the prototype writes nothing. `/live/chat/say` writes a chat turn.
- `pulse.js` and `mechanics.js` show fixtures dated 2026-08-20 and 2026-08-18 until a live round answers.

[`BUGS.md`](../BUGS.md) tracks each of these.

## 9. Retrieval boundary

The House retrieves bounded evidence. It does not load an entire archive into every prompt.

Automatic retrieval is narrower than explicit `recall`. Low-information turns may retrieve nothing. Explicit recall remains available for deliberate archive investigation.

Semantic proximity is a candidate signal, not factual authority. Important answers follow the cited source and its authority state. Imported corporate or project documents require an explicit source-precedence policy.

## 10. Context-budget boundary

The adapter bounds each context organ independently. Room context, tool schemas, a fresh paper boat, Anamnesis counsel, active lessons, automatic recall, canon, thread neighbors, directives, and growth nudges each have their own rules. No single coordinator assigns one turn budget across all of them. Several valid organs can stack into a context surface larger than a short task warrants.

The Athanor has not established that retrieval and continuity reduce total input tokens or total task cost against a no-Athanor baseline. Treat net efficiency as an evaluation question, not a product claim.

## 11. Memory boundary

The Athanor is not indiscriminate transcript storage. Durable memory is deliberate by default.

- Events and realizations belong in memories.
- Transferable engineering rules belong in coding lessons.
- Project-bound rules belong in project lessons.
- Current state can supersede older current state.
- Narrative history remains recoverable.
- Secrets belong in a secret manager, never in memory.

The Athanor can preserve a wrong interpretation if an operator or agent records it. Correction and supersession make the trail repairable. They do not remove the need for judgment.

## 12. Identity boundary

The House preserves and loads an identity contract. It does not prove metaphysical identity, consciousness, or equivalence between model providers.

A room keeps names, voice, commitments, corrections, and shared history across model changes. Different models express the same contract with different capability, style, or reliability.

Identity prose is co-authored. The installer does not manufacture intimacy or a personality on the operator's behalf. A personality package is starting material. A model or LoRA is a replaceable body, not proof of identity.

## 13. Provider boundary

A local House does not make the model provider local. Context sent to a hosted model is processed under that provider's terms.

Local embeddings keep archive vectorization off a hosted service. They do not prevent selected memory context from reaching the active model provider.

The Athanor cannot remove provider rate limits, model policies, outages, or capability differences. Several OMP sessions on one subscription share that subscription's limits.

## 14. Organizational boundary

The room model is not an enterprise authorization system. The `house` scope is one shared commons with no per-source permissions. Authorization does not run before retrieval and ranking.

A central multi-user deployment requires tenant, team, project, and private scopes; authorization filtering before ranking; source provenance and versioning; retention and deletion policy; auditability; administrative controls; tested connectors. Do not place a company's private corpus behind shared retrieval until those controls exist and are verified. See the OMEGA section of [`ROADMAP.md`](./ROADMAP.md).

## 15. Non-goals

The Athanor does not replace Git for source history, a secret manager for credentials, object storage for large binaries, human judgment over consequential memories and lessons, the AI harness that executes models and tools, or knowledge interfaces such as Obsidian. The House coordinates continuity and retrieval across those systems.
