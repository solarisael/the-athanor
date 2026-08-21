//! Adversarial proofs for the lesson trigger boundary (migration 0019 +
//! `lesson_trigger_match`), run against a real isolated PostgreSQL.
//!
//! Every test names the mutation it kills and carries a `red-proof:` line: the
//! exact edit to production code that must make it fail. A trigger test that
//! only confirms the happy path is worse than nothing here — the matcher is
//! live code the moment it ships, so each case pins the OPPOSITE of its own
//! fixture condition too (no-fire beside fire, foreign scope beside house
//! scope, remind beside block, unknown extension beside known).

use athanor_substrate::{
    LessonMutationReceipt, LessonTriggerMatchParams, LessonTriggerMatchResult,
    LessonTriggerSurface, LessonUpdateParams, lesson_trigger_match, lesson_update,
};
use serde_json::{Value, json};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgPool, Row};
use std::str::FromStr;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn isolated_database_url() -> String {
    let url = std::env::var("SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL")
        .expect("dedicated test database URL must be configured when this proof is run");
    let lower = url.to_ascii_lowercase();
    assert!(
        !lower.contains("solarisael_memory"),
        "refusing the live/default database"
    );
    assert!(
        !lower.contains("solarisael-house"),
        "refusing a production-looking database"
    );
    url
}

