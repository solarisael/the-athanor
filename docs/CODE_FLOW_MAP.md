# The Athanor Code Flow Map

This document maps the current first-party code by ownership, call boundary, and
runtime order. It is a source map, not a target-architecture proposal. Each
large path is split into a small diagram so that a reader can follow one event
without crossing the whole repository at once.

**Snapshot:** current working tree on 2026-08-17.

**Scope:** production Rust, the OMP adapter, authored Godot code, and operational
build/install scripts. Tests are proof satellites rather than runtime nodes.
`gui/addons/juicee/` is one third-party plugin boundary. `gui-prototype/` is a
standalone mock/specimen and has no production runtime edge.

## Reading the map

- Solid arrows are runtime calls, messages, data movement, or strict startup
  order.
- Dashed arrows are optional integrations, proof surfaces, or non-product labs.
- PostgreSQL is authoritative for AKASHA state. Vault files are authoritative
  only in the Vault profile. NATS carries delivery; it is not a second database.
- Function names appear where they define a cross-module transition. Small pure
  helpers remain in the module index instead of becoming graph noise.

## 1. Whole runtime

```mermaid
flowchart LR
    Operator["Operator / Windows SCM"] --> Manager["athanor-manage<br/>service::dispatch"]
    Manager --> Plan["runtime_plan"]
    Plan --> Supervisor["Supervisor::run"]

    subgraph Children["Managed runtime children"]
        PG[("PostgreSQL<br/>AKASHA authority")]
        NATS["NATS JetStream<br/>transport, not authority"]
        Delivery["athanor-delivery<br/>PostgreSQL ↔ NATS bridge"]
        Host["one Athanor Host<br/>per room"]
    end

    Supervisor --> PG
    Supervisor --> NATS
    Supervisor --> Delivery
    Supervisor --> Host

    OMP["OMP adapter"] <-->|"authenticated Host WebSocket"| Host
    Godot["Godot client"] <-->|"authenticated Host WebSocket"| Host
    OMP <-->|"long-lived JSONL child stdio"| Substrate["athanor-substrate"]

    Substrate <-->|"durable organ reads and writes"| PG
    Substrate <-->|"Vault profile only"| Vault[("room-attributed files")]
    Substrate -.->|"query embeddings"| Ollama["Ollama<br/>inference only"]
    Substrate --> GigaWorker["GIGA worker task<br/>inside substrate process"]
    GigaWorker <-->|"claim events / store candidates"| PG
    GigaWorker -.->|"bounded classifier / extraction"| Ollama

    PG -->|"claim crane_outbox rows"| Delivery
    Delivery -->|"mark published / record receipts"| PG
    Delivery -->|"publish and consume Crane subjects"| NATS
    NATS -->|"JetStream acknowledgements"| Delivery
    NATS -->|"sanitized receipt stream"| Host
    Host --> HostState[("room-scoped Host projection state")]
```

Startup order is PostgreSQL when managed, then NATS, delivery, and one Host per
configured room. Shutdown drains the same process plan in reverse. OMP can use
both boundaries: Host commands for interactive room state and substrate JSONL
for durable organs. Godot uses Host only.

NATS never opens a PostgreSQL connection. `athanor-delivery` is the
transactional-outbox bridge: it claims durable rows from PostgreSQL, publishes
them to JetStream, consumes delivered events, and records authoritative receipts
back in PostgreSQL.

GIGA's queue events, candidates, review transitions, and promotions are durable
PostgreSQL state. The worker runs inside the substrate process and calls Ollama
only for bounded classification or extraction. Ollama holds neither the GIGA
queue nor candidate authority.

Sources: `crates/athanor-install/src/service.rs`,
`crates/athanor-install/src/native_runtime.rs`,
`crates/athanor-install/src/supervisor.rs`, `crates/house-host/src/server.rs`,
`adapters/omp/rust-transport.ts`, `gui/src/host_session.rs`.

## 2. Rust workspace dependency DAG

Arrows point from a crate to its internal workspace dependency.

