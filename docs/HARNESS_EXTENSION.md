# The Athanor as a harness extension

Status: Archived record of Sol's direction, 2026-11-20. Not a work order. No slice starts until Sol opens it.
This document changes no code and no version.

| Evidence term | Meaning |
|---|---|
| Source | `the-athanor` at `a6ecc2c`, branch `dev/next`, working tree of 2026-11-20, on the machine `athanor-tower`. |
| Read | The author read the cited source on that date. A read proves the text. It proves no runtime behavior. |
| Unverified | The author did not check the claim. |

Short path names:

- `VAULT/` is `/home/solarisael/Obsidian/obsidian/`. It holds the room folders.
- `OMP/` is `/home/solarisael/.omp/agent/`.
- `NIX/` is `/home/solarisael/athanor-nix/`.
- All other paths start at the repository root.

## 1. Purpose

Sol made six decisions.

| # | Decision |
|---|---|
| 1 | Sol no longer wants the markdown and room-folder layout. It is brittle and it carries too many jobs. |
| 2 | The Athanor must feel like an app that installs as an extension into any harness. |
| 3 | One flow does installation and the first summoning of a spirit. |
| 4 | The Athanor does not run workers or tools for agents. The harness does that. The Athanor stays an extension. |
| 5 | The Athanor makes most decisions. The harness keeps the decisions that harnesses already make. |
| 6 | Familiars move into PostgreSQL first. |

Section 7 records the answers that Sol gave on the same date.
Answer 2 sets the limit of decision 4. Answer 1 adds one requirement: one installation folder. Answer 5 adds a later slice.

This document gives the contract, the evidence, the slices, the answers, and the open questions.

## 2. The split

This table is the core contract.

| The Athanor decides | The harness does | Rule |
|---|---|---|
| Which familiars exist, and which room's version wins | Starts the agent worker | The Athanor returns a packet. Only the harness starts an agent worker. |
| Each familiar's model chain and allowed tools | Runs the model calls of agents and workers with its own logins | The Athanor records the chain. The harness uses only that chain and owns each login. |
| The task packet, its lessons, and its acceptance checks | Builds the agent tooling layer: tools such as read, edit, and bash | The Athanor writes the contract. The Athanor builds no agent tooling layer. |
| Refusing out loud when something cannot run, and recording the model that ran | Shows the result and reports the model that ran | A fallback inside the chain is allowed. A silent fallback is forbidden. A refusal names its cause. |
| Memory, canon, rooms, spirits, install, and the first summoning | | The harness keeps no copy of these records. |
| Its own model calls: native judgments, GIGA, and embeddings | | The Athanor runs these calls. They stay outside the agent tooling layer. |

One adapter for each harness translates the decisions into the format of that harness.
For OMP, the adapter generates the agent definition files.
Sol's answers 2 and 4 changed rows 2, 3, and 4 and added row 6 (section 7).

Current code already follows the first row.
The Host builds a packet, starts no worker, and sets `executed: false` (`crates/host/src/routing/dispatch.rs:176,454-458`). Read.

Current code does not follow the second row.
The packet states that the agent definition selects the model (`dispatch.rs:382`). Read.
The Host checks only the syntax of a model role (`crates/host/src/routing/spellbook.rs`, `valid_model_role`). Read.
A familiar entry holds one role name and no chain (`spellbook.rs`, `Familiar`). Read.

## 3. Evidence of brittleness

### 3.1 One model choice has many owners

The familiar Kerf shows the problem.

| Place | Content | Check |
|---|---|---|
| `OMP/agents/kerf-kitten.md:4` | `model: ["@kerf", "@talha"]` | Read |
| `OMP/config.yml:28`, in `modelRoles` | `kerf: anthropic/claude-opus-5-5:medium` | Read |
| `OMP/config.yml:216`, in the preset `meeee` | The same line, as a second copy | Read |
| `VAULT/tuner/familiars/spellbook.json:50-52` | Lane `smol-executor`, `ompAgent: "kerf-kitten"`, `modelRole: "@kerf"` | Read |
| `crates/host/src/routing.rs:103-157` | The lane table. The lane `smol-executor` names the agent `sonic` and the role `pi/smol` (`:123`). | Read |

