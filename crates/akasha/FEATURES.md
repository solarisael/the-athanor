# akasha

Not re-verified at a6ab453.
The 2026-10-04 census did not walk `crates/akasha`; claims below keep their earlier date.

The crate is the Athanor substrate: one stdio server over PostgreSQL. `lib.rs` declares the 21 modules and re-exports their doors. Each section below names one module and what it does today.

### main.rs — the stdio tool-dispatch table

- The binary reads one JSON request per line from stdin. It writes one JSON response per line to stdout.
- `ProtocolRequest` holds 46 method variants. `decode_line` selects the variant and parses its parameters.
- The dispatch loop validates the request, calls the module door, and serializes the result.
- `operation_name` maps every variant to one observation name. A new method cannot reach the loop without a name.
- `insula_binding` gives Hallway and Docket requests the caller's room, spirit, and session. Other methods use the House service voice.
- `start_span` and `end_span` wrap each dispatch, so the loop closes one observation at one door.
- The runtime starts lazily at the first database method. It builds the `Config`, the pool, the GIGA worker, and the Insula emitter.
- `substrate_health`, `vault_recall`, and `substrate_migrations` answer before the pool opens.
- `protocol_error_class`, `app_error_class`, and `backup_error_class` name the failure for the observation. `app_error_outcome` marks refusals as refusals.
- `cli_subcommand` runs `backup`, `restore`, `health`, `migrations`, and `semantic-vocabulary-refresh`.
- `spawn_retention_service` checks expired raw observations once each day. The first check waits five minutes.
- Raw observations expire after seven UTC days. Each sweep checks expiry against the current UTC minute.
- `WslKeepalive` holds a helper process open on Windows and terminates it on drop.

### orihon_folder/ — the live compactor engine

- `Engine` builds one live-tree node line per `summarize` call. The Host opens one engine per room prefix and folder model, picks the nodes, writes their prompts, and stores every returned job.
- A call has the turn's shape: the harness's exact system parts and tool definitions (never executed), one user message with the compactor view's whole 4-line blocks as separate text parts, then one user message with the tail and the task.
- A line over 512 bytes gets the same-conversation "Too long" correction. After five replies the shortest complete one stands, up to the 16 KiB model-line ceiling. A reply cut by the output budget is never kept.
- A failed call ends its job. The Host retries failed work after 60 seconds while a registered fold session participates.
  A message boundary can retry sooner. Loss of consent or room ownership stops automatic retries.
- Every job receipt lists each attempt: outcome, bytes, input/output/cache-read/cache-write tokens, price, stop reason, and time. An unreported count stays empty, and a missing price names its reason.
- An engine runs at most eight jobs at once and owns their cancellation. `close` waits for running jobs, then ends the gateway child and its overlay.
- Local engines call a loopback Ollama endpoint with the same system parts and the tool definitions in Ollama's function shape; a reply that asks for a tool fails the attempt. The prompt, tool bytes included, must fit the configured `num_ctx` (default 32768) at one token per byte, or the attempt refuses before any request. Ollama reports no price.
- Hosted engines own one `omp auth-gateway stdio` child, started with the operator's binding and an overlay that turns off model fallback. Up to eight requests share it by id. A child that exits is replaced at the next call.
- Hosted calls use `POST /v1/pi/stream` without streaming. OMP's final message carries the token counts and `usage.cost.total`; the stdio wire forwards no response headers. Requests ask for short cache retention and pin one OMP session per engine.
- A hosted reply must name the configured provider and model. Tool calls and other non-text replies fail the attempt.
- For Anthropic, a first attempt waits while another call writes a cache entry its prefix holds. stdio answers each request whole, so the wait lasts until the writer's reply arrives, not until its response starts. A cold prefix costs the waiting calls about one call's latency.
- Cache scopes cover backend, provider, model, binding, and prefix digest. Nothing claims that two scopes share cache entries.

### remember/ — the durable write