```mermaid
flowchart BT
    Core["house-core"]
    Protocol["house-protocol"]
    Vault["house-vault"]
    Substrate["house-substrate"]
    Delivery["house-delivery"]
    Host["house-host"]
    Install["athanor-install"]
    Godot["athanor-godot<br/>gui/src"]

    Protocol --> Core
    Host --> Core
    Host --> Protocol
    Host --> Delivery
    Substrate --> Core
    Substrate --> Protocol
    Substrate --> Vault
    Delivery --> Protocol
    Godot --> Protocol

    Proof["tests and smoke harnesses"] -.-> Core
    Proof -.-> Protocol
    Proof -.-> Vault
    Proof -.-> Substrate
    Proof -.-> Delivery
    Proof -.-> Host
    Proof -.-> Install
    Proof -.-> Godot
```

`house-core`, `house-vault`, and `athanor-install` have no internal workspace
crate dependency. Repository proximity does not erase authority boundaries:
Vault stays database-free, and the core does not import an adapter.

Source: workspace manifests resolved from `Cargo.toml` and each crate manifest.

## 3. Service boot and shutdown

```mermaid
sequenceDiagram
    participant SCM as Operator / Windows SCM
    participant Manage as athanor-manage service::dispatch
    participant Layout as current.json + config + secrets
    participant Plan as runtime_plan
    participant Sup as Supervisor::run
    participant PG as managed PostgreSQL
    participant NATS as NATS
    participant Delivery as delivery
    participant Host as Host per room

    SCM->>Manage: start
    Manage->>Layout: resolve active immutable version and topology
    Manage->>Plan: build ordered ProcessSpec list
    Plan-->>Sup: PostgreSQL? -> NATS -> delivery -> Hosts
    opt managed database mode
        Sup->>PG: spawn
        loop until deadline
            Sup->>PG: readiness check
        end
    end
    Sup->>NATS: spawn and await readiness
    Sup->>Delivery: spawn and await readiness
    loop each configured room
        Sup->>Host: spawn identity-bound Host and await /health
    end
    Sup-->>SCM: report RUNNING only after every child is ready

    SCM->>Manage: stop
    Manage->>Sup: stop process plan
    Sup->>Host: stop Hosts in reverse order
    Sup->>Delivery: stop
    Sup->>NATS: stop
    opt managed database mode
        Sup->>PG: stop
    end
```

A child spawn is not readiness. Failure before the full plan becomes ready is a
service-start failure, not a partial success.

Sources: `crates/athanor-install/src/service.rs`,
`crates/athanor-install/src/native_runtime.rs`,
`crates/athanor-install/src/supervisor.rs`.

## 4. One OMP context turn

The ordering below is the `pi.on("context")` path. Conversation capture occurs
before memo replay so detached GIGA ingestion is not skipped by a cache hit.

```mermaid
sequenceDiagram
    participant Pi as OMP context hook
    participant Adapter as adapters/omp/index.ts
    participant Host as Athanor Host
    participant Rust as RustJsonlTransport
    participant Data as substrate or Vault

    Pi->>Adapter: context event
    Adapter->>Adapter: load room directives, room state, active-spirit snapshot
    opt enabled room model default
        Adapter->>Adapter: resolve and apply model selector
    end
    Adapter->>Host: write conversation log
    Adapter-->>Rust: detached GIGA conversation ingest
    Adapter->>Adapter: inspect turn-addition memo/cache

    alt byte-stable memo hit
        Adapter->>Adapter: replay memoized additions
    else cache miss
        Adapter->>Host: analyze_context
        Host-->>Adapter: query classification and lane hints
        Adapter->>Adapter: add room and routing reminders
        opt fresh session
            Adapter->>Rust: paper_boat_wake
            Rust->>Data: load latest Boat and later unboated memories
            Adapter->>Rust: anamnesis read
        end
        Adapter->>Adapter: keyword directive
        Adapter->>Adapter: deterministic process-lesson plan
        Adapter->>Adapter: prose lesson-trigger matches
        Adapter->>Adapter: inspect Recall Policy
        opt unresolved entity affects classification
            Adapter->>Rust: entity_resolve
            Rust-->>Adapter: canonical entity evidence
            Adapter->>Host: analyze_context again
        end
        Adapter->>Host: evaluate Recall Policy
        alt Vault profile
            Adapter->>Rust: vault_recall
            Rust->>Data: file-authoritative bounded retrieval
        else AKASHA profile
            Adapter->>Rust: recall
            Rust->>Data: PostgreSQL-authoritative retrieval
        end
        Adapter->>Host: apply_viewport
        alt refresh succeeds
            Adapter->>Host: complete_refresh
        else refresh fails
            Adapter->>Host: fail_refresh
        end
        Adapter->>Adapter: record recall telemetry
        Adapter->>Adapter: memoize byte-stable additions
    end
    Adapter-->>Pi: anchor additions after the current user turn
```