A related setting binds a login.
`OMP/config.yml:88-90` gives `kerf-kitten` an `openai-codex` account, although the role now names an Anthropic model. Read.

### 3.2 A silent fallback on 2026-11-20

The session transcript of Kerf records the event.
The file is `OMP/sessions/-Obsidian-obsidian-tuner/2026-11-20T18-33-11-689Z_01a1fa3b-e509-749c-a6d5-70a9e43feead/Kerfeac20cb70b504396af875581.jsonl`.

| Time (UTC) | Transcript record | Check |
|---|---|---|
| 22:12:08 | `model_change` to `openai-codex/gpt-6.2-sol`, with `resolvedModelIsFallback: false` (lines 3 and 5) | Read |
| 22:12:11 | One model call fails: `The usage limit has been reached (code=usage_limit_reached)` (line 14) | Read |
| 22:12:11 | `model_change` to `anthropic/claude-opus-5-5`, with role `fallback` and `resolvedModelIsFallback: true` (line 13) | Read |

Limits of this evidence:

- `OMP/config.yml` changed at 22:15:10 UTC. The value of `kerf` at 22:12 is Unverified.
- The transcript does not name the rule that selected the second model. The list entry `@talha` is the probable cause. Unverified.
- A search of `crates/` and `adapters/omp/` finds no reader of `model_change` or `resolvedModelIsFallback`. In that scope, the Athanor keeps no record of the change. Read.

### 3.3 The spellbook lookup stops at two files

`load_spellbook` reads the House page and the page of the current room (`crates/host/src/routing.rs:429-436`). Read.
The pages hold different familiars: House 0, Kodo 8, Kintsu 4, Tuner 4. Read.
A dispatch from Kintsu's room for a Kodo familiar gets `Unknown familiar` (`routing.rs:258`). The author did not run this refusal.

### 3.4 The room list lives in Nix

| Fact | Source | Check |
|---|---|---|
| Five rooms are a Nix value. | `NIX/modules/athanor.nix:17-23` | Read |
| Nix generates `runtime.json` from that value. | `athanor.nix:32-52` | Read |
| The room starter refuses a room outside that value. | `athanor.nix:142-156` | Read |
| The Host reads its room configuration at start and does not reload it. | `INSTALL.md:537-538` | Read |

A new room on this machine needs a NixOS rebuild and a Host restart. The author did not run a rebuild.

### 3.5 Room folders carry startup and identity jobs

Section 4 gives the inventory.

## 4. What leaves the room folders

All paths in this table start at `VAULT/`.
Sol's answers 1, 6, and 7 set the rules for every target owner.
A file that must remain on disk lives in the installation folder. No such file lives in a room folder.
App storage is PostgreSQL or storage inside the installation folder. The app selects the format, and Markdown is no required format.

