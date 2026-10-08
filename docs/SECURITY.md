# The Athanor Security and Privacy

Status: current. Verified against the code at commit `a6ab453` on 2026-10-04. Each current-state sentence names the code that decides it. Policy sections tell operators and agents what to do. The process map is in [`ARCHITECTURE.md`](./ARCHITECTURE.md). The same limits, seen as limits, are in [`LIMITATIONS.md`](./LIMITATIONS.md).

Warning: Do not expose any Athanor port to a network. Sections 4 to 7 give the reasons.

## 1. Secrets policy

Never store these in a room, memory, lesson, paper boat, repository, or public evaluation artifact:

- API keys;
- access tokens;
- database passwords;
- private keys;
- session cookies;
- recovery codes;
- raw credential exports.

Keep your own credentials in a platform credential store or a secret manager. The product keeps its own secrets in files and in the environment; section 3 names each place.

## 2. Consent before installation

The installing agent must explain the change and get the operator's choice before it:

- requests elevation;
- installs global software;
- changes a system service;
- deletes or overwrites data;
- moves an existing room;
- changes model-provider authentication;
- imports a private archive;
- connects an external database.

What the installer does today:

- It requires Administrator (`installer/athanor.iss:17`).
- It creates the Windows service `SolarisaelAthanor` with automatic start through `sc.exe` (`crates/athanor-install/src/installer.rs:334-339`; `boundaries.rs:449-556`).
- It writes a room-state file only when the file is missing (`installer.rs:240-271`).
- It edits the OMP `config.yml` in place to add the loader. Uninstall removes that entry (`installer.rs:1343-1347,1073-1080`).
- It backs up the database before an upgrade and on a first managed install (`installer.rs:279-281,320-322`).
- Uninstall keeps the data root unless the operator runs `purge --confirm-data-loss` (`installer.rs:1063-1095`).
- `databaseMode` is `managed` or `external`. The external URL is stored in the secrets file (`installer.rs:1229-1250,1254-1286`).
- Install output prints only the version and the flags (`crates/athanor-install/src/cli/manage.rs:90-95`).
- No install path uses WSL. `substrate/deploy-local.ps1` names WSL in its refusal message, but checks only `$IsWindows` (`substrate/deploy-local.ps1:21-23`).

## 3. Where the product keeps its secrets

`D` is `%ProgramData%/Solarisael/Athanor` (`crates/athanor-install/src/layout.rs:4,22-27`).

| Place | Contents | Protection |
|---|---|---|
| `D/secrets/runtime-secrets.json` | `hostToken` and `postgresPassword` (32 random bytes each), plus `externalDatabaseUrl` (`installer.rs:1254-1286`) | Inheritance removed; full control for SYSTEM and Administrators only (`boundaries.rs:353-381`). Applied to `D`, the folder, and the file (`installer.rs:239,1259-1261,1286`) |
| `%USERPROFILE%/.omp/agent/athanor/client.json` | A copy of `hostToken`, plus `houseId`, `stateRoot`, `hostUrl`, `defaultRoom`, `rooms` (`installer.rs:1316-1340`) | An ACL for the operator user on the folder and the file (`installer.rs:1333,1338`; `boundaries.rs:383-436`) |
| `ATHANOR_HOST_TOKEN` in the OMP environment | The bearer. The loader sets it when the variable is empty (`adapters/omp/installed-loader.ts:554-556,628-634`) | None. The adapter sends it as `Authorization: Bearer` (`adapters/omp/house-proof/host.ts:43-47,153,182`) |
| Native Host configuration | Database and broker credentials remain in native execution. OMP no longer forwards its environment to substrate children. | Native configuration and existing secret ACLs |
| The Docket capability | The environment wins, else a room-capability file (`adapters/omp/house-proof/tools.ts:78-99`; `room.ts:147-155`) | Not re-verified at `a6ab453`; `adapters/omp/house-proof/room.ts` decides the file location |

Both Pulse proxies read `hostToken` from the secrets file (`gui-prototype/serve.ts:62`; `gui-desktop/src/main.rs:49-50,56`).

Native judgments can receive an approved provider credential through the authenticated loopback connection.
The credential remains transient and does not enter context caches or receipts.
The installed Host broker identity can publish Hallway pointers.
The exact subject permission is `athanor.hallway.room.>`.
Existing restrictions on unrelated subjects remain unchanged.

Warning: Off Windows, both ACL functions do nothing and report success (`boundaries.rs:377-380,431-435`).

## 4. Network