Related lifecycle hooks:

```mermaid
flowchart LR
    Compact["session_compact"] --> Strip["remove recall working set"]
    Strip --> Invalidate["Host invalidateAfterCompaction"]

    Task["task tool_call / progress / lifecycle / result"] --> Normalize["Host lineage normalize / settle"]
    Normalize --> Lineage["write lineage memories"]

    End["agent_end"] --> Capture["capture final conversation"]
    Capture --> Giga["detached GIGA ingest"]

    Shutdown["shutdown"] --> Close["close Host and Rust transports"]
```

Sources: `adapters/omp/index.ts`, `adapters/omp/giga.ts`,
`adapters/omp/kitten-lineage.ts`, and `adapters/omp/solarisael-house-proof/`.

## 5. OMP tool ownership and transport

The current adapter registers 35 tools. A tool name is not an authority claim;
the owner column names the boundary that actually performs or persists the
operation.

| Tool | Runtime owner / boundary |
|---|---|
| `recall` | Rust JSONL -> substrate `recall` or `vault_recall` routing |
| `canon_read` | Rust JSONL -> PostgreSQL canon store |
| `canon_write` | Rust JSONL -> guarded PostgreSQL canon write |
| `remember` | Rust JSONL -> typed lesson transaction or memory transaction |
| `delete_lesson` | Rust JSONL -> lesson store mutation |
| `update_lesson` | Rust JSONL -> lesson store mutation |
| `wake` | Rust JSONL -> `paper_boat_wake` |
| `room_state` | Adapter -> room-state file |
| `set_room_state` | Adapter -> room-state file plus `active_spirit.md` snapshot |
| `lessons` | Rust JSONL -> typed lesson query |
| `design_doc` | Rust JSONL -> design-document query |
| `design_doc_write` | Rust JSONL -> guarded design-document write |
| `sleep` | Adapter flushes GIGA buffer, then Rust JSONL -> `paper_boat_sleep` |
| `house_lane_status` | Host lane status plus substrate health probe |
| `familiar_status` | Host routing boundary plus room spellbook file |
| `familiar_dispatch` | Host routing boundary plus room spellbook; returns a packet only |
| `house_dispatch` | Host routing boundary; returns a packet only |
| `house_routing_mode` | Adapter -> room-state file |
| `kitten_lineage_status` | Adapter-local lifecycle diagnostics |
| `recall_policy` | authenticated Host Recall Policy command |
| `house_model_default` | Adapter -> room-state file and current OMP model resolver |
| `anamnesis` | Rust JSONL -> PostgreSQL Anamnesis read |
| `anamnesis_write` | Rust JSONL -> guarded PostgreSQL Anamnesis write |
| `giga_candidate_list` | Rust JSONL -> GIGA candidate store |
| `giga_health` | Rust JSONL -> GIGA queue/store health |
| `giga_queue_maintenance` | Rust JSONL -> bounded GIGA queue maintenance |
| `giga_review` | Rust JSONL -> trusted GIGA review transition |
| `giga_promote_memory` | Rust JSONL -> transactional memory promotion |
| `giga_promote_coding_lesson` | Rust JSONL -> transactional coding-lesson promotion |
| `giga_promote_project_lesson` | Rust JSONL -> consent-gated project-lesson promotion |
| `hallway_create` | Rust JSONL -> Hallway store |
| `hallway_join` | Rust JSONL -> Hallway store |
| `hallway_post` | Rust JSONL -> Hallway store |
| `hallway_read` | Rust JSONL -> Hallway store |
| `hallway_inbox` | Rust JSONL manual read; Host-owned automatic projection |
| `hallway_knock_policy` | Rust JSONL -> append-only room-owned Hallway wake policy |
| `hallway_knock` | Rust JSONL -> bounded Hallway Knock lifecycle |

