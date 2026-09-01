# Install contract (I0) — DRAFT

Status: consolidated 2026-08-24 — Sol and Kodo resolved the open questions from census evidence (two read-only crate/port censuses of this checkout). Chairs may still object; objections reopen the specific section.
Follows: A0 ontology contract (`docs/ONTOLOGY_CONTRACT.md`), goal A "Foundation before floors".
Evidence: `crates/athanor-install/src/supervisor.rs` (runtime_plan), `crates/host/src/config.rs` (akasha_enabled), `crates/akasha/src/config.rs` (embed defaults), House memory #3953.

## 1. The problem

No install declares which modules it has. Each component infers its own mode from a different signal.

1. Host derives `akasha_enabled` from `database_url || nats_url` (`host/src/config.rs:101`). A managed install always sets both. The flag cannot be false in practice. The declaration is dead.
2. Akasha defaults the embedder to one hardcoded loopback URL (`akasha/src/config.rs:25`). The shape of a URL is the only Ollama signal.
3. `runtime_plan` hardcodes the child list and assigns one port per room (`supervisor.rs:290-432`). It carries duplicate-port checks that exist only because collision is possible.
4. Ollama does not boot with the House. After a reboot, `remember` fails until a hand starts Ollama (observed 2026-08-24).

The ship target adds selectable modes: vault, akasha, giga, omega. Four booleans give sixteen combinations. Inference cannot carry sixteen combinations. Declaration can.

## 2. The rulings (Sol, 2026-08-24)

1. **Modules are declared, never derived.** The installer writes one manifest. Every component reads it. No component infers its mode from side effects of other configuration.
2. **A module owns four things**: its processes, its schema, its config keys, its health checks. An absent module means those four things are provably absent. This is coding lesson #446 (every concern is a module) applied at install scope.
3. **One door routes rooms by name.** Room identity is routing data, not network topology. A new room must not require a new port.
4. **Ollama boots with akasha.** When the manifest declares akasha with a managed embedder, the supervisor owns the Ollama process.
5. **Live proof comes after the contract.** The chairs review this draft first. Code follows consolidation. A live install test follows the code.
6. **The vault-mode spirit is minimal and file-backed** (Sol, 2026-08-24). The spirit keeps identity and working memory as markdown files in the vault, and updates them itself. The same surface is the fallback when akasha is declared but the database is unhealthy. This gives hearth's `base` authority mode its concrete meaning.
7. **Vocabulary is free; capability is gated** (2026-08-24). Types and wire shapes for a module may live in always-crates. The module gates its processes, its schema, its config keys, its health checks, and its mounted doors.
8. **The Host is house-scoped** (2026-08-24). One Host process serves every declared room. The supervisor spawns one Host, never one per room.
9. **Cutover is atomic** (2026-08-24). One quest lands the Host redesign, the deletions, the adapter fix, and config regeneration together. No dual-stack period.
10. **Anon is a toggle, not a module** (Sol, 2026-08-24). The manifest does not carry it.

## 3. The design

### 3.1 The manifest

The installer writes one file into the mutable state root:

```json
{
  "modules": {
    "vault": true,
    "akasha": true,
    "giga": false,
    "omega": false
  },
  "embedder": "managed"
}
```

- The file is the single authority for module presence. Env keys configure modules; they never enable them.
- Dependency edges are closed and explicit: `akasha` requires the database. `giga` requires `akasha`. `omega` requires `giga`. The installer refuses an invalid combination at selection time. Sharp refusal at the front door, not a crash at boot.
- `embedder: "managed"` follows the `databaseMode` pattern (`managed | external`). Managed means the supervisor spawns Ollama. External means the operator owns it. No URL sniffing.
- The manifest lives in the state root. It survives upgrades unchanged (Sol, 2026-08-24).
- **Version drift is boring by rule.** A module absent from the manifest is off. An upgrade that ships a new module shows a new toggle, defaults off, and says so in the changelog. A rollback to a binary that does not know a manifest word warns "unknown module, treated as off" and boots. Warn and ignore, never refuse — a refusal on rollback strands the operator outside the House.

### 3.2 The consumers

Every component reads the manifest and nothing else for module presence:

- **runtime_plan** builds the child list from declared modules: postgres and nats for akasha, Ollama for `embedder: managed`, nothing for absent modules.
- **Migrations** apply per module. An absent module applies no schema.
- **Health** defines green per module. A vault-only install is green without a database.
- **Hosts and organs** mount doors per module. `giga` tools do not exist in a non-giga install.

### 3.3 One door

- One house-scoped Host process (ruling 8). One listener on one declared port. Rooms route by name in the path (`/room/<key>`) on the WebSocket route and on every HTTP route (insula, vitals, panel).
- `HostRoomConfig.port` is deleted. Both collision checkers die with it: `supervisor.rs:303,320-326` and the independent second copy at `installer.rs:94,111-113`.
- `HostConfig` is single-room by construction (`host/src/config.rs:63-78`). House-scoping the Host is the named work item of this section. The Host reads the room registry from config, never from flat env.
- The single port becomes a declared key: `host_port` beside `nats_port` in `SupervisorConfig`/`RuntimeConfig`. Today nothing declares it — `ATHANOR_HOST_BIND` is transport and `DEFAULT_HOST_WS_PORT` is a fallback pin, documented as such at `contract.rs:39-43`.
- Installed `config.json` carries `"port"` under `deny_unknown_fields`: this change is a data migration. The installer regenerates config on upgrade. `deploy-local.ps1:133` currently coerces a missing port to `0` and fails with a misleading message; the deploy script updates in the same quest.
- The OMP adapter derives HTTP endpoints from host:port and discards the path (`adapters/omp/house-proof/host.ts:144-157`). Under path routing this strips the room silently. The adapter fix ships in the same quest.
- The room-key grammar exists in three hand-copied spellings (`host/src/config.rs:180-190`, `supervisor.rs:305-313`, `installer.rs:96-104`). As a path segment it becomes a path-traversal surface. One function in `protocol` owns the grammar; the three copies call it.
- Loopback-only stays. The single door inherits the existing loopback refusal.