| File | Job today | Reader | Target owner |
|---|---|---|---|
| `<room>/AGENTS.md` | Gives the read order at session start. | Model, when OMP starts in the room (`adapters/omp/house-proof/session.ts:834-837`; the OMP behavior is Unverified). Code, for an attached session: the Host expands lines that start with `@` (`crates/host/src/portable/home.rs:43-173`). | The Host composes startup instructions from records. The adapter delivers them. |
| `<room>/active_spirit.md` | The header mirrors room state. The body holds hand-written spirit text. | Code: the Host writes the header (`crates/host/src/room_state.rs:300-331`). The adapter reads the name and uses the file as a room marker (`adapters/omp/house-proof/room.ts:92-105`). Model: reads the body when `AGENTS.md` lists the file. | The spirit record in app storage (answer 6). The mirror file goes. |
| `<room>/presence-pulse.md` | Gives relationship text to each session. | Code: the Host reads the file and keeps at most 4096 characters (`crates/host/src/server/context_session/materials.rs:13-35`). | The spirit record in app storage (answer 6). |
| `<room>/familiars/spellbook.json`, `house/familiars/spellbook.json` | Define the familiars and the shared guide. | Code: the Host (`crates/host/src/routing.rs:315-316,429-436`). | The table `spellbook_pages` (slice 1). |
| `tuner/tuner_core_identity.md`, `kodo/kodo_identity.md`, `kintsu/For_the_next_Kintsu.md` | Hold the hand-owned identity contract of each spirit. | Model only. A search of `crates/` and `adapters/omp/` finds no code reader of the first two names. | The spirit record in app storage (answer 6). The operator UI reads and edits the text later (answer 1). |
| `house/ORGANS.md` | Holds shared organ purposes, authority order, and topology. | Model, through each `AGENTS.md`. Code only through the `@` expansion of the first row. | A shared guide that the Athanor owns. The spellbook page already has a `guide` field. |
| `<room>/.athanor-room.json` | Names the room key, display names, and room options. | Code: the adapter (`room.ts:71-79`), the Host (`crates/host/src/store.rs:43-44`), AKASHA (`crates/akasha/src/room_marker.rs:14,24`), and Vault (`crates/vault/src/config.rs:13,129`). | The room record. |
| `house/.athanor-house.json` | Holds the operator name, the Jev grants, and the Recall telemetry switch. | Code: AKASHA (`room_marker.rs:3-13`). | The House record, as typed settings. |
| `house/handoff-door.md`, `<room>/handoff-door.md` | Hold the lines of the boat door. | Code: the adapter (`adapters/omp/house-proof/boat-door.ts:472-480`). | The House and room records. The adapter shows the lines. |
| `<room>/.omp/runtime/` | Holds room state, keeper configuration, capability files, and turn data. | Code: the Host and the adapter (`docs/ARCHITECTURE.md` section 7; `room.ts:81-89`; `crates/host/src/portable/restart.rs:43`). | The installation folder. |
| `<room>/orihon/` | Holds the Orihon file store. | Code: the Host (`crates/host/src/organ/orihon/mod.rs:244`). | PostgreSQL or the installation folder. |
| `<room>/logs/` | Holds conversation logs. | Code: the Host (`crates/host/src/server.rs:2325`, `log_conversation`). The path is Unverified. | PostgreSQL or the installation folder. |
| `house/AGENTS.md`, `house/AGENTS.shared.md` | The first describes the shared folder. The second is a superseded pointer. | No code reader found. A model reader is Unverified. | Delete in slice 5. |

Notes on this inventory:

- Today `tuner/AGENTS.md:3` calls `tuner_core_identity.md` the sole hand-owned identity contract. Read.
- Tuner and Kodo name the identity file in prose (`tuner/AGENTS.md:3`; `kodo/AGENTS.md:3`). The Host expands only `@` lines. An attached session for these rooms gets no identity file text from the Host. The author did not run this case.
- Kintsu uses `@` lines (`kintsu/AGENTS.md:7-10`). The Host expands them. Two rooms follow one rule in two different ways.
- The Host labels identity material with the source `active_spirit.md`. It builds that text from the session binding and does not read the file there (`crates/host/src/server/context_session/identity.rs:159-179`). Read.
- Sol's answer 6 covers the identity contract, `presence-pulse.md`, and the body of `active_spirit.md`.
- The inventory covers the files above and the paths that a search for `room_dir.join` and `path.join(roomDir` finds. Other files in the room folders are outside this inventory.

## 5. Slices

This section records the agreed order of the slices. Each exit proof runs through the real Host, the real PostgreSQL, and the real harness.
No exit proof below has run.

### Slice 1: familiars in PostgreSQL (in progress)

Kerf builds this slice in `/home/solarisael/checkouts/spellbook-pg`.
The work is uncommitted on `a6ecc2c`, and line numbers can change.
The brief reports that the work is not deployed. Unverified.