```mermaid
flowchart LR
    Tools["tools.ts registrations"] --> Local["adapter-local handlers"]
    Tools --> HostClient["Host command clients"]
    Tools --> RT["RustJsonlTransport.request"]

    Local --> Files[("room files / OMP session state")]
    HostClient <-->|"authenticated WebSocket"| Host["house-host router"]

    RT --> Ensure["ensureStarted"]
    Ensure --> Queue["queue and flush stdin line"]
    Queue --> Main["athanor-substrate main"]
    Main --> Decode["decode response stdout line"]
    Decode --> Handle["handleLine"]
    Handle --> RT

    Timeout["timeout after definitive dispatch"] --> Unknown["outcome unknown<br/>do not retry as if unsent"]
    RT -.-> Timeout
```

`RustJsonlTransport` keeps one child process alive and correlates JSONL request
IDs. A timeout after dispatch is explicitly outcome-unknown because replaying a
write could duplicate an operation whose response was merely lost.

Hallway Knock actuation follows a separate local path. The recipient Host claims
one PostgreSQL pointer under a short session lease; the OMP doorman includes only
trusted routing IDs in its custom wake message and invokes `pi.sendMessage` for
one turn. The spirit reads the exact Hallway message through the ordinary
untrusted social path. Started/completed/failed settlement returns through Host;
claiming never advances a Hallway read cursor or clears a Bell row.

Sources: `adapters/omp/solarisael-house-proof/tools.ts`,
`adapters/omp/rust-transport.ts`, `adapters/omp/solarisael-house-proof/host.ts`.

## 6. Substrate request router

```mermaid
flowchart TD
    Stdin["stdin JSONL"] --> Main["main.rs line loop"]
    Main --> Req{"ProtocolRequest"}

    Req --> Vault["vault_recall"]
    Vault --> VaultCrate["house-vault<br/>no database initialization"]

    Req --> Pre["health / migrations<br/>pre-init paths"]

    Req --> NeedDB["database-backed request"]
    NeedDB --> Lazy["lazy Config::from_env + pool"]
    Lazy --> Worker["spawn_giga_worker when enabled"]
    Lazy --> Router{"request family"}

    Router --> Continuity["canon, remember, paper boat,<br/>recall, entity, anamnesis"]
    Router --> Knowledge["lesson, lesson context/triggers,<br/>design documents, cluster"]
    Router --> Social["Hallway create/join/post/read/inbox<br/>Knock policy/create"]
    Router --> Giga["GIGA ingest, queue, worker,<br/>review, promotion, health"]
    Router --> Ops["substrate health and maintenance"]

    Continuity --> PG[("PostgreSQL")]
    Knowledge --> PG
    Social --> PG
    Giga --> PG
    Ops --> PG
    Worker -.-> Giga

    PG --> Resp["ProtocolResponse"]
    VaultCrate --> Resp
    Pre --> Resp
    Resp --> Stdout["stdout JSONL"]
```

The main binary owns protocol dispatch and lazy shared state. Domain modules own
the transactions; `main.rs` does not become a second implementation of their
rules.

Sources: `crates/house-substrate/src/main.rs`, `lib.rs`, `config.rs`,
`health.rs`, and the domain modules listed in the module index.

## 7. Recall paths

### AKASHA profile

```mermaid
flowchart TD
    Q["recall request"] --> Validate["validate and normalize"]
    Validate --> Parse["extract dates and terms<br/>rooms = room + house"]
    Parse --> Embed["optional Ollama query embedding"]

    Parse --> BM["BM25F candidates"]
    Parse --> Vocab["nearest semantic vocabulary"]
    Vocab --> Bridge["BM25F concept bridge"]
    Embed --> Vector["pgvector semantic chunks"]
    Parse --> Trgm["pg_trgm word-similarity chunks"]
    Parse --> Dates["date matches"]
    Parse --> Threads["lexical thread rows"]

    BM --> Fuse["weighted rank fusion"]
    Bridge --> Fuse
    Vector --> Fuse
    Trgm --> Fuse
    Dates --> Fuse
    Threads --> Fuse

    Fuse --> Bound["dedupe and truncate"]
    Bound --> Neighbors["hydrate thread-neighbor graph"]
    Neighbors --> Canon["attach active canon matches"]
    Canon --> Cluster["attach cluster resonance"]
    Cluster --> Result["evidence + taxonomy + warnings"]
```