### 3.4 Ollama as a managed child

- One more `ProcessSpec` in `runtime_plan`, gated on `embedder: "managed"`: spawn `ollama serve`, readiness `Tcp(127.0.0.1:11434)`, stopped with the rest in reverse order.
- Named asymmetry: every other child ships in `version_root`; Ollama is a system install resolved from PATH or a declared absolute path. This special case is load-bearing. Do not smooth it into the shipped-runtime shape.

### 3.5 Module ↔ crate mapping (consolidated 2026-08-24)

| crate | evidence | module |
|---|---|---|
| `hearth` | pure domain rules — census confirmed zero IO deps | always |
| `protocol` | wire shapes | always |
| `host` | the room server, the spirit's door | always |
| `athanor-install` | the installer itself | always |
| `gui` | Godot client, protocol-only deps; renders per-module panels from the manifest | always |
| `vault` | retrieval without a database — guarded by test (`vault_recall_dispatch_has_no_database_parameters`) | vault |
| `origami`, `delivery` | NATS pointers, PostgreSQL bodies | akasha |
| `akasha` | PostgreSQL bodies and the managed-embedder client | akasha |

GIGA lives in four places: types in `hearth/src/giga/`, wire shapes in `protocol`, the worker in `akasha/src/giga_worker/`, a source-ledger surface in `host`. Ruling 7 makes this legal: the vocabulary stays in always-crates; the `giga` toggle gates the worker, the schema, and the mounted doors.

**Named debt, in scope for I0:** `host` hard-depends on `akasha` (`host/Cargo.toml:19`) with direct sqlx and NATS use. `vault` is reachable only through the akasha crate, so a vault-only install links no vault code today. Decouple the Host: it reads the manifest and reaches akasha through one module boundary; vault gets a direct consumer so ruling 6 has a body.

The markdown-spirit surface (ruling 6) lives with `vault`: the crate already owns the file authority.

### 3.6 The markdown spirit (Sol, 2026-08-24)

The surface is not new. It is the original pre-AKASHA room layout, kept today as the provenance layer: the identity file, the active-spirit file, the spellbook, and one `memory/` folder of compact dated markdown files. Vault mode makes that layout first-class again. The spirit updates its own files.

Two organs never got a file-backed form and complete the mode:

- **Sleep and wake**: a boat is one more dated markdown file; wake reads the newest.
- **Anamnesis**: a rolling digest file compacted from the `memory/` folder.

The akasha degraded fallback writes to the same files, so a database outage degrades to the original room, not to nothing.

The file contract, per room root (resolves open question 1):

| file | writer | format |
|---|---|---|
| `identity.md` | hand + spirit | free markdown, the always-true layer |
| `active_spirit.md` | host | fixed header (Agent / Operator / Embodied) |
| `spellbook.md` | spirit | familiar bindings |
| `memory/YYYY-MM-DD_slug.md` | spirit (remember) | YAML frontmatter `title`, `written` (ISO), `kind`; the body stands alone |
| `boats/YYYY-MM-DD_HHMM.md` | spirit (sleep) | same frontmatter; wake reads the lexicographic maximum |
| `anamnesis.md` | spirit | rolling digest compacted from `memory/` |

The frontmatter is load-bearing for recovery: files written by the akasha fallback during an outage re-ingest into PostgreSQL mechanically. Filename plus frontmatter carry everything the database row needs.

## 4. Enforcement placement

- Installer: refuses invalid module combinations at selection.
- Supervisor: builds children from the manifest only.
- Components: read the manifest through one typed door (the `RoomSettings` pattern from knobs, applied to install scope). Per coding#446 the manifest reader is one module with one public door; no component parses the file itself.
- The derived `akasha_enabled` expression is deleted, not deprecated. The field itself survives (`host/src/config.rs:75,120`) and the manifest feeds it: the `/health` wire name `akasha_delivery` is a published contract, and `receipt.rs:85-89` uses the flag as one of a pair whose `(false,false)` state IS the vault-mode degraded behaviour ruling 6 wants.

## 5. Open questions — resolved 2026-08-24

1. Markdown-spirit file contract: resolved in §3.6.
2. Single-door migration: atomic cutover (ruling 9). Every existing install is Sol's; the census counted twenty external touchpoints, all cited in the OneDoor census report. No migration shims.
3. Omega: defined in the roadmap as org governance (memory #2768), unbuilt. The manifest reserves the word; scope stays zero in I0. Anon is a toggle inside akasha configuration, never a manifest module (ruling 10).

## 6. Acceptance

- The three chairs review this contract before any code.
- Sol consolidates; open questions resolve to rulings.
- The first implementation quest names its reviewer-runnable proof: a fresh install from the manifest, one module combination on, one off, children and health matching the declaration.