- Every listener binds loopback. The Host refuses a non-loopback bind (`crates/host/src/config.rs:92-120`). The production builder sets `127.0.0.1` (`crates/athanor-install/src/app.rs:30-58`).
- PostgreSQL listens with `-h 127.0.0.1` and NATS with `-a 127.0.0.1` (`crates/athanor-install/src/supervisor.rs:417-459`).
- The adapter refuses a Host URL that is not `ws:` on `127.0.0.1`, `localhost`, or `::1` (`host.ts:74,88-90`).
- Both Pulse proxies bind `127.0.0.1` (`serve.ts:44`; `gui-desktop/src/main.rs:63`).
- No TLS exists. `server.rs` has no TLS code (`crates/host/src/server.rs:250-258`). The adapter accepts only `ws:` (`host.ts:88-90`). The proxies forward plain HTTP (`gui-desktop/src/proxy.rs:28,32`; `serve.ts:58`).
- `GET /health` needs no bearer. It returns status, schema version, WebSocket path, version, sequence, `state_hash`, delivery health, and Insula health (`server.rs:252,261-275`).
- The harness control door uses a random token per run that is never exported (`app.rs:92`; `crates/athanor-install/src/harness/control.rs:17-18`).

A public door still needs TLS, operator authentication, and an approved origin policy. See section 7.

## 5. Host authentication

- One bearer serves the whole House. Every room must share it (`crates/host/src/house.rs:205-234`). It comes from `hostToken` (`app.rs:30-58`).
- The WebSocket upgrade checks the `Authorization: Bearer` header in constant time (`server.rs:282,301-323`). Frames are not re-authenticated after the upgrade (`server.rs:432-483`).
- Every HTTP door except `/health` checks the bearer on each request. Panel and surface doors also refuse a query string, hold 4 operations, and cap the body at 64 KiB (`crates/host/src/panel.rs:45-46,132-141,199-212`).
- The surface doors are mounted behind the same guard (`crates/host/src/surface.rs:13-15,29-37`; `server.rs:257`). Insula doors cap the body at 512 KiB (`crates/host/src/insula.rs:41-55`).
- The bearer proves reach, not identity. `validate_command` compares the claimed House, room, spirit, scope, and recipient with public configuration values (`server.rs:2537-2583`).
- `sender_session` is caller-chosen, free-form, and at most 256 bytes (`server.rs:2567-2581`).
- `Subscribe` and `PaperBoatReceiptSubscribe` pass with an all-blank binding (`server.rs:2551-2566`).
- Deltas, receipts, and chat deltas go to every subscribed socket. Only Hallway deltas are filtered by session (`server.rs:370-430,375-377`).
- `room/state` returns every presence `session` and `operator` to any bearer holder (`surface.rs:118-121,133`).
- Knock claim and settle also compare the sender with the configured spirit (`server.rs:1649-1665`).

## 6. Identity

- No person identity exists. The Host stamps the chat author from the room file's single `operator` (`server.rs:934-939,1009`; `surface.rs:85`). Two people in one room get the same name.
- Operator identity is self-asserted. `set_room_state` rewrites the operator name, and the room marker wins over stored state (`tools.ts:818-847`; `room.ts:112-119`).
- The Hallway tools send no credential. Their descriptions say "authenticated" (`tools.ts:1598,1626,1649`). The binding comes from the working directory, room files, and the harness session id (`tools.ts:60-70`; `room.ts:99-127`).
- Not re-verified at `a6ab453`: whether the substrate checks that binding. `crates/akasha` decides.
- `LogConversation` creates directories and appends files under a caller-chosen `room_dir` with no check (`server.rs:2117-2127,2131-2235`). The comment at `server.rs:2064-2066` claims otherwise.
- `LogConversation` also takes the `operator` label for stored turns from the wire (`server.rs:2155`).

## 7. Pulse proxies

The 2026-10-04 source repair guards both proxies before route handling. It is not deployed.

- `Host` must equal `127.0.0.1:<listening-port>`.
- Any supplied `Origin` must equal `http://127.0.0.1:<listening-port>`.
- Requests other than GET or HEAD require that Origin.
- Cross-site Fetch Metadata is refused. Refused requests receive HTTP 403 before forwarding or local repair.
- Navigation without an Origin header remains available at the canonical loopback URL.
- These checks prevent browser cross-origin requests. They do not authenticate native clients, which can forge headers.
- Permitted live calls receive the Host bearer server-side. The page never holds it.
- The allow-list has 15 routes, including `chat/say` and `room/state` (`gui-prototype/live-routes.json:2-16`; `proxy.rs:30`).
- The proxy documentation includes chat writes. The route allow-list remains unchanged.
- The desktop webview has no CSP (`gui-desktop/tauri.conf.json:8`). Its one capability grants no IPC commands (`gui-desktop/capabilities/default.json:1-6`).
- Nothing in those files restricts where the window can navigate (`tauri.conf.json:5-10`; `gui-desktop/src/main.rs:70-73`).

## 8. Rooms

- Each room is its own `server::Host` inside one process (`house.rs:166-202`). `validate_command` requires `sender_room` to equal the configured room (`server.rs:2568`). Chat checks the payload room too (`server.rs:1004`).
- The Host requires a safe room key that matches the room-state file (`config.rs:87-89`; `crates/host/src/store.rs:56-62`). The adapter enforces a room-key pattern (`host.ts:5-6,77-79`).
- A missing room falls back. The adapter uses `<ATHANOR_VAULT_ROOT or ~/Solarisael>/default-room` for an unrecognized working directory (`room.ts:105-110`).
- Pulse uses room `kodo` when `PULSE_ROOM` or `--room` is unset (`serve.ts:34`; `gui-desktop/src/main.rs:53-77`). The room must be listed in `runtime.json` (`serve.ts:38-40`).
- Not re-verified at `a6ab453`: that cross-room retrieval needs a named room or exact address. `crates/akasha` decides.