### Vault profile

```mermaid
flowchart TD
    Q["vault_recall request"] --> Marker["load .solarisael-room.json"]
    Marker --> Collect["collect eligible attributed files<br/>respect ignores and limits"]
    Collect --> Parse["parse Markdown / JSON / JSONL / text"]
    Parse --> Docs["in-memory documents"]
    Docs --> Rank["BM25F rank"]
    Rank --> Bound["bounded attributed candidates"]

    NoDB["No PostgreSQL<br/>No writes"] -.-> Q
```

Sources: `crates/house-substrate/src/recall.rs`,
`crates/house-substrate/src/bm25f.rs`, `crates/house-vault/src/lib.rs`.

## 8. Host command cycle

```mermaid
flowchart TD
    Main["house-host main"] --> Config["HostConfig::from_env + validate"]
    Config --> New["Host::new"]
    New --> Load["load room state, cursor,<br/>sessions, receipts, ReceiptTracker"]
    Load --> Serve["serve"]
    Serve --> Axum["Axum /health + WebSocket"]
    Serve -.-> Bridge["run_receipt_bridge<br/>when NATS configured"]

    Axum --> Socket["handle_socket"]
    Socket --> Text["process_text"]
    Text --> JSON["parse JSON"]
    JSON --> Hash["semantic hash"]
    Hash --> Parse["parse_client_command"]
    Parse --> Validate["validate room/spirit/session binding,<br/>expiry and hop bounds"]
    Validate --> Route{"command router"}

    Route --> Receipt["receipt / recall subscribe,<br/>resync, ack"]
    Route --> Policy["Recall Policy set/evaluate/<br/>complete/fail/invalidate"]
    Route --> Context["analyze_context / apply_viewport"]
    Route --> Routing["lane status / dispatch / familiar status"]
    Route --> Lineage["lineage normalize / settle"]
    Route --> Shell["conversation log / lesson plan / braid"]

    Policy --> Mutate["commit_change"]
    Receipt --> Mutate
    Context --> Reply["typed direct event"]
    Routing --> Reply
    Lineage --> Reply
    Shell --> Reply

    Mutate --> Version["compute mutations, version, state hash"]
    Version --> Store["persist policy, sessions, cursor,<br/>idempotency receipt"]
    Store --> Reply
    Store --> Delta["broadcast ordered delta"]
    Reply --> Socket
    Delta --> Socket

    Validate -->|"invalid"| Reject["typed rejection; no mutation"]
    Bridge --> Tracker["filter accepted / duplicate / stale /<br/>foreign / malformed receipts"]
    Tracker --> Delta
```

Sources: `crates/house-host/src/main.rs`, `config.rs`, `server.rs`, `policy.rs`,
`store.rs`, `receipt.rs`, `viewport.rs`.

## 9. Paper Boat, Crane delivery, and receipt projection

```mermaid
sequenceDiagram
    participant Tool as OMP sleep tool
    participant Sub as paper_boat_sleep
    participant PG as PostgreSQL
    participant Delivery as DeliveryService
    participant NATS as JetStream
    participant Host as Host receipt bridge
    participant Godot as Godot client

    Tool->>Sub: room + standalone Boat body
    Sub->>Sub: hash db-only source path and prepare memory
    Sub->>PG: BEGIN
    Sub->>PG: write_memory_tx + continuation graph
    PG->>PG: trigger inserts crane_outbox boat.ready
    Sub->>PG: select outbox event id
    Sub->>PG: COMMIT
    opt configured filesystem backup
        Sub->>Sub: post-write backup
    end
    Sub-->>Tool: durable receipt

    Delivery->>PG: Store::claim_next
    PG-->>Delivery: pending Crane outbox row
    Delivery->>Delivery: parse and validate CraneEvent
    Delivery->>NATS: Broker::publish lane subject
    NATS-->>Delivery: publish acknowledgement
    Delivery->>PG: Store::mark_published

    NATS-->>Delivery: consume boat.ready before addressed Crane
    Delivery->>Delivery: enforce subject, lane, recipient, expiry
    Delivery->>PG: Store::record_receipt authoritative transaction
    Delivery->>NATS: sanitized BoatReceiptProjection
    Delivery->>NATS: acknowledge consumed event
    NATS-->>Host: receipt stream
    Host->>Host: ReceiptTracker classification
    Host-->>Godot: subscribed receipt snapshot / delta
```