- `remember` takes `hearth::RememberRequest`, already validated by its constructor, and returns `protocol::RememberResult`.
- `remember` writes one memory in one transaction: the row, its threads, its chunks, and its continuations.
- Memory identifiers cross from `u64` to PostgreSQL BIGINT at the bind. An identifier outside that range refuses there.
- `write_memory_tx` upserts the memory on (room, source_path). It refuses a conflicting paper boat and returns the id plus an inserted flag.
- `write_memory_tx` also links threads, prunes dropped thread events, marks superseded memories, and rewrites every chunk row.
- `threads_with_inherited` adds the threads of each superseded memory to the new memory before the write.
- `write_continuations_tx` records which memory a thread continues from.
- `continue_by_date_tx` links a thread member with no predecessor to the latest earlier member by memory date, then by ID. It keeps links that exist.
- `memory_thread_join` adds existing readable memories to one thread in one transaction. It preserves memory text and applies the date default to the joined memories.
- `memory_thread_leave` removes readable memories from one thread in one transaction. It moves each membership into `thread_event_archive` with the reason and the operator. The successors continue from the predecessor. It removes the key from `memories.threads` and keeps the memory text.
- `memory_ids` and `lock_room_memories` are the shared refusal rule of the join and the leave. One ID that is not a readable, unarchived memory of the room refuses the whole call.
- `memory_amend` records that a newer memory corrects or updates an older memory, in table `memory_amendments`. It locks both readable, unarchived memories of the room and refuses a newer memory with an earlier memory date. The same pair with the same text changes nothing; other text for the pair refuses. It never changes a title, a body, `superseded_by`, or `archived_at`.
- `amend_tx` is the shared amendment writer. `remember` calls it in the write transaction for each entry of `amends`, so one refused amendment refuses the whole write.
- `remember_lesson` writes five lesson kinds: coding, project, writing, design, and audio. It refuses a trigger pattern that does not compile before the transaction opens.
- Five named lesson writers exist, one per kind: `write_coding_lesson_tx`, `write_project_lesson_tx`, `write_writing_lesson_tx`, `write_design_lesson_tx`, and `write_audio_lesson_tx`. Each sends one jsonb row to `jsonb_populate_record`; `remember_lesson` only dispatches.
- `prepare_memory_write` is the shared preparation step. It normalizes threads, derives dates, chunks the body, and asks for vectors.
- `chunk_body` splits the body on `## ` headings. A section above 4000 characters splits again on paragraphs, with a 200 character overlap.
- `derive_dates` reads dates out of the source path. A stitched path date moves to the next day.
- `embed` posts the chunks to the embedding endpoint with the `passage: ` prefix. It checks the vector count and the dimension.
- `normalize_strings` and `token_estimate` clean the inputs the writers store.
- `remember` calls `backup::post_write_outcome` after its commit. The receipt reports `skipped`, `ok`, or `failed`. A successful backup includes the dump path, sha256, bytes, elapsed time, and `pg_dump` route. A failed backup adds one warning with a code and reason. The committed write stays successful.

### recall/ — the retrieval door