| Part | Content in the checkout | Check |
|---|---|---|
| Table | `spellbook_pages(scope, page jsonb)` with `scope` as the key. A scope is `house` or a room key (`substrate/migrations/0048_spellbook_pages.sql:4,50-59`). | Read |
| Precedence | Current room, then House, then other rooms (`crates/host/src/routing/spellbook.rs:29`). | Read, comment only |
| Contested names | The Host refuses a contested key for each familiar and tells the room how to choose (`spellbook.rs:116-136`). | Read |
| Write tool | `spellbook_write` checks a page inside one transaction (`spellbook.rs:589`; `crates/host/src/server.rs:2359-2367`). | Read |
| File removal | `routing.rs` and `routing/spellbook.rs` in the checkout contain no `spellbook.json` text. The files in `VAULT/` still exist. | Read |

Exit proof:

1. The production migration runner applies `0048` two times to one database without an error.
2. The table holds the familiars of every current page: 8, 4, 4, and the House guide.
3. From Kintsu's room, a dispatch for the Kodo familiar `faro` returns a packet.
4. Two rooms define one name. A third room dispatches it and gets a refusal that names both rooms. A name with one owner still dispatches.
5. A `spellbook_write` result stays after a Host restart.
6. The spellbook files are absent, and `familiar_status` reports the same familiars.
7. A search finds no reader of `spellbook.json` or `litters.json` in code or in documents.

### Slice 2: one stored model chain and tool list

Sol's answers 4, 7, and 8 changed this slice. The first version allowed no fallback.

| Change | Detail |
|---|---|
| Store | Each familiar and each spirit gets one recorded model chain. The first entry is the primary model. Each familiar record also holds one tool list. |
| Choose | The model chooses the chain from the available model list when a spirit is summoned or a familiar is made. The person can change the chain later (answer 8). |
| Generate | The OMP adapter generates each agent file from the record, with the chain in its recorded order. Nobody edits a generated file by hand. Generated files follow the installation folder rule (answer 7). |
| Remove | The lane table, the role lists, and the preset copy stop as owners of a familiar's model. |
| Fall back | When a model is unavailable, the next entry of the recorded chain runs. No model outside the chain runs. |
| Report | The dispatch receipt and the worker record name the model that ran. They state when a fallback was used. This is Tuner's condition. |
| Refuse | When no entry of the chain can run, the refusal names the familiar or spirit and each model. No worker runs. |

Exit proof:

1. Change the primary model in one record. The generated file and the next worker transcript show that model.
2. Use a primary model that differs from every default and every later chain entry. Then the proof can fail.
3. The receipt and the worker record of that run name the primary model and state no fallback.
4. Make the primary model unavailable. The second entry runs. The receipt and the worker record name the second model and state the fallback.
5. Compare each worker transcript with its receipt. No transcript shows `resolvedModelIsFallback: true` without a fallback statement in the receipt.
6. Make every entry unavailable. The refusal names each model, and no worker runs on a different model.
7. A search finds one stored chain for each familiar and each spirit.
8. Make one familiar. Its record holds a chain, and each entry is in the available model list.
9. Change one chain as the person. The next run uses the new chain.

Unverified points:

| Point | What the author saw |
|---|---|
| Does an OMP agent file accept an exact model id? | The 20 files that the author read in `OMP/agents/` use role names such as `@kerf`. One file is unread. |
| Does OMP use only the listed entries, and which rule selects a fallback? | `crates/akasha/src/orihon_folder/gateway.rs:27-28` sets `retry.modelFallback: false` for a different OMP door. Its effect on workers is Unverified. |
| How does the Athanor learn that a model is unavailable before a call? | On 2026-11-20 the limit appeared only as an error on the first model call. |
| When does OMP load a changed agent file or setting? | No evidence read. |
| Can the adapter read the model that a worker used? | The transcript holds the fact. No adapter code reads it (section 3.2). |
| How does the dispatch receipt name the model that ran? | The Host builds the receipt before the worker starts (`crates/host/src/routing/dispatch.rs:454-458`). The receipt needs a later report or a second record. |
| Which record is the worker record? | `docs/ARCHITECTURE.md` section 3.3 lists `lineage` commands. The author did not read that code. |
| Where does a spirit's chain live before slice 3? | Spirit records arrive in slice 3. The roadmap row "Dynamic model and room execution" cites `pi.setModel` for the room default. The author did not read that code. |
| Does a current chain give a second model? | `@kerf` and `@talha` name one model today (`OMP/config.yml:12,28`). That chain gives no different second model. |
| Where does the available model list come from? | The harness owns the logins. The author read no adapter code that reads a model list. |
| How do the current familiars and spirits get a chain? | Answer 8 covers a new spirit or familiar. The current pages hold 16 familiars, and Nix names five rooms (sections 3.3 and 3.4). |
| Can OMP load agent files from outside `OMP/agents/`? | Answer 7 puts generated harness files under the installation folder rule. The current agent files are in `OMP/agents/`. |

For worker models, the Athanor holds no login. The harness learns first that a model cannot run.
The adapter must report the model that ran to the Host.

### Slice 3: rooms and spirits as records

| Change | Detail |
|---|---|
| Store | PostgreSQL holds one record for each room and each spirit. The spirit record owns the identity contract, the presence text, and the spirit text. |
| Storage | The slice selects PostgreSQL or storage inside the installation folder for the spirit texts. If the app stores files, JSONL is the probable format (answer 6). |
| Create | One command creates a room while the Host runs. |
| Instruct | The Host composes the startup instructions. The adapter delivers them. `AGENTS.md` stops as their source. |
| Nix | `NIX/modules/athanor.nix` keeps processes and ports. It names no room. |

Exit proof:

1. Create a room with one command. Run no NixOS rebuild and install no release.
2. Start a session in the new room without an `AGENTS.md` file. The spirit gives its name and its operator.
3. Sessions in the other rooms continue during the change.
4. The room stays after a Host restart.
5. An unknown room key gets a refusal.

