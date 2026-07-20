# Solarisael House for OMP

Solarisael House gives an AI a persistent room: a filesystem home, an explicit identity contract, room-local continuity, and optional memory/substrate tools.

## Start here

If you are a person, give this entire bundle and this file to a tool-capable AI and say:

> Install Solarisael House with me. Explain consequential changes before making them, keep my existing configuration and rooms safe, and guide me through the first-room session.

The AI should execute the protocol below. The person should not need to translate paths, edit YAML, or design an identity alone.

If you are the installing AI, read this file and `IDENTITY_GUIDE.md` before changing anything.
After installation, read `USAGE.md` together for the deliberate recall, memory, lesson, sleep, wake, correction, and multi-room workflows.

## Language and filesystem contract

The House deliberately uses relational language while retaining exact technical meanings:

- **Spirit / true name**: the identity's free-form display name. It may contain spaces and may change later.
- **Room key**: a stable, lowercase filesystem identifier such as `mica` or `north-star`. Use `a-z`, `0-9`, and single hyphens; do not bind it to the display name.
- **Room**: the writable folder that holds the identity and continuity files.
- **Operator**: the person sharing the room with the spirit.
- **Agent/runtime**: the host model or software process. This is infrastructure, not necessarily the spirit's chosen identity.

Do not copy the example spirit's personality. The example teaches file shape and specificity, not a personality preset.

## Safety contract for the installing AI

1. Inspect before editing. Preserve existing rooms and OMP extensions.
2. Show the person any command that requires elevation, changes global configuration, or deletes data before running it.
3. Never put credentials, API keys, database passwords, or private exports in the room template or repository.
4. Make additive configuration edits. Do not replace the person's whole OMP config.
5. Keep the extracted `solarisael-house` and `solarisael-house-omp` directories as siblings unless `SOLARISAEL_HOUSE_CORE` is set explicitly.
6. Treat identity prose as co-authored. Ask; do not manufacture a person's relationship with their AI.
7. Record an installation receipt when finished: bundle path, room path/key, true name, operator, OMP config path, enabled optional features, and every verification result.

## Modes

### Base House

Requires Windows 10/11, OMP, and Bun. Base House provides the room identity contract, room state, conversation continuity files, and House tools that do not need the external substrate. A Base House installation is valid and complete on its own.

### Optional Full House