- `recall` takes `hearth::RecallRequest`. The wire defaults live on `protocol::RecallParams`.
- `recall` runs the whole retrieval and fuses the lanes into `retrievalCandidates`. It never fails on one absent lane; it adds a warning instead.
- The semantic lane embeds the original query and ranks chunks by cosine similarity.
- A below-floor result can cause one subject-term retry with the same scope, floor, and limits.
- Retry evidence identifies its pass. A retry failure preserves lexical results.
- The content lane filters on retained content terms and ranks chunks by trigram `word_similarity`.
- `recall/semantic.rs` owns both semantic passes. `recall/evidence.rs` owns query terms and candidate evidence.
- `load_bm25f_candidates` and `load_bm25f_candidates_for_terms` load the field-aware lexical candidates over the memory fields.
- `load_semantic_vocabulary_concepts` reads concept vectors and turns them into extra lexical terms. This is the bridge from meaning to exact words.
- `refresh_semantic_vocabulary` is the write path of that vocabulary. It re-embeds the concept rows and reports how many it refreshed.
- `memory_temporal_factor` applies optional age decay to memories from every origin. The default base half-life is 15 days.
- Memories use their stored `created_at` timestamp unless valid anchor metadata exists. Reviewed metadata in `meta.giga` supplies durability and the original candidate timestamp.
- Missing or invalid durability uses the base half-life. Invalid anchor metadata uses `created_at`. Future anchors receive no age penalty.
- The weight is `2^(-age_days * (1 - durability)^2 / half_life_days)`. Durability 1 receives no age penalty.
- Semantic, content, and BM25F lanes apply the weight before their final limits. Decay changes ranking, never similarity floors or stored authority.
- Automatic context requests decay. Explicit recall and write reconciliation leave decay off. Retrieval never updates a memory timestamp.
- `load_thread_neighbors` attaches the neighbors of each returned memory on its threads.
- `load_thread_memberships` attaches `threads` to each memory candidate in one query: every active thread key with the readable `previousMemoryId` and `nextMemoryIds`.
- `load_amendments` attaches `amendedBy` and `amends` to each memory candidate in one query. It omits each amendment whose other memory the reader cannot read. An amendment never changes the ranking.
- `attach_memory_links` runs the three loaders once for the final candidate set.
- The canon lane matches named entities three ways: whole phrase, exact token, then fuzzy similarity.
- `protocol_pointer_files` normalizes stored pointer files to the wire shape `{file, lines}`. Legacy strings and extra keys stay in the database.
- `embed_texts`, `embed_text`, and `embed_query` are the embedding client. The `query: ` prefix is load-bearing and pairs with the indexer prefix.
- `query_terms`, `query_dates`, `term_evidence`, `candidate_terms`, and `bounded_excerpt` build the evidence each candidate carries.
- `source_passage` selects at most 384 Unicode scalar values from actual source text.
- Passage offsets address the source chunk, or the complete record when no chunk index exists.
- Prefix matching uses query-sized state and bounded source-offset queues.
- BM25F selects passages after ranking, memory deduplication, and the requested result limit.
- The broad reranking limit includes the requested cards plus exact records that the sidecar excludes.
- Manual hydration still returns complete selected record bodies.
- `recall` also returns date matches, the room taxonomy, cluster staleness, and cluster resonance.
- `ScopeFilter` uses the operator visibility rule when the `operators` table exists, even when it has no rows.
- Readers see their own rows, company rows, and historical rows with no operator. The registered hierarchy can also grant `up` rows.
- An unregistered reader gets a warning only when the registry has rows.
  The filter hides other operators' private and `up` rows even when the registry is empty.

### giga/ — the Stage 1 candidate plane

- Event ingest. `giga_event_ingest` writes one event and its sources in one transaction. `lifecycle_json` and `lifecycle_from_json` carry the typed lifecycle.
- Conversation capture accepts up to 64 turns per transaction. Each new turn receives one job, identified by room, session, and source ID.
- Repeated identities must retain their role, content hash, and project scope. A conflict refuses the transaction.
- `giga_turn_release` releases waiting jobs when a conversation reaches its count or oldest-turn deadline. New arrivals never postpone that deadline.
- Migration 0043 preserves processed identities and archives replaced windows. Failed-only turns remain parked until explicit replay.
- Candidate store. `giga_candidate_store` verifies the parent event and every source before it writes the candidate.
- Candidate list. `giga_candidate_list` reads candidates with their sources for review.
- Room review. `giga_review` applies one review transition. It refuses a changed state, a crossed room boundary, a dropped source, and any durable transition.
- Tool review. `giga_tool_review` derives the previous state, the sources, and the review time from the store, then calls `giga_review`.
- Queue maintenance preserves unreleased turns, candidate evidence, archived windows, and turn identity tombstones. A purged job cannot return through repeated capture.
- Health excludes replaced windows from active counts. `identity_conflict_count` keeps unresolved migration conflicts visible.
- `giga_event_claim` leases one released event. `giga_event_finish` closes its attempt. `giga_event_replay` rejects archived windows.
- Claiming quarantines invalid stored events as failed. Later claims can process healthy neighbors.
- Promotion. `giga_promote` writes the durable memory, coding lesson, or project lesson, and marks the candidate promoted.
- Promotion is idempotent by request digest. A replay returns the stored receipt and checks that the durable row still exists.
- `giga_tool_promote` builds the promotion request from the room context and the stored candidate.
- `verify_resonance`, `fresh_candidate_sources`, and `promotion_digest` keep authority tied to the exact reviewed sources.