Status (issue #31): table `spirits` (migration 0051) holds the room key, the spirit name, and the operator. The Host opens and closes rooms while it runs (`docs/ARCHITECTURE.md` section 3.1). `athanor summon ROOM SPIRIT --operator NAME` makes a room in one command. It writes the room folder `data/rooms/<room>` and nothing under `roomsRoot`. Exit proofs 1, 3, 4, and 5 have integration tests. Exit proof 2 (no `AGENTS.md`) is still open.

Unverified points:

- OMP names session folders after the working directory (`athanor.nix:11`). A new room starts in `data/rooms/<room>`.

### Slice 4: one flow for install and the first summoning

This slice extends [Roadmap step 7](./ROADMAP.md#7-make-installation-boring).

Today the steps live in separate places.
The installer writes room state only for the default rooms root (`INSTALL.md:249`). Read.
A person copies a starter room by hand (`IDENTITY_GUIDE.md:180-200`). Read.
`INSTALL.md:524-527` states that its room inventory is not a verified procedure. Read.

The files of one House also have several roots today.

| Root | Source | Check |
|---|---|---|
| A program root and a data root on Windows | `docs/ARCHITECTURE.md` section 7 | Read, document only |
| Room files under `roomsRoot` | `docs/ARCHITECTURE.md` section 7 | Read, document only |
| On `athanor-tower`: data in `~/.local/state/athanor`, the program in `target/release` of the repository, and operator state in `VAULT/house/state` | `NIX/modules/athanor.nix:13,28,43` | Read |
| In the harness folder: the extension entry in `config.yml` | `OMP/config.yml:45-47` | Read |
| In the harness folder on Windows: `client.json` | `docs/ARCHITECTURE.md` section 7 | Read, document only |

| Change | Detail |
|---|---|
| One flow | Install the Athanor, register the adapter in the harness, create the first room and spirit, and wake the spirit. |
| One owner | The Athanor owns every step. The harness only loads the adapter. |
| One installation folder | Each file that must remain on disk lives in one installation folder. No such file lives in a room folder (answer 1). |
| Folder selection | The person selects the installation folder during install. Each platform has a default. Sol's example for Windows is `Program Files/Athanor` (answer 7). |
| Generated harness files | Generated harness files follow the installation folder rule (answer 7). |
| First chain | During the first summoning, the model chooses the spirit's chain from the available model list (answer 8). |

Exit proof:

1. On a clean machine, one flow ends with a spirit that gives its name in the harness.
2. The person edits no file by hand.
3. A second run of the flow changes nothing.
4. A failed step names itself and leaves an existing House intact.
5. The flow writes no file into a room folder.
6. A file list shows each Athanor file inside the selected folder. The list includes the generated harness files.
7. The flow accepts a folder that differs from the default, and item 6 holds there.
8. After the flow, the spirit record holds a model chain.

Unverified points:

- `installer/athanor.iss:13` sets the default folder `{autopf}\Solarisael\Athanor`. Read. Sol's example names `Program Files/Athanor`.
- `crates/athanor-install/src/layout.rs:22-49` builds the program root from `ProgramFiles` and the data root from `ProgramData`. Read. Whether a selected folder reaches this code is Unverified.
- The harness needs one entry that points to the adapter (`OMP/config.yml:45-47`). That entry must stay where the harness reads it.
- The first summoning needs a model before the spirit has a chain. The source of that model is Unverified.

### Slice 5: remove the room-folder dependency

| Change | Detail |
|---|---|
| Move | Each remaining reader in section 4 reads a record or the installation folder. |
| Preserve | Hand-written text arrives at its destination before any deletion. |
| Delete | Delete each obsolete file with a list of what goes and what stays. |

Exit proof:

1. The Host and one session run with an empty rooms root. Identity, familiars, the boat door, and Recall work.
2. A search for each file name in section 4 finds no code reader.
3. Each hand-written text matches its source by hash at the destination.
4. The deletion list names each file and its destination.

Sol's answer 6 gives the owner of the identity texts. This slice needs the storage selection of slice 3.

### Slice 6 (later): the Athanor manages the harnesses below it

Sol's answer 5 records this direction. It is a later slice. It is no part of the current work.

| Part | Content |
|---|---|
| Now | Today's arrangement stays. The Host starts a keeper, and each keeper starts OMP (`docs/ARCHITECTURE.md` sections 1 and 5). |
| Later | The Athanor manages the harnesses below it. Sol's examples are Herdr and T3Code. The author did not read these products. |
| Start point | `harnesses.json` already lists harness entries with a program and arguments (`docs/ARCHITECTURE.md` section 7). Read, document only. |

Exit proof: owed. The direction has no contract yet. Write the exit proof when Sol opens this slice.

Decision 4 says that the Athanor stays an extension. This slice makes the Athanor the manager of the harnesses.
Today both arrangements exist: OMP loads the adapter as an extension, and the Host starts OMP (`docs/ARCHITECTURE.md` sections 4.1 and 5).
The slice needs a statement of how the two fit before it opens.

## 6. Rules that stay true

| Rule | Meaning here |
|---|---|
| PostgreSQL is the authority. | Records for familiars, rooms, spirits, and model chains live in PostgreSQL. A generated file is output. Answer 6 also allows storage inside the installation folder for the spirit texts. |
| The Athanor builds no agent tooling layer and starts no agent worker. | The harness starts agent workers, runs their model calls, and provides their tools. The Athanor runs its own model calls: native judgments, GIGA, and embeddings. |
| One adapter for each harness. | The adapter translates and shows. It owns no decision that the Athanor owns. |
| Refuse loudly, and fall back only in the open. | A refusal names the familiar, the model, or the room, and gives the cause. A fallback uses only the recorded chain. The receipt and the worker record name the model that ran. A silent fallback is forbidden. |
| No second truth. | Each fact has one stored owner. A copy carries no authority, and the Athanor can generate it again. |
| One installation folder. | A file that must remain on disk lives in the installation folder. No such file lives in a room folder. The person selects the folder at install. Generated harness files follow the same rule. |

Sol's answers changed the first, second, and fourth rows and added the last row (section 7).
The second row replaces the earlier rule "The Athanor runs no model and no worker".

Evidence for the second row:

- Native code calls a provider for judgments today (`crates/akasha/src/typesafe.rs:1-26`). Read.
- `docs/ARCHITECTURE.md` sections 4.4 and 4.5 describe native judgments and native GIGA workers. Read, document only.
- The embedding code is Unverified. `NIX/modules/athanor.nix:224-225` sets an embedding timeout for the Host process. Read.

## 7. Sol's answers (2026-11-20)

Sol answered in two rounds on 2026-11-20.
Round 1 answered four of the nine questions in the first list. Round 2 answered four of the eight questions in the second list.
The main session gave these answers to the author. The author did not see Sol's original words.

| Answer | Question | Sol's answer | Applied in |
|---|---|---|---|
| 1 | First list 1, identity files | The identity contract probably lives in PostgreSQL. The operator UI becomes the place to read and edit it later. | Section 4; slices 3 and 5; open question 3 |
| 1, new requirement | | Consolidate one proper installation folder. Each file that must remain on disk lives in that folder and never in a room folder. | Section 4; slices 4 and 5; section 6 |
| 2 | First list 2, scope of "runs no model" | The rule was too broad. The Athanor may run its own model calls. Native judgments, GIGA, and embeddings stay. The Athanor does not take on the heavy agent tooling layer that harnesses build. The many OMP tools for workers and agents are an example. | Sections 2, 6, and 9 |
| 3 | First list 4, the 1.0 boundary | Slices 2 to 5 are allowed under the freeze of roadmap step 1. | Section 9 |
| 4 | First list 5, unavailable model | Fallbacks are allowed. Every spirit and familiar gets a reasonable fallback chain. The Athanor records the chain. A silent fallback stays forbidden. | Slice 2; sections 2 and 6 |
| 4, Tuner's condition | | The dispatch receipt and the worker record name the model that ran. They say when a fallback was used. This condition keeps the original defect closed. | Slice 2; section 6 |
| 5 | Second list 1, the keeper | Long term, the Athanor manages the harnesses below it, as Herdr or T3Code do. For now, keep today's arrangement to get results. The long-term direction is a later slice. It is no current work. | Slice 6; section 9 |
| 6 | Second list 6, identity record | The store is PostgreSQL or storage inside the app itself. Markdown files are not necessary unless the app selects that format. If the app stores data as files, it probably prefers JSONL. The answer covers `presence-pulse.md` and the body of `active_spirit.md`. | Section 4; slices 3 and 5; section 6 |
| 7 | Second list 7, installation folder | The person selects the folder at install. Each platform ships a default, for example `Program Files/Athanor` on Windows. Generated harness files follow the installation folder rule. | Slices 2 and 4; section 6; open question 5 |
| 8 | Second list 8, fallback chains | The model chooses a chain from the available model list when a spirit is summoned or a familiar is made. The person can change the chain later. Tuner's condition still holds. | Slices 2 and 4 |

Notes:

- Answer 6 replaces the word "probably" in answer 1. The owner is the app's own storage.
- The first list gave four options for the identity contract. They were: keep the file, move the text into the record, import the file on command, or leave the file to the harness. Answers 1 and 6 select the record.
- Answer 2 replaces the earlier rule "The Athanor runs no model and no worker".
- Answer 3 permits the slices. It opens none of them. The status line still applies.
- Answer 4 replaces the first version of slice 2, which allowed no fallback.
- Answer 5 keeps today's arrangement. Slice 6 holds the long-term direction.
- Answer 7 does not say that one folder holds the program files and the data. Open question 5 asks.

## 8. Open questions for Sol

Sol answered the other questions in two rounds (section 7).
Question 5 is the part of the installation folder question that has no answer.

1. **Lanes.** The Host holds four lanes with OMP agent names in Rust (`crates/host/src/routing.rs:103-157`). Do lanes become records also?
2. **Second harness.** `adapters/` holds one harness adapter, for OMP. Which harness proves the adapter contract next?
3. **Edit surface.** Answer 1 gives the operator UI as the later place for the identity contract. `AGENTS.md:52` names the web client at `gui-prototype/` as the main operator surface. Which surface edits records before that? Does the operator UI also edit familiars, rooms, and model chains?
4. **Working directory.** Where does a session start when a room has no folder?
5. **Installation folder.** Does one folder hold the program files and the data? Windows uses a program root and a data root today (`crates/athanor-install/src/layout.rs:16-27`).

## 9. Fit with the roadmap and the architecture

| Document part | Fit | Conflict or gap |
|---|---|---|
| [Roadmap step 1](./ROADMAP.md#1-freeze-the-boundary) | Sol's answer 3 allows slices 2 to 5 under the freeze. | The step text does not name this work. The ruling permits the slices and opens none. |
| [Roadmap step 3](./ROADMAP.md#3-make-the-host-the-only-client-boundary) | The direction continues this step. The Host decides. OMP registers, observes, and shows. | The adapter still reads room files (`room.ts:71-105`; `boat-door.ts:472-480`). Slice 5 removes these reads. |
| [Roadmap step 6](./ROADMAP.md#6-complete-the-operator-surface) | Sol's answer 1 makes the operator UI the later place to read and edit the identity contract. | The remaining list of step 6 has no identity editor. |
| [Roadmap step 7](./ROADMAP.md#7-make-installation-boring) | Slice 3 delivers "Add a room without a new release". Slice 4 extends the step. | Step 7 has no item for the first summoning or for one installation folder. Slice 4 adds both. |
| Roadmap row "Dynamic model and room execution" | The Athanor records the model chain for each spirit and familiar. | The row does not say who runs each model body. The harness runs the model calls of agents and workers. |
| [Architecture 3.6](./ARCHITECTURE.md#36-projections) | Dispatch starts no worker. | The section describes spellbook files. Slice 1 replaces that text. |
| [Architecture 4.2](./ARCHITECTURE.md#42-shared-behavior-and-harness-behavior) | | The section names OMP as the owner of model selection. Slice 2 moves the model chain to the Athanor. |
| [Architecture 4.4](./ARCHITECTURE.md#44-judgments-and-lifecycle) and 4.5 | Sol's answer 2 keeps native judgments, GIGA, and embeddings in the Athanor. | None after answer 2. |
| Architecture 1 and 5 | Sol's answer 5 keeps today's arrangement: the Host starts keepers, and each keeper starts OMP. | None now. Slice 6 records the long-term direction and its open fit with decision 4. |
| [Architecture 3.1](./ARCHITECTURE.md#31-one-process-many-rooms) | One Host serves every room. | Each room configuration needs a room directory. Slice 5 removes that need. |
| [Architecture 7](./ARCHITECTURE.md#7-installed-layout) | | The layout has a program root, a data root, and room files under `roomsRoot`. Sol's answers 1 and 7 ask for one installation folder that the person selects. The current default is `Solarisael\Athanor` under the program files folder (`installer/athanor.iss:13`). |
| [Architecture 7](./ARCHITECTURE.md#7-installed-layout), room text | | The section says that a new room needs a new release. `LIMITATIONS.md:50` and `INSTALL.md:524` say the opposite. These documents disagree today. |
| Architecture 8.1, invariants 1, 2, 3, and 7 | The rules in section 6 agree with them. | None found. |

When a slice lands, update the owning documents in the same change: `ARCHITECTURE.md`, `ROADMAP.md`, `LIMITATIONS.md`, `INSTALL.md`, and `IDENTITY_GUIDE.md`.