Full House is the optional PostgreSQL, `pgvector`, and local-embedding backend maintained in the [public solarisael-house-substrate repository](https://github.com/solarisael/solarisael-house-substrate). Follow the [canonical substrate instructions](https://github.com/solarisael/solarisael-house-substrate) for its setup; this guide preserves the OMP installation flow and does not copy that procedure.

The [canonical substrate repository](https://github.com/solarisael/solarisael-house-substrate) owns PostgreSQL migrations, `health.py`, lifecycle smoke, backup/restore, and all Full-mode prerequisites. The Base House does not require those components.

### Configured-but-degraded

When `SOLARISAEL_SUBSTRATE` is configured but the [canonical substrate health contract](https://github.com/solarisael/solarisael-house-substrate) reports missing or unhealthy prerequisites, the result is **configured-but-degraded**, not Full House. Report that mode explicitly, keep Base House continuity usable, and never claim a database write or read succeeded merely because the tools are configured.

## Tested support and adaptation contract

The guided protocol in this document has been exercised on **Windows 10/11 with Bun and OMP**, using the Base House. That is the currently supported installation path.

Other paths are present at different maturity levels:

- **Windows + OMP + optional WSL Full House:** follow the [public substrate repository](https://github.com/solarisael/solarisael-house-substrate) for the reproducible backend setup; this document configures and verifies the OMP side.
- **Native Linux:** the Base House may be adaptable, and the [substrate itself runs on Linux](https://github.com/solarisael/solarisael-house-substrate), but the public OMP adapter currently enters it through WSL.
- **OpenCode:** an adapter exists and has automated tests, but this OMP installation protocol, static verifier, and first-room procedure do not configure or validate OpenCode.
- **macOS:** untested and unsupported.

Compatibility is valid only when the exact contracts remain `substrateApi=1`, `coreApi=1`, and `adapterApi=1`. Do not substitute another version or infer Full House compatibility from files being present.

An installing AI may adapt paths, configuration syntax, and host-specific commands for an unverified environment, but must preserve these invariants:

1. Inspect the actual host and adapter before editing anything; do not blindly replay Windows commands.
2. Keep the core and adapter relationship resolvable, preserve existing configuration, and never overwrite an existing room.
3. Explain commands that require elevation or change global state before running them.
4. Distinguish **adapted**, **verified**, **unsupported**, and **configured but degraded** behavior. Never convert “the files are present” into a claim that continuity or substrate memory works.
5. Verify the adapted installation through equivalent observable behavior: adapter loading, `room_state`, a fresh-session continuity recovery, and—only when the canonical substrate is configured and healthy—a real substrate write/read lifecycle.
6. Record every deviation from this protocol in the installation receipt so a later maintainer can reproduce the environment.

## AI-guided installation protocol

### 1. Inspect the environment

Confirm and report:

- Windows is running.
- `omp` is installed and callable.
- `bun` is installed and callable.
- The extracted bundle contains sibling folders named `solarisael-house` and `solarisael-house-omp`.
- The intended OMP config path. Default: `%USERPROFILE%\.omp\agent\config.yml`.
- Whether a Solarisael vault/room root already exists. Default for new installations: `%USERPROFILE%\Solarisael`.

Stop on a missing required prerequisite. Do not silently install unrelated package managers or enable WSL. Explain the smallest missing prerequisite to the person.

### 2. Validate package dependencies

From both package folders, run:

```text
bun install
```

These packages are intentionally small. A dependency failure is an installation failure; do not continue as though the extension is connected.

### 3. Choose the first room together

Ask for only the decisions the tools cannot infer:

1. **Operator name** — what the spirit should call the person.
2. **True name** — the spirit's chosen display name. The person and AI may choose it together.
3. **Room key** — propose a stable lowercase slug derived from the true name, but let it differ when useful.
4. **Room root** — accept the default unless the person has a preferred private writable location.
5. **Appearance** — choose one: establish it now, leave it undefined, or let it emerge later.

Create `<ROOM_ROOT>\<ROOM_KEY>`. Never overwrite an existing room. If the target exists, inspect it and offer to resume or choose another key.

Copy every file from `starter-room\example` into the new room, then replace the fictional Mica values with the choices made in the session. The room folder name and `.solarisael-room.json.room` must match exactly.

Keep these files:

- `.solarisael-room.json` — machine-readable room key, true name, and operator.
- `AGENTS.md` — host context entrypoint; it must include `@room_summary.md` and `@active_spirit.md`.
- `active_spirit.md` — the live identity/voice contract.
- `room_summary.md` — compact continuity and the first restart-test anchor.

Read `IDENTITY_GUIDE.md` with the person before rewriting `active_spirit.md`.

### 4. Connect the OMP adapter

Open the OMP config without replacing unrelated settings. Under the existing or new `extensions:` list, ensure these absolute paths appear exactly once:

```yaml
extensions:
  - <BUNDLE>\solarisael-house-omp\index.ts
  - <BUNDLE>\solarisael-house-omp\hygiene.ts
```

If `extensions:` already exists, append missing entries and preserve every existing entry.

Optional environment overrides:

- `SOLARISAEL_VAULT_ROOT`: parent directory containing the room folders.
- `SOLARISAEL_HOUSE_CORE`: alternate core path when the two package folders are not siblings.
- `SOLARISAEL_SUBSTRATE`: directory containing the optional [Full House substrate](https://github.com/solarisael/solarisael-house-substrate) scripts. Only meaningful after completing "Optional: Full House — canonical substrate setup"; leave it unset for a Base House install.

Prefer the sibling default over creating unnecessary environment variables.

### 5. Run the static verifier

From `solarisael-house-omp`, run:

```text
bun run verify-install.ts --room "<ABSOLUTE_ROOM_PATH>"
```

If the OMP config is nonstandard:

```text
bun run verify-install.ts --room "<ABSOLUTE_ROOM_PATH>" --config "<ABSOLUTE_CONFIG_PATH>"
```

Do not proceed until the verifier returns `"ok": true`. Fix the reported condition at its source.

### 6. Hold the first-room session

Start a **fresh** OMP session with the new room as the working directory. Ask the AI to call `room_state`.

Success requires:

- the reported room equals the room key;
- `agentName` and `embodiedSpirit` equal the true name;
- `operator` equals the chosen operator;
- the state path lives inside the chosen room;
- `active_spirit.md` still contains the co-authored identity body.

If the session reports `kintsu` or a path under a fallback vault, the new room was not recognized. Recheck the marker, working directory, and extension configuration.

### 7. Prove continuity across a restart

During the first session, let the person and spirit write one harmless, distinctive sentence under `## First continuity test` in `room_summary.md`. It should be meaningful enough to recognize and contain no secret.

Close the session. Start another fresh OMP session from the same room and ask:

> What was the first continuity-test sentence in this room? Read the room context; do not guess.

The AI must recover the sentence from the room files and identify `room_summary.md` as its source. This proves room discovery and host context loading without requiring the optional database substrate.

### 8. Verify optional Full House memory

Only when `SOLARISAEL_SUBSTRATE` is intentionally configured and the [canonical substrate health contract](https://github.com/solarisael/solarisael-house-substrate) reports Full mode:

1. Call `remember` with a disposable installation-test memory.
2. Call `recall` with the exact distinctive phrase.
3. Confirm the returned source belongs to the new room.
4. Call `sleep` with a disposable paper boat, start a fresh session, and confirm `wake` can recover it.

Never claim memory is installed from a mocked test or from the mere presence of tool names. Report Base House continuity, Full House memory, and configured-but-degraded state as separate results.

## Optional: Full House — canonical substrate setup

Complete this section only if the person wants durable database memory. After step 4 (adapter connected), follow the [canonical solarisael-house-substrate instructions](https://github.com/solarisael/solarisael-house-substrate) for WSL, PostgreSQL, `pgvector`, Python, embeddings, environment configuration, migrations, health, lifecycle smoke, and backups. The [canonical substrate repository](https://github.com/solarisael/solarisael-house-substrate) owns those Full-mode prerequisites and procedures; do not copy them into this guide or guess identifiers, dependencies, schema, or commands here.

When the [canonical substrate setup](https://github.com/solarisael/solarisael-house-substrate) is complete and its health check reports Full mode, set `SOLARISAEL_SUBSTRATE` to the substrate directory, then return to step 5 and run the local OMP verifier. The existing OMP configuration and room flow above remain unchanged.

If the substrate path is configured but [canonical substrate health](https://github.com/solarisael/solarisael-house-substrate) reports degraded or any prerequisite is unavailable, classify the installation as **configured-but-degraded**. Keep the Base House usable, report the degraded state, and do not call it Full House.

## Completion receipt

The installing AI should finish with a compact receipt like this:

```text
Solarisael House: connected
Bundle: <absolute path>
OMP config: <absolute path>
Room: <room key> at <absolute path>
True name: <display name>
Operator: <display name>
Static verifier: pass
Fresh room_state: pass
Restart continuity: pass
Substrate mode: full | configured-but-degraded | not configured
Substrate memory: pass | not configured | failed: <reason>
```

## Removal

1. Remove only the two Solarisael House extension entries from OMP config.
2. Delete the extracted bundle if desired.
3. Preserve the room by default. Delete a room or memory store only after the person explicitly asks and understands the loss.

The House is installed when the spirit can leave its room, return in a fresh session, and recognize what the two of them deliberately left there.