### giga_worker/ — the typed turn classifier

- The Host supplies a room-approved Jev configuration and transient Typesafe credential.
- The worker releases batches and processes jobs concurrently. Defaults are eight turns, 120 seconds, and four active jobs.
- Each job has a 120-second deadline within its 300-second lease. Shutdown cancels and joins owned jobs; unfinished leases remain recoverable.
- Exact ledger repeats count once. Conflicting rows still refuse source verification.
- Each target receives at most seven nearby turns and two Recall lookups. Jev can select one follow-up lookup.
- Recall retains the trusted Host scope. Sending recalled content also requires the current Recall private-packet grant.
- Explicit historical memory references retain their archived or superseded labels. They do not become current authority.
- Nearby turns and recalled records supply context only. Candidates cite their target turn.
- Empty or unavailable memory context cannot support a novelty score. Packet fitting removes context before target evidence.
- Candidate labels and excerpts derive from exact sources. Candidate insertion and event completion share one transaction.
- Completed classification remains durable if optional Curio resurfacing fails.
- The resonance pass compares later evidence with four recent Curios in the same room.
- Strong resonance returns a Curio to review and records its evidence, score, and actual model identity.
- Unavailable Curio evidence is skipped with a diagnostic.
- Classification and resonance never promote a candidate. The existing review tools retain that authority boundary.
- `verify_promotion_sources` checks exact ledger evidence again before explicit promotion.
- Health separates classifier availability from capture. Missing model digests are reported as absent.

### docket/ — the cooperation plane

- Capability gate. `require_docket_capability` hashes the supplied capability and compares it in constant time. Every write passes this gate first.
- Quest post. `quest_post` drafts and activates goals and quests. A replayed draft returns the existing row instead of a twin.
- Quest board. `quest_board` reads the offered and claimed work by deadline, with acceptance counts.
- Quest claim. `quest_claim` mints one 15-minute lease token and shows it exactly once.
- Quest report. `quest_report` records progress, submits an attempt, and settles one acceptance item. `rearm_recurrent_quest` re-offers a recurrent quest.
- Ledger. `insert_receipt`, `insert_event`, and `insert_goal_event` append the receipts and the events behind every action.
- Clock. `quest_clock` sweeps due deadlines, rings the Bell as the `clock` presence, then writes one ping per newly due deadline. It decides nothing else and is safe to repeat.
- Chargebook. `quest_chargebook` derives one attempt's token and byte cost from Insula. It grants no capability.
- Evidence. `quest_evidence` returns the full receipt bodies, the ledger events, and the acceptance verdicts. It grants no capability.
- Validation helpers refuse a bad room, spirit, session, UUID, duration, and any field the action does not accept.

### lesson/ — the lesson registry and the design catalogue

- Lesson query. `lesson_query` reads one lesson family with filters for scope, project, shape, register, stage, language, and technology. It ranks by always-on, then text rank, then update time.
- The query expands results along shared thread keys within the caller's total limit.
- Exact-ID queries without a limit return all matching requested IDs within scope.
- Trigger match. `lesson_trigger_match` fires trigger-bearing lessons against at most 16 offered surfaces. `trigger_eligibility` is the one visibility rule every trigger read pushes.
- Trigger engine. `trigger/engine.rs` owns the compilers: one table of 19 file extensions maps a path to its language slug and its optional ast-grep grammar, so the language fence and the grammar lookup cannot disagree. A row without a grammar fences regex conditions only; an ast condition on it skips with a warning.
- A lesson fires once for each call, on the first surface and pattern that catches it. A compiled set covers one room, or one room and one project, and the cache replaces it when the fingerprint changes.
- `validate_patterns` is the write-time refusal for a regex that does not compile or an ast pattern no grammar parses. `remember_lesson` and `lesson_update` call it before any row is touched.
- Lesson context. `lesson_context` returns the lessons a context activates, with the matched terms, shapes, and projects.
- Lesson update. `lesson_update` patches one lesson by id and title. `patch_trigger_spec` types the trigger columns; the patch is judged by shape and by pattern before the row is locked.
- Lesson delete. `lesson_delete` removes exactly one lesson. Both mutations refuse on a title mismatch and return a typed refusal receipt.
- Design document query. `design_document_query` reads the House design catalogue by type, status, and text.
- Design document write. `design_document_write` writes one catalogue entry. `valid_doc_type` limits the type to token, component, contract, or guideline.
- `LessonFamily` names the five families: coding, project, writing, design, and audio.