`paper_boat_wake` does not trust a NATS payload as continuity. It reloads the
latest complete Boat from PostgreSQL and adds a bounded set of later unboated
memories. Duplicate delivery replays safely; poison events dead-letter and
transient failures nack within bounded retry policy.

Sources: `crates/house-substrate/src/paper_boat.rs`, migrations `0016` and
`0017` in `crates/house-substrate/src/migrations.rs`,
`crates/house-delivery/src/`, `crates/house-host/src/receipt.rs`.

## 10. GIGA candidate lifecycle

```mermaid
flowchart TD
    Conversation["OMP conversation"] --> Capture["capture transcript + private source ledger"]
    Capture --> Ingest["GIGA conversation ingest<br/>bounded turn windows"]
    Ingest --> Events[("PostgreSQL<br/>giga_events: pending")]

    Events --> Claim["worker claim"]
    Claim --> Validate["validate claim and event bounds"]
    Validate --> Sources["resolve exact private-ledger sources"]
    Sources --> Ollama["Ollama inference<br/>gate + extraction only"]
    Ollama --> Decision{"worthy candidate?"}
    Decision -->|"no"| Finish["finish event without candidate"]
    Decision -->|"yes"| Store["atomically store one GigaCandidate<br/>in PostgreSQL and finish event"]
    Validate -->|"retryable failure"| Retry["bounded retry then failed"]

    Store --> Proposal["candidate proposal<br/>not authority"]
    Proposal --> Review["giga_review"]
    Review --> ReviewChecks["trusted room + reviewer identity +<br/>current state + exact source resonance"]
    ReviewChecks --> InReview["durable PostgreSQL<br/>in_review transition"]

    InReview --> Promote{"promotion tool"}
    Promote --> Authority["re-check authority, state, and sources"]
    Authority --> Memory["transactional memory write"]
    Authority --> Coding["transactional coding lesson write"]
    Authority --> Consent["explicit project publication consent"]
    Consent --> Project["transactional project lesson write"]
    Memory --> Record["record promotion"]
    Coding --> Record
    Project --> Record
```

A candidate never becomes evidence merely because the worker extracted it.
Review and promotion are separate durable transitions. Project lessons add an
operator publication-consent gate.

Sources: `adapters/omp/giga.ts`, `crates/house-substrate/src/giga.rs`,
`crates/house-substrate/src/giga_worker.rs`,
`crates/house-core/src/conversation.rs`.

## 11. Native release, install, rollback, and removal

### Release assembly

```mermaid
flowchart LR
    Pins["installer/dependencies.json"] --> Fetch["fetch pinned PostgreSQL,<br/>pgvector, NATS, Godot"]
    Fetch --> Verify["hash verification + cache"]
    Verify --> Cargo["cargo release-build<br/>five binaries + Godot cdylib"]
    Cargo --> Stage["stage runtime, adapter, Godot, manager"]
    Stage --> Import["Godot headless import"]
    Import --> Manifest["hash payload and emit release manifest"]
```

### Install or update

```mermaid
flowchart TD
    Start["ReleaseManifest validate"] --> Config["load House config"]
    Config --> Preflight["platform + payload hash preflight"]
    Preflight --> Mutable["create and ACL-restrict mutable dirs<br/>and room state"]
    Mutable --> Legacy["one-time legacy import"]
    Legacy --> Backup["pre-upgrade database backup"]
    Backup --> Stop["stop service"]
    Stop --> Stage["stage, copy, write manifest,<br/>atomic version rename"]
    Stage --> Stable["update stable manager, runtime config,<br/>secrets, OMP client projection"]
    Stable --> Pointer["atomically update current.json"]
    Pointer --> Migrate["run production migration registry"]
    Migrate --> Service["install or update Windows service"]
    Service --> Run["start service and await health"]
    Run -->|"ready"| Loader["register stable OMP loader"]

    Stage -->|"failure"| Restore["restore pointer and database;<br/>remove failed first install"]
    Stable -->|"failure"| Restore
    Pointer -->|"failure"| Restore
    Migrate -->|"failure"| Restore
    Run -->|"failure"| Rollback["rollback to prior release"]
```