/// Session-temporary mirrors of the two tables migration 0019 touches. The
/// lessons mirror keeps the 0008 column shape the query path reads plus the
/// five trigger columns; the ledger mirror keeps the composite FK and the
/// CHECK constraints, because two of the proofs below are exactly about those.
async fn temp_trigger_pool() -> TestResult<PgPool> {
    let options = PgConnectOptions::from_str(&isolated_database_url())?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .after_connect(|connection, _metadata| {
            Box::pin(async move {
                sqlx::query("SELECT set_config('search_path', 'pg_temp', false)")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with(options)
        .await?;
    sqlx::query(
        "CREATE TEMP TABLE lessons (
            lesson_key TEXT NOT NULL,
            kind_path TEXT GENERATED ALWAYS AS (
                lesson_key || '/' || COALESCE(
                    NULLIF(BTRIM(REGEXP_REPLACE(LOWER(COALESCE(shape, '')), '[^a-z0-9]+', '-', 'g'), '-'), ''),
                    'general'
                )
            ) STORED,
            id BIGINT NOT NULL,
            scope TEXT NOT NULL DEFAULT 'house',
            project TEXT,
            voice TEXT,
            register TEXT[] NOT NULL DEFAULT '{}',
            shape TEXT,
            stage TEXT[] NOT NULL DEFAULT '{}',
            title TEXT NOT NULL,
            lesson TEXT NOT NULL,
            trigger_context TEXT,
            proof_pattern TEXT,
            example_text TEXT,
            example_cmd TEXT,
            writers TEXT[] NOT NULL DEFAULT '{}',
            tools TEXT[] NOT NULL DEFAULT '{}',
            negation_of BIGINT,
            tags TEXT[] NOT NULL DEFAULT '{}',
            source_memory_path TEXT,
            source_lines_start INTEGER,
            source_lines_end INTEGER,
            always_on BOOLEAN NOT NULL DEFAULT FALSE,
            language_keys TEXT[] NOT NULL DEFAULT '{}',
            technology_keys TEXT[] NOT NULL DEFAULT '{}',
            thread_keys TEXT[] NOT NULL DEFAULT '{}',
            meta JSONB NOT NULL DEFAULT '{}',
            condition TEXT[] NOT NULL DEFAULT '{}',
            ast_condition TEXT[] NOT NULL DEFAULT '{}',
            trigger_scope TEXT[] NOT NULL DEFAULT '{}',
            interrupt_mode TEXT,
            repeat_cooldown_secs INTEGER,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            PRIMARY KEY (lesson_key, id),
            CONSTRAINT lessons_interrupt_mode_check
                CHECK (interrupt_mode IS NULL OR interrupt_mode IN ('block','remind'))
        )",
    )
    .execute(&pool)
    .await?;
    sqlx::query(
        "CREATE TEMP TABLE lesson_trigger_events (
            id BIGSERIAL PRIMARY KEY,
            lesson_key TEXT NOT NULL,
            lesson_id BIGINT NOT NULL,
            room TEXT NOT NULL,
            session_id TEXT NOT NULL,
            surface TEXT NOT NULL CHECK (surface IN ('tool','prose')),
            tool_name TEXT,
            path TEXT,
            pattern_kind TEXT NOT NULL CHECK (pattern_kind IN ('regex','ast')),
            matched_pattern TEXT NOT NULL,
            urgency TEXT NOT NULL CHECK (urgency IN ('block','remind')),
            fired_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            FOREIGN KEY (lesson_key, lesson_id)
                REFERENCES lessons(lesson_key, id) ON DELETE CASCADE
        )",
    )
    .execute(&pool)
    .await?;
    Ok(pool)
}

struct Trigger<'a> {
    id: i64,
    scope: &'a str,
    title: &'a str,
    condition: Vec<&'a str>,
    ast_condition: Vec<&'a str>,
    trigger_scope: Vec<&'a str>,
    interrupt_mode: Option<&'a str>,
    cooldown: Option<i32>,
    language_keys: Vec<&'a str>,
}

impl<'a> Trigger<'a> {
    fn regex(id: i64, title: &'a str, pattern: &'a str) -> Self {
        Trigger {
            id,
            scope: "house",
            title,
            condition: vec![pattern],
            ast_condition: vec![],
            trigger_scope: vec![],
            interrupt_mode: None,
            cooldown: None,
            language_keys: vec![],
        }
    }
}

async fn insert_trigger(pool: &PgPool, trigger: &Trigger<'_>) -> TestResult {
    sqlx::query(
        "INSERT INTO lessons
         (lesson_key,id,scope,title,lesson,proof_pattern,condition,ast_condition,
          trigger_scope,interrupt_mode,repeat_cooldown_secs,language_keys)
         VALUES ('coding',$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
    )
    .bind(trigger.id)
    .bind(trigger.scope)
    .bind(trigger.title)
    .bind(format!("body of {}", trigger.title))
    .bind(format!("proof of {}", trigger.title))
    .bind(
        trigger
            .condition
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>(),
    )
    .bind(
        trigger
            .ast_condition
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>(),
    )
    .bind(
        trigger
            .trigger_scope
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>(),
    )
    .bind(trigger.interrupt_mode)
    .bind(trigger.cooldown)
    .bind(
        trigger
            .language_keys
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>(),
    )
    .execute(pool)
    .await?;
    Ok(())
}

fn tool_surface(tool: &str, path: &str, text: &str) -> LessonTriggerSurface {
    LessonTriggerSurface {
        kind: "tool".into(),
        tool: Some(tool.into()),
        path: Some(path.into()),
        text: text.into(),
    }
}

fn prose_surface(text: &str) -> LessonTriggerSurface {
    LessonTriggerSurface {
        kind: "prose".into(),
        tool: None,
        path: None,
        text: text.into(),
    }
}

async fn match_surfaces(
    pool: &PgPool,
    room: &str,
    session: &str,
    surfaces: Vec<LessonTriggerSurface>,
) -> TestResult<LessonTriggerMatchResult> {
    Ok(lesson_trigger_match(
        pool,
        LessonTriggerMatchParams {
            room: room.into(),
            session: session.into(),
            surfaces,
        },
    )
    .await?)
}

fn fired_ids(result: &LessonTriggerMatchResult) -> Vec<i64> {
    let mut ids: Vec<i64> = result.fired.iter().map(|entry| entry.id).collect();
    ids.sort_unstable();
    ids
}

async fn ledger_count(pool: &PgPool) -> TestResult<i64> {
    Ok(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM lesson_trigger_events")
            .fetch_one(pool)
            .await?,
    )
}

async fn backdate_fire(
    pool: &PgPool,
    lesson_id: i64,
    room: &str,
    session: &str,
    seconds_ago: i32,
) -> TestResult {
    sqlx::query(
        "INSERT INTO lesson_trigger_events
         (lesson_key,lesson_id,room,session_id,surface,tool_name,path,pattern_kind,
          matched_pattern,urgency,fired_at)
         VALUES ('coding',$1,$2,$3,'tool','edit','src/a.rs','regex','seeded','block',
                 NOW() - make_interval(secs => $4))",
    )
    .bind(lesson_id)
    .bind(room)
    .bind(session)
    .bind(f64::from(seconds_ago))
    .execute(pool)
    .await?;
    Ok(())
}

/// `None` when the write was accepted, `Some(message)` when it was refused.
/// A refusal here must be a hard error, not an Ok receipt carrying a warning.
async fn patch_lesson(
    pool: &PgPool,
    id: i64,
    title: &str,
    patch: Value,
) -> TestResult<Option<String>> {
    match lesson_update(
        pool,
        LessonUpdateParams {
            kind: "coding-lesson".into(),
            id,
            expected_title: title.into(),
            patch,
        },
    )
    .await
    {
        Ok(LessonMutationReceipt::Updated { .. }) => Ok(None),
        Ok(LessonMutationReceipt::Refused { error, .. }) => Ok(Some(error)),
        Ok(LessonMutationReceipt::Deleted { .. }) => {
            Err(std::io::Error::other("lesson update returned a delete receipt").into())
        }
        Err(error) => Ok(Some(error.to_string())),
    }
}

// Kills: the ledger is never consulted (or is consulted with an inverted
// comparison), so a NULL-cooldown lesson screams on every single tool call of
// the session. Pins both sides: no refire inside the session, refire in a
// different session of the same room.
// red-proof: delete the "latest fired_at" ledger lookup from the repeat-policy
// filter in lesson_trigger_match (treat every eligible lesson as fireable).
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; the trigger tables are session-temporary"]
async fn null_cooldown_fires_once_per_session_and_again_in_a_fresh_session() -> TestResult {
    let pool = temp_trigger_pool().await?;
    insert_trigger(
        &pool,
        &Trigger::regex(901, "No unwrap in the hot path", "unwrap\\(\\)"),
    )
    .await?;

    let first = match_surfaces(
        &pool,
        "kodo",
        "session-a",
        vec![tool_surface(
            "edit",
            "src/hot.rs",
            "let value = maybe.unwrap();",
        )],
    )
    .await?;
    assert_eq!(fired_ids(&first), vec![901], "first tool surface must fire");
    assert_eq!(ledger_count(&pool).await?, 1);

    let repeat = match_surfaces(
        &pool,
        "kodo",
        "session-a",
        vec![tool_surface(
            "edit",
            "src/hot.rs",
            "let other = maybe.unwrap();",
        )],
    )
    .await?;
    assert!(
        repeat.fired.is_empty(),
        "NULL cooldown means once per session, got {:?}",
        fired_ids(&repeat)
    );
    assert_eq!(
        ledger_count(&pool).await?,
        1,
        "a suppressed match must not write telemetry"
    );

    let other_session = match_surfaces(
        &pool,
        "kodo",
        "session-b",
        vec![tool_surface(
            "edit",
            "src/hot.rs",
            "let third = maybe.unwrap();",
        )],
    )
    .await?;
    assert_eq!(
        fired_ids(&other_session),
        vec![901],
        "suppression is scoped to (room, session), not global"
    );
    assert_eq!(ledger_count(&pool).await?, 2);
    Ok(())
}

// Kills: the cooldown column is read but the elapsed comparison is dropped or
// reversed, so either a cooled-down lesson stays silent forever or a lesson
// still inside its cooldown refires. Both directions are pinned with backdated
// ledger rows so the clock, not the test, decides.
// red-proof: flip the cooldown comparison in the repeat-policy filter
// (`fired_at < NOW() - cooldown` -> `fired_at > NOW() - cooldown`).
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; the trigger tables are session-temporary"]
async fn elapsed_cooldown_refires_while_a_live_cooldown_stays_silent() -> TestResult {
    let pool = temp_trigger_pool().await?;
    let mut cooled = Trigger::regex(911, "Cooled lesson", "todo!\\(\\)");
    cooled.cooldown = Some(60);
    let mut warm = Trigger::regex(912, "Warm lesson", "todo!\\(\\)");
    warm.cooldown = Some(3_600);
    insert_trigger(&pool, &cooled).await?;
    insert_trigger(&pool, &warm).await?;
    backdate_fire(&pool, 911, "kodo", "session-c", 600).await?;
    backdate_fire(&pool, 912, "kodo", "session-c", 600).await?;

    let result = match_surfaces(
        &pool,
        "kodo",
        "session-c",
        vec![tool_surface(
            "write",
            "src/lib.rs",
            "fn pending() { todo!() }",
        )],
    )
    .await?;
    assert_eq!(
        fired_ids(&result),
        vec![911],
        "only the lesson whose 60s cooldown elapsed 600s ago may refire"
    );
    assert_eq!(
        ledger_count(&pool).await?,
        3,
        "the refire adds exactly one ledger row to the two seeded ones"
    );
    Ok(())
}

// Kills: a fire that never lands in the ledger (silent telemetry loss), a fire
// that lands with the wrong room/session/surface/urgency columns, and orphan
// ledger rows surviving their lesson.
// red-proof: remove the `INSERT INTO lesson_trigger_events` from the fire path,
// or drop `ON DELETE CASCADE` from the composite FK in migration 0019.
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; the trigger tables are session-temporary"]
async fn every_fire_writes_its_ledger_row_and_dies_with_its_lesson() -> TestResult {
    let pool = temp_trigger_pool().await?;
    let mut lesson = Trigger::regex(921, "Ledger lesson", "eval\\(");
    lesson.interrupt_mode = Some("remind");
    insert_trigger(&pool, &lesson).await?;

    let result = match_surfaces(
        &pool,
        "kodo",
        "session-d",
        vec![tool_surface("write", "scripts/run.py", "eval(payload)")],
    )
    .await?;
    assert_eq!(fired_ids(&result), vec![921]);

    let row = sqlx::query(
        "SELECT lesson_key,lesson_id,room,session_id,surface,tool_name,path,pattern_kind,
                matched_pattern,urgency FROM lesson_trigger_events",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(row.try_get::<String, _>("lesson_key")?, "coding");
    assert_eq!(row.try_get::<i64, _>("lesson_id")?, 921);
    assert_eq!(row.try_get::<String, _>("room")?, "kodo");
    assert_eq!(row.try_get::<String, _>("session_id")?, "session-d");
    assert_eq!(row.try_get::<String, _>("surface")?, "tool");
    assert_eq!(
        row.try_get::<Option<String>, _>("tool_name")?.as_deref(),
        Some("write")
    );
    assert_eq!(
        row.try_get::<Option<String>, _>("path")?.as_deref(),
        Some("scripts/run.py")
    );
    assert_eq!(row.try_get::<String, _>("pattern_kind")?, "regex");
    assert_eq!(row.try_get::<String, _>("matched_pattern")?, "eval\\(");
    assert_eq!(
        row.try_get::<String, _>("urgency")?,
        "remind",
        "the ledger records the demoted urgency, not the default"
    );

    sqlx::query("DELETE FROM lessons WHERE lesson_key='coding' AND id=921")
        .execute(&pool)
        .await?;
    assert_eq!(
        ledger_count(&pool).await?,
        0,
        "deleting the lesson must cascade its ledger rows"
    );
    Ok(())
}

// Kills: the scope clause dropped from the new query path, which would leak
// another room's private lessons into this room's interrupts. Pins all three
// buckets in one call: foreign room (never), own room (yes), house (yes).
// red-proof: delete the `AND scope = ANY($scopes)` predicate from the
// trigger-bearing lesson query.
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; the trigger tables are session-temporary"]
async fn foreign_room_scope_never_fires_while_house_and_own_room_do() -> TestResult {
    let pool = temp_trigger_pool().await?;
    let mut foreign = Trigger::regex(931, "Kintsu-only lesson", "sample_rate");
    foreign.scope = "kintsu";
    let mut own = Trigger::regex(932, "Kodo-only lesson", "sample_rate");
    own.scope = "kodo";
    let house = Trigger::regex(933, "House lesson", "sample_rate");
    insert_trigger(&pool, &foreign).await?;
    insert_trigger(&pool, &own).await?;
    insert_trigger(&pool, &house).await?;

    let result = match_surfaces(
        &pool,
        "kodo",
        "session-e",
        vec![tool_surface(
            "edit",
            "src/audio.rs",
            "let sample_rate = 48_000;",
        )],
    )
    .await?;
    assert_eq!(
        fired_ids(&result),
        vec![932, 933],
        "kintsu's private lesson must never reach kodo"
    );

    let lesson_ids: Vec<i64> =
        sqlx::query_scalar("SELECT lesson_id FROM lesson_trigger_events ORDER BY lesson_id")
            .fetch_all(&pool)
            .await?;
    assert_eq!(lesson_ids, vec![932, 933]);
    Ok(())
}

// Kills: interrupt_mode collapsed to a single urgency — either everything
// becomes a remind (no lesson can ever block) or the explicit demotion is
// ignored and reminders escalate to blocks. NULL means block by default.
// red-proof: hardcode `urgency = "remind"` where interrupt_mode is mapped.
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; the trigger tables are session-temporary"]
async fn null_interrupt_mode_blocks_and_explicit_remind_demotes() -> TestResult {
    let pool = temp_trigger_pool().await?;
    let defaulted = Trigger::regex(941, "Defaulted lesson", "panic!\\(");
    let mut demoted = Trigger::regex(942, "Demoted lesson", "panic!\\(");
    demoted.interrupt_mode = Some("remind");
    insert_trigger(&pool, &defaulted).await?;
    insert_trigger(&pool, &demoted).await?;

    let result = match_surfaces(
        &pool,
        "kodo",
        "session-f",
        vec![tool_surface("edit", "src/lib.rs", "panic!(\"nope\")")],
    )
    .await?;
    let mut urgencies: Vec<(i64, String)> = result
        .fired
        .iter()
        .map(|entry| (entry.id, entry.urgency.clone()))
        .collect();
    urgencies.sort();
    assert_eq!(
        urgencies,
        vec![(941, "block".to_string()), (942, "remind".to_string())]
    );
    Ok(())
}

// Kills: trigger_scope tokens parsed but never enforced, so a 'tool:write'
// lesson fires on every edit and a 'text' lesson fires on tool payloads. Also
// pins the empty-scope default (regex matches prose AND tool).
// red-proof: make the trigger_scope check always return true.
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; the trigger tables are session-temporary"]
async fn trigger_scope_tokens_bind_their_surfaces() -> TestResult {
    let pool = temp_trigger_pool().await?;
    let mut write_only = Trigger::regex(951, "Write-only lesson", "secret");
    write_only.trigger_scope = vec!["tool:write"];
    let mut prose_only = Trigger::regex(952, "Prose-only lesson", "secret");
    prose_only.trigger_scope = vec!["text"];
    let unscoped = Trigger::regex(953, "Unscoped lesson", "secret");
    insert_trigger(&pool, &write_only).await?;
    insert_trigger(&pool, &prose_only).await?;
    insert_trigger(&pool, &unscoped).await?;

    let on_edit = match_surfaces(
        &pool,
        "kodo",
        "session-g",
        vec![tool_surface("edit", "src/lib.rs", "let secret = 1;")],
    )
    .await?;
    assert_eq!(
        fired_ids(&on_edit),
        vec![953],
        "tool:write and text lessons must stay silent on an edit surface"
    );

    let on_prose = match_surfaces(
        &pool,
        "kodo",
        "session-h",
        vec![prose_surface("I will hardcode the secret for now")],
    )
    .await?;
    assert_eq!(
        fired_ids(&on_prose),
        vec![952, 953],
        "prose fires the text-scoped and the unscoped lesson only"
    );
    let surfaces: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT surface FROM lesson_trigger_events ORDER BY surface")
            .fetch_all(&pool)
            .await?;
    assert_eq!(surfaces, vec!["prose".to_string(), "tool".to_string()]);
    Ok(())
}

// Kills: an unsupported file extension turned into a hard error (one .txt edit
// takes the whole tap down) or silently swallowed with no warning at all. The
// contract is: skip the ast pattern, append a warning, keep matching regex.
// red-proof: change the unknown-extension arm from `warnings.push(..)` to
// `return Err(AppError::Invalid(..))`.
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; the trigger tables are session-temporary"]
async fn unknown_extension_skips_ast_patterns_with_a_warning_not_an_error() -> TestResult {
    let pool = temp_trigger_pool().await?;
    let mut ast_only = Trigger::regex(961, "Ast lesson", "");
    ast_only.condition = vec![];
    ast_only.ast_condition = vec!["console.log($$$ARGS)"];
    let regex_only = Trigger::regex(962, "Regex lesson", "console\\.log");
    insert_trigger(&pool, &ast_only).await?;
    insert_trigger(&pool, &regex_only).await?;

    let result = match_surfaces(
        &pool,
        "kodo",
        "session-i",
        vec![tool_surface("write", "notes/scratch.txt", "console.log(1)")],
    )
    .await?;
    assert!(result.ok);
    assert_eq!(
        fired_ids(&result),
        vec![962],
        "the ast lesson is skipped, the regex lesson still fires"
    );
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.contains("scratch.txt") || warning.contains("extension")),
        "the skip must be reported, got {:?}",
        result.warnings
    );
    assert_eq!(
        ledger_count(&pool).await?,
        1,
        "a skipped ast pattern must not write a ledger row"
    );
    Ok(())
}

// Kills: `#[serde(skip_serializing_if = "Vec::is_empty")]` on warnings — the
// exact live incident (house-protocol lib.rs:661) where only HEALTHY responses
// crashed the adapter because it read warnings[0] of a missing array. A
// zero-warning response must still carry `"warnings": []` on the wire.
// red-proof: add `#[serde(skip_serializing_if = "Vec::is_empty")]` to
// LessonTriggerMatchResult::warnings.
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; the trigger tables are session-temporary"]
async fn healthy_zero_warning_response_still_serializes_an_empty_warnings_array() -> TestResult {
    let pool = temp_trigger_pool().await?;
    insert_trigger(&pool, &Trigger::regex(971, "Wire lesson", "dbg!\\(")).await?;

    let quiet = match_surfaces(
        &pool,
        "kodo",
        "session-j",
        vec![tool_surface("edit", "src/lib.rs", "let value = 1;")],
    )
    .await?;
    assert!(quiet.fired.is_empty());
    let wire = serde_json::to_value(&quiet)?;
    assert_eq!(
        wire.get("warnings"),
        Some(&json!([])),
        "healthy responses must serialize warnings, got {wire}"
    );
    assert_eq!(wire.get("fired"), Some(&json!([])));
    assert_eq!(wire.get("ok"), Some(&json!(true)));

    let loud = match_surfaces(
        &pool,
        "kodo",
        "session-j",
        vec![tool_surface("edit", "src/lib.rs", "dbg!(value);")],
    )
    .await?;
    let fired_wire = serde_json::to_value(&loud)?;
    let entry = fired_wire
        .get("fired")
        .and_then(Value::as_array)
        .and_then(|entries| entries.first())
        .expect("a fire must be present on the wire");
    for key in [
        "family",
        "id",
        "title",
        "lesson",
        "proofPattern",
        "urgency",
        "surface",
        "path",
        "patternKind",
        "pattern",
    ] {
        assert!(
            entry.get(key).is_some(),
            "fired entry is missing {key}: {entry}"
        );
    }
    assert_eq!(fired_wire.get("warnings"), Some(&json!([])));
    Ok(())
}

// Kills: semantic trigger validation demoted to a warning, skipped entirely, or
// run AFTER the row is written — any of which lets an uncompilable regex or an
// unparseable ast pattern sit in the table and poison every later match.
// red-proof: replace the `return Err(AppError::Invalid(..))` in the trigger
// validation pre-pass with `warnings.push(..)` (or delete the pre-pass call).
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; the trigger tables are session-temporary"]
async fn invalid_trigger_specs_are_refused_by_the_write_path() -> TestResult {
    let pool = temp_trigger_pool().await?;
    insert_trigger(&pool, &Trigger::regex(981, "Validated lesson", "ok")).await?;

    for (patch, expectation) in [
        (json!({ "condition": ["("] }), "condition"),
        (json!({ "astCondition": ["fn ("] }), "astCondition"),
        (json!({ "triggerScope": ["voice"] }), "triggerScope"),
        (json!({ "interruptMode": "scream" }), "interruptMode"),
    ] {
        let refusal = patch_lesson(&pool, 981, "Validated lesson", patch.clone()).await?;
        let error = refusal.unwrap_or_else(|| panic!("{patch} must be refused, not accepted"));
        assert!(
            error
                .to_ascii_lowercase()
                .contains(&expectation.to_ascii_lowercase()),
            "refusal for {patch} must name the offending field, got {error}"
        );
    }

    let row = sqlx::query(
        "SELECT condition, ast_condition, trigger_scope, interrupt_mode
         FROM lessons WHERE lesson_key='coding' AND id=981",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        row.try_get::<Vec<String>, _>("condition")?,
        vec!["ok".to_string()]
    );
    assert!(row.try_get::<Vec<String>, _>("ast_condition")?.is_empty());
    assert!(row.try_get::<Vec<String>, _>("trigger_scope")?.is_empty());
    assert!(
        row.try_get::<Option<String>, _>("interrupt_mode")?
            .is_none(),
        "a refused write must leave the row untouched"
    );

    let accepted = patch_lesson(
        &pool,
        981,
        "Validated lesson",
        json!({ "condition": ["unwrap\\(\\)"], "interruptMode": "remind" }),
    )
    .await?;
    assert!(
        accepted.is_none(),
        "a valid trigger patch must still be accepted, got {accepted:?}"
    );
    Ok(())
}

// Kills: regex hit location metadata dropped at the substrate wire boundary, or
// the matched surface and byte offset both hard-coded to zero.
// red-proof: remove either location field from `LessonTriggerFired`, or replace
// either value in its constructor with zero.
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; the trigger tables are session-temporary"]
async fn regex_fire_serializes_surface_index_and_match_start() -> TestResult {
    let pool = temp_trigger_pool().await?;
    insert_trigger(
        &pool,
        &Trigger::regex(991, "Offset wire lesson", "unwrap\\(\\)"),
    )
    .await?;

    let result = match_surfaces(
        &pool,
        "wire-offsets",
        "session-offsets",
        vec![
            tool_surface("edit", "src/quiet.rs", "nothing here"),
            tool_surface("edit", "src/hit.rs", "prefix unwrap()"),
        ],
    )
    .await?;
    let wire = serde_json::to_value(&result)?;
    let entry = wire
        .get("fired")
        .and_then(Value::as_array)
        .and_then(|entries| entries.first())
        .expect("a regex fire must be present on the wire");

    assert_eq!(entry.get("surfaceIndex"), Some(&json!(1)));
    assert_eq!(entry.get("matchStart"), Some(&json!(7)));
    Ok(())
}

// Kills: the language fence lost between PostgreSQL and house-core — the
// language_keys column dropped from the trigger SELECT (every keyed lesson
// becomes universal), or the extension check inverted. Pins both sides in one
// call shape: the keyed lesson fires on its own language and stays silent on a
// foreign one, while the unkeyed lesson beside it fires on both.
// red-proof: delete `language_keys` from TRIGGER_SELECT's column list (or from
// the spec built in `trigger_row`).
#[tokio::test]
#[ignore = "requires SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL; the trigger tables are session-temporary"]
async fn a_rust_keyed_lesson_fires_on_rust_and_stays_silent_on_python() -> TestResult {
    let pool = temp_trigger_pool().await?;
    let mut keyed = Trigger::regex(971, "Rust-keyed lesson", "unwrap\\(\\)");
    keyed.language_keys = vec!["rust"];
    let unkeyed = Trigger::regex(972, "Unkeyed lesson", "unwrap\\(\\)");
    insert_trigger(&pool, &keyed).await?;
    insert_trigger(&pool, &unkeyed).await?;

    let on_rust = match_surfaces(
        &pool,
        "kodo",
        "session-fence-rs",
        vec![tool_surface("edit", "src/lib.rs", "let x = y.unwrap();")],
    )
    .await?;
    assert_eq!(fired_ids(&on_rust), vec![971, 972]);

    let on_python = match_surfaces(
        &pool,
        "kodo",
        "session-fence-py",
        vec![tool_surface("edit", "app.py", "x = y.unwrap()")],
    )
    .await?;
    assert_eq!(
        fired_ids(&on_python),
        vec![972],
        "a rust-keyed lesson must not fire on a python surface"
    );
    Ok(())
}