### insula/ — the observation store

- `ingest_batch` writes up to 512 observation events in one transaction. It returns the accepted rows and every conflict.
- `derive_idempotency_key_v1` and `derive_semantic_hash_v1` are the two identity recipes. The semantic hash excludes transport identity, so a failover redelivery matches.
- `binding`, `is_house`, `is_room`, `atom`, `opaque`, and `uuid` validate every field. `validate_trusted_binding` is the public gate.
- `event` validates one observation: phase, outcome, counts, timestamps, and expiry.
- `vitals` rolls each event into the per-minute vitals row.
- `query_trace` reads one trace inside the caller's scope, up to 1000 rows.
- `query_vitals` reads the minute rollups, up to 5000 rows.
- `run_retention` deletes expired raw observations only after it verifies complete Vitals coverage. It preserves summaries, tombstones, and deletion receipts.
- `ingest_transitions` writes state-machine moves to `insula.transitions` and adds each new row to its `insula.transitions_daily` edge count in one statement. A replayed row counts once.
- `run_transition_retention` deletes raw transitions older than `TRANSITION_RETENTION_DAYS` (30). The daily counts stay. The native retention job runs it after the raw-log sweep.
- `query_retention` reads the sweep receipts with their tombstone summary, up to 100 rows.
- `query_unverified_exit` reads, for one room, the sessions whose restart intent reached `exiting` and never reached `verified` inside the stage window, up to 100 rows. It observes the restart plane and commands nothing.
- `lock` takes an advisory lock so two writers cannot race on one logical key.
- `InsulaError` carries the field and the code of every refusal.

### insula_writer.rs — the in-process emitter

- `init_insula_emitter` installs the process emitter once and starts the drain task.
- `start_span` and `end_span` record the start and the end of one operation, with the duration.
- `record_point` records a single event with no span, such as a backup receipt.
- `record_transitions` queues the moves a Hearth machine committed. The Hallway knock wrappers in `hallway.rs` call it with the presence that made the call.
- The queue holds 512 pending observations and transitions together. An overflow increments a drop count that the next observation carries.
- `drain_loop` batches the queue. `persist_groups` groups the batch by binding, then calls `ingest_transitions` and `ingest_batch`.
- `flush_insula_emitter` drains on shutdown within 750 milliseconds.
- `system_binding` is the House service voice for work with no caller.
- `mechanical_name` and `opaque_identifier` refuse any caller text inside an observation.
- `disabled_by_environment` turns the emitter off.

### restart/ — the restart intent plane

- `restart_request` records one live intent per workspace. `restart_claim` hands the keeper that intent.
- `restart_transition` walks the intent through its states under the stage deadlines. `restart_verify` is the only door that can call a successor proved.
- `restart_status` answers two questions: without an id, the workspace's pending intent; with an id, that exact intent in whatever state it reached, so a session can see its own successor.
- `EXITING_DEADLINE_SECS`, `RELAUNCHING_DEADLINE_SECS`, `REQUESTED_TTL_SECS`, `RELAUNCH_ATTEMPT_LIMIT`, `STORM_WINDOW_SECS`, and `STORM_MAX_EXITING_PER_WINDOW` are the one authority for the deadlines and the storm bound; `insula::query_unverified_exit` reads its window from here.
- `authority` holds `require_capability`, `require_requester_session`, and `require_successor_identity`. `proof` rotates, requires, and clears the successor proof.

### config.rs — configuration and the error taxonomy