## 9. NATS delivery

- NATS runs today. The service starts `nats-server -js` on loopback (`supervisor.rs:417-459`). The production builder always sets the NATS URL (`app.rs:30-58`).
- The Host runs a NATS `DeliveryService` task (`house.rs:171-181,235-248`). It consumes Hallway delivery over NATS (`server.rs:2863`).
- The Hallway projection carries `from_room` and `to_rooms` (`server.rs:2885-2894`).
- Not re-verified at `a6ab453`: whether NATS messages exclude record bodies, and whether consumers reload before they acknowledge. `crates/akasha` decides.
- Not re-verified at `a6ab453`: NATS account and subject permissions. `crates/athanor-install/src/native_runtime.rs` decides.

## 10. The service

- The service `SolarisaelAthanor` runs only PostgreSQL, in managed mode, and NATS (`crates/athanor-install/src/service.rs:264-308`; `supervisor.rs:41-55,417-459`).
- The Host, the keepers, OMP, and Pulse are user processes (`crates/athanor-install/src/cli/start.rs:119-125`).
- `sc.exe create` sets only `binPath`, `start= auto`, and `DisplayName` (`boundaries.rs:449-556`). It names no service account.
- Not re-verified at `a6ab453`: the account the service runs as. `crates/athanor-install/src/boundaries.rs` decides.

## 11. Model and embedding providers

Local storage does not make a hosted model private. Review the provider's data handling before you send private room material.

Not re-verified at `a6ab453`: which memory excerpts the adapter puts in the prompt. `adapters/omp/index.ts` decides.

When you use a hosted embedding endpoint, treat every embedded document as data sent to that provider. Provider portability does not override provider terms, logging, policy, rate limits, or outages.

Worker lanes fix their model role (`crates/host/src/routing.rs:100-157`). `house_model_default` changes the harness model through `pi.setModel` (`tools.ts:1278`).

## 12. Memory and lesson writes

Write durable memory deliberately. Before you record sensitive personal or company material, consider:

- whether the detail is necessary for continuity;
- which room or project owns it;
- whether it should be shared;
- whether a source document is a better authority;
- how it can be corrected, superseded, archived, exported, or deleted.

A memory records an event or realization. A lesson records a reusable rule. Do not convert an entire private transcript into lessons automatically.

## 13. Destructive operations

- The `delete_lesson` tool exists (`tools.ts:714`).
- Not re-verified at `a6ab453`: that it needs the exact ID and current title, and refuses broad deletion. `tools.ts` and `crates/akasha` decide.

Room and memory deletion need explicit operator intent and a clear statement of the affected scope. Keep the room when you remove an adapter or a bundle. Back up AKASHA before migrations, bulk imports, retention changes, or destructive maintenance.

## 14. Public evidence and demonstrations

Public artifacts contain sanitized aggregates and synthetic fixtures. Never publish:

- raw private turns;
- memory titles;
- source paths;
- retrieved excerpts;
- people or entity names;
- room or thread identifiers;
- private retrieval diagnostics;
- screenshots containing account names, home paths, notifications, or credentials.

Public demonstrations use a sterile demo House with synthetic rooms, projects, decisions, and memories. Record from a clean account or environment. Post-production redaction is defense in depth, not the primary privacy boundary. See [`EVIDENCE.md`](./EVIDENCE.md).

## 15. Planned boundaries

These are design intent, not current behavior. [`ROADMAP.md`](./ROADMAP.md) holds each one.

- Delivery and execution: a PostgreSQL outbox, ID-only NATS messages, and independently bound model, target, and session for headless room work.
- Synthesis and self-improvement: separate trusted checkers, capability-free WASM modules, and no proposal that approves, publishes, or installs itself.
- Companions: scope-bounded grants for child rooms, and consented, lineage-tracked training that cannot activate its own weights.
- Marketplace: inert packages until signature, provenance, permission, sandbox, revocation, and local activation checks pass.
- Enterprise: tenant, team, and private scopes, with authorization before ranking, encrypted transport, and audited administration.

Do not treat the current House as a substitute for the enterprise controls. It has one shared bearer and no person identity (`house.rs:205-234`; `server.rs:2537-2583`). It has no TLS (`host.ts:88-90`).

## 16. Vulnerability reports

Report security defects privately to the repository owner before you publish exploit details or private-data exposure. Include the affected component, the release version, and the API version. Add a minimal synthetic reproduction, the expected boundary, the observed result, and whether private data was exposed.

Do not attach real room data to a report. Check [`BUGS.md`](../BUGS.md) before you report a defect.