### Explicit rollback and removal

```mermaid
flowchart TD
    Rollback["rollback command"] --> Undo["take undo backup"]
    Undo --> Stop["stop service"]
    Stop --> Pointer["flip current pointer to prior version"]
    Pointer --> DB["restore prior database backup"]
    DB --> Start["start and await readiness"]
    Start -->|"failure"| Newer["restore newer release"]

    Uninstall["uninstall"] --> Remove["remove service, operator integration,<br/>and program files"]
    Remove --> Preserve["preserve operator data"]
    Purge["purge + explicit confirmation"] --> Destroy["remove operator data"]
```

Sources: `installer/build-native-release.ps1`,
`installer/native-build-cache.ps1`, `crates/athanor-install/src/installer.rs`,
`manifest.rs`, `layout.rs`, `boundaries.rs`, `omp.rs`, `service.rs`, and
`supervisor.rs`.

## 12. Godot client and authored UI

```mermaid
flowchart TD
    Host["Athanor Host WebSocket"] <--> Session["AthanorHostSession"]
    Session -->|"owns credentials and transport"| Link["HostLink"]
    Session --> Protocol["protocol.rs<br/>wire vocabulary, parsers, delta application"]
    Protocol --> Shell["shell.rs router"]

    Shell --> Resume["Resume"]
    Shell --> Policy["RecallPolicy"]
    Shell --> Routing["Routing"]
    Shell --> Familiar["Familiars"]
    Shell --> Dispatch["Dispatch"]
    Shell --> Health["Health"]

    Policy -->|"only authored mutation"| Requested["requested Recall mode"]
    Routing --> ReadOnly["read-only projections"]
    Familiar --> ReadOnly
    Health --> ReadOnly
    Dispatch --> Packet["validated OMP spawn packet<br/>never executes it"]

    Session --> Boat["PaperBoatReceipt state"]
    Boat --> Chat["S01ChatCenter"]
    Chat --> Composer["Composer<br/>visibly disabled: no conversation command"]
    Chat --> Message["MessageCard"]
    Chat --> Receipt["ReceiptCard"]
    Chat --> Disclosure["DisclosureBanner"]
    Shell --> Nav["ReliquaryNavigator"]

    Effects["Effects Lab"] -.-> Juicee["third-party Juicee plugin"]
    Prototype["gui-prototype/<br/>disconnected mock/specimen"]
```

The Rust extension owns Host protocol state. GDScript composes the visible
instrument. Generic design-system components present state but do not gain
persistence or transport authority. The composer remains disabled because the
Host protocol does not implement a conversation-send command.

Sources: `gui/src/`, `gui/screens/s01_chat_center.gd`,
`gui/design-system/components/`, `gui/navigation/reliquary_navigator.gd`,
`gui/effects_lab/effects_lab.gd`, `gui/main.tscn`.

## 13. Module ownership index

### Rust production modules: 64

| Crate | Modules | Boundary |
|---|---|---|
| `house-core` (8) | `lib`, `context`, `conversation`, `hallway`, `lesson_triggers`, `lineage`, `routing`, `triggers` | Provider-neutral domain types; context classification; transcript/source identity; Hallway validation; trigger compilation; quest normalization; lane and dispatch contracts |
| `house-protocol` (2) | `lib`, `host` | Substrate JSONL DTOs/results/errors and Host command/event envelopes |
| `house-vault` (1) | `lib` | Strict file-authoritative, database-free Vault retrieval |
| `house-substrate` (19) | `lib`, `main`, `config`, `state`, `migrations`, `backup`, `health`, `remember`, `recall`, `bm25f`, `canon`, `entity`, `anamnesis`, `lesson`, `cluster`, `paper_boat`, `hallway`, `giga`, `giga_worker` | JSONL dispatch, PostgreSQL configuration and migrations, durable organs, retrieval, GIGA, backup, and health |
| `house-delivery` (5) | `lib`, `main`, `model`, `broker`, `store` | Crane envelope validation, PostgreSQL outbox/receipt store, JetStream publication and consumption |
| `house-host` (8) | `lib`, `main`, `config`, `server`, `policy`, `store`, `receipt`, `viewport` | Authenticated client boundary, room projection state, Recall Policy, ordered deltas, receipt bridge, viewport shaping |
| `athanor-install` (10) | `lib`, `main`, `layout`, `manifest`, `boundaries`, `installer`, `native_runtime`, `omp`, `service`, `supervisor` | Installed layout, release validation, transactional update/rollback, runtime planning, Windows service, OMP integration |
| `athanor-godot` (11) | `lib`, `host_link`, `host_session`, `protocol`, `shell`, `recall_policy`, `routing`, `familiar_status`, `dispatch`, `health`, `paper_boat_receipt` | Thin native client transport, exact Host wire contract, projections, shell routes, and Paper Boat receipt state |