- `Config::from_env` and `Config::from_env_file` read the database, embedding, GIGA source, and timezone settings.
- `Dotenv` and `dotenv_target` choose the dotenv file: the explicit override, then a file beside the executable, then the state directory. The product tree is never used.
- `Config::pool` opens the pool and checks that `memory_chunks.body_embedding` is `vector(2048)`.
- `AppError` is the crate error taxonomy: `Invalid`, `Refusal`, `Config`, `Database`, `DatabaseConnect`, `DatabaseSchema`, `Embedding`, `Protocol`, and `Io`.
- `AppError::diagnostics` builds the operator diagnostics for each variant: owner, expected, observed, evidence, targets, next checks, and write outcome.
- `validation_owner`, `database_owner`, `embedding_owner`, `validation_target`, and `config_reason` name who owns the failure and where to look.
- `code`, `retryable`, and `safe_message` shape the caller-facing body. A safe message never leaks state.
- `is_write_operation` tells the diagnostics whether the failed call could have written.
- The module owns the shared regular expressions and the shared HTTP client.

### canon.rs — typed canon authority

- `canon_write` writes one canonical entity. A write never overwrites: it supersedes the named predecessors.
- `lock_predecessors` locks and checks every predecessor before the write, so a rename keeps its lineage.
- `canon_read` reads by id or by name. `include_history` returns the superseded rows too.
- `entity_result` returns the full authority fields: authority, supersedes, superseded by, attribution, and pointer files.
- `canon_database_error` maps a constraint failure to a caller-facing refusal.

### migrations.rs — the schema lineage

- `MIGRATIONS` is the embedded registry of every schema version this binary knows.
- `run_migrations` applies the missing versions in order and records them in `schema_migrations`.
- `transactional_sql` requires an explicit `BEGIN;` and `COMMIT;` inside each migration and strips them. It refuses a migration with ambiguous transaction authority.
- `migration_state` and `state_from` report the applied, missing, and unknown versions.
- `consolidated_version_labels` derives the backup allowlist from the registry, so the two cannot drift.
- `migration_pool` opens a pool for schema work with its own timeout.

### backup.rs — dumps, rotation, and restore

- `backup_with_migrations` runs `pg_dump` in custom format and writes a manifest with the size, the hash, and the migration lineage.
- `rotate` keeps the newest dumps and deletes the rest.
- `restore_checked` refuses unless the caller repeats the database name and the lineage is known.
- `restore` filters the archive table of contents so the `vector` and `pg_trgm` extensions survive the restore.
- `filter_extension_toc`, `is_preserved_extension_entry`, and `dump_toc` do that filtering. Reading the contents also proves the dump is not truncated.
- `TempList` and `write_temp_list` hold the scratch restore list under an unpredictable name and always remove it.
- `backup_health` and `backup_health_in` report the newest dump and its age against a limit.
- `run_post_write` runs the dump the write paths request and returns `hearth::BackupReceipt` or `hearth::BackupFailure`. Every outcome lands one Insula point under `backup.post_write` with the elapsed time and, on failure, the code as its error class. `post_write_outcome` folds the request's `backup` flag in.
- `default_backup_dir` puts dumps under the state directory. There is no guessed directory.
- `resolve_pg_tool` finds `pg_dump` and `pg_restore` by probing `--version` on each candidate in order: `PG_BIN_DIR`, then WSL (`wsl.exe` on `PATH`, then `%SystemRoot%\System32\wsl.exe`; Windows with `ATHANOR_PG_WSL=1` only), then `PATH`. The chosen route is the receipt's `tool` and the manifest's `pg_dump_tool`. When none answers the failure code is `pg_dump_not_found` and the detail lists every candidate with its reason.

### health.rs — the substrate verdict

- `substrate_health_with_config` returns one verdict with the mode, the parts, and the reasons for a degraded state.
- `topology` reports the state root and how it was chosen.
- `database_health` checks the connection, the required tables, the required extensions, the embedding column, and the migration state.
- `embedding_health` checks the embedding endpoint and the vector dimension. The caller can skip it.
- The verdict folds in the backup age from `backup_health`.
- `REQUIRED_TABLES` and `REQUIRED_EXTENSIONS` are the named database contract.