### OMP adapter modules: 27

| Area | Modules | Boundary |
|---|---|---|
| Adapter entry and loading | `index.ts`, `athanor-root.ts`, `installed-loader.ts`, `discovery.ts` | OMP registration, installed-source resolution, room discovery, lifecycle hooks |
| Transport and capture | `rust-transport.ts`, `giga.ts`, `kitten-lineage.ts` | Long-lived substrate child, transcript/source-ledger ingestion, task lifecycle bridge |
| Standalone guard | `hygiene.ts` | Repository/packaging hygiene check; not a session runtime import |
| House proof runtime | `tools.ts`, `host.ts`, `context.ts`, `recall-policy.ts`, `recall.ts`, `substrate.ts`, `room.ts`, `conversation-log.ts`, `lesson-triggers.ts`, `lesson-context.ts`, `triggers.ts`, `routing.ts`, `lineage.ts`, `entity-resolution.ts`, `anamnesis.ts`, `recall-telemetry.ts`, `feedback.ts`, `text.ts`, `constants.ts` | Tool registration, Host clients, context assembly, retrieval routing, room files, lessons, lineage, entity handling, telemetry, and shared text/constants |

### Authored Godot GDScript

| Area | Modules | Boundary |
|---|---|---|
| Product screen | `screens/s01_chat_center.gd` | S01 composition and visible session/receipt state |
| Navigation | `navigation/reliquary_navigator.gd` | Shell navigation projection |
| Design components | `receipt_card`, `consequence_button`, `composer`, `message_card`, `evidence_card`, `status_channel`, `text_action`, `disclosure_banner`, `field_row`, `reliquary_panel`, `page_header`, `ritual_surface` | Presentation components; no persistence or transport authority |
| Effects lab | `effects_lab/effects_lab.gd` | Separate visual experiment using the external Juicee plugin |
| External plugin | `addons/juicee/` | Third-party effect implementation treated as one dependency node |
| Proof satellites | `navigation/tests/`, `screens/tests/`, `effects_lab/tests/` | Godot smoke and contract checks; not runtime ownership |

### Operational surfaces

| Path | Responsibility |
|---|---|
| `installer/build-native-release.ps1` | Fetch, verify, compile, stage, import, hash, and assemble a native payload |
| `installer/native-build-cache.ps1` | Deterministic dependency cache used by release assembly |
| `installer/athanor.iss` | Windows installer packaging |
| `substrate/deploy-local.ps1` | Local substrate deployment helper |
| `substrate/state_paths.sh` | Shared local state-path resolution for substrate operations |
| `.github/workflows/` | CI and release proof; not runtime |
| `tests/`, crate tests, `substrate/tests/` | Cross-boundary proof satellites |
| `gui-prototype/` | Disconnected HTML/CSS/JS mock and design specimen |

## 14. Fast traversal paths

- **A prompt gains continuity:** section 4 -> section 5 -> section 6 -> section 7
  -> section 8 -> back to the OMP context.
- **A Boat becomes visible in Godot:** section 9 -> Host receipt bridge in section
  8 -> Godot receipt projection in section 12.
- **A conversation proposes durable knowledge:** section 4 capture -> section 10
  candidate/review/promotion -> substrate transaction in section 6.
- **The installed system starts:** section 11 install -> section 3 service plan ->
  section 1 runtime topology.
- **A worker is selected:** OMP tool table -> Host routing family in section 8 ->
  returned spawn packet; the main model still invokes the OMP task tool explicitly.