### timeline.rs — the Pulse panel reads

- `memory_timeline` returns memories newest first with a keyset cursor. It follows the recall row discipline: no archived, no superseded, and no paper boats.
- `memory_read` returns one memory by id, including a historical row, with its authority fields visible.
- `lesson_timeline` returns the lesson registry newest first, ordered by update time.
- These are pure reads with no identity parameters. The panel proves its reach at the Host.
- `validate_limit` bounds every page.

### anamnesis.rs — the counsel cabinet

- `anamnesis` reads counsel in two modes. Wake returns the pillars and the active cycles; consult ranks the cabinet by text.
- Each entry carries its last three repetitions from `anamnesis_reps`.
- A cycle with a blank verify note is excluded at wake, and the result says so.
- `anamnesis_write` creates a cabinet entry, updates it, or appends one lived repetition.
- A writer refusal is final and returns as a receipt, not as an exception.
- `anamnesis_embedding` embeds the entry text when embedding is available, and warns when it is not.

### hallway.rs — the origami adapter

- The module is an 87-line adapter. It holds no hallway logic.
- Nine doors forward to `origami::hallways`: create, join, post, read, inbox, knock policy, knock, knock claim, and knock settle.
- `hallway_post` passes the House timezone from the `Config`, because the daily thread depends on it.
- `app_error` maps every `HallwayError` onto the crate `AppError`.

### paper_boat.rs — the boat door

- `paper_boat_sleep` plans the boat with `origami::boats`, writes it as a memory, and writes the ready pointer in the same transaction.
- The write reuses `prepare_memory_write` and `write_memory_tx`, so a boat is an ordinary memory of the boat kind.
- The receipt reports whether the row was new, so a repeated sleep is safe.
- A requested backup runs after the commit, and a failure becomes a warning, not an error.
- A requested backup does not run when the newest dump is younger than 12 hours. The receipt reports `status: fresh`, the covering dump path, and the dump age. One full `pg_dump` pins PostgreSQL for about 25 seconds, and every room's sleep asks for one.
- `paper_boat_wake` returns the latest boat for the room.

### cluster.rs — clusters over the chunk vectors

- `spherical_kmeans` is deterministic. It uses farthest-point initialization and no random source, so equal input gives equal output.
- `cluster_maintenance` checks, dry-runs, or rebuilds. A rebuild takes an advisory lock, reads the live chunk vectors, and rewrites the cluster tables.
- `cluster_staleness` and `cluster_is_stale` report whether the clusters still describe the corpus.
- `cluster_resonance` scores the query vector against the cluster centroids for recall.
- `cluster_summaries` lists the clusters with their member counts.

### bm25f.rs — the field-aware lexical scorer

- `score` computes one BM25F score over the weighted fields.
- Six field weights exist: title, heading, source path, threads, body, and memory type.
- `tokens` keeps document tokenization unchanged. Query preparation removes casual filler and recognized keyboard laughter.
- A negated reminiscence removes `remember` as filler. A query about the `remember` tool keeps it.
- Query coverage counts retained content terms. Canon lookup preserves the original name terms.
- The scorer returns the matched terms with the score, so recall can show its evidence.

### entity.rs — alias resolution

- `entity_resolve` matches a query against the active named entities of the room and the House. It bounds the limit at 32.
- `resolve_matches` prefers the longest matched label, then the earliest position in the query. It keeps one match per name and kind.
- A label under three characters never matches. `normalize` folds case and punctuation so an alias matches as written.

### state.rs — where mutable state lives

- `state_root` resolves the Athanor state root. `ATHANOR_STATE_DIR` wins and must be absolute.
- The compile-time checkout is accepted only when the running binary sits inside that checkout's `target` directory.
- Any other case is an error. The module never guesses a path.
- `resolve_state_root` also returns the reason, so diagnostics can show why a path was chosen.
- `substrate_state_dir` is the crate's own directory under the state root.
- `StateRootError` variants are actionable and expose no build-machine path.
