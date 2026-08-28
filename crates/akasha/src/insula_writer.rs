use crate::insula::{
    INSULA_MAX_BATCH_EVENTS, IdempotencyScope, IngestBatch, ObservationEvent, ObservationPhase,
    OutcomeClass, TrustedBinding, ingest_batch,
};
use chrono::Utc;
use sqlx::PgPool;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use uuid::Uuid;

const QUEUE_CAPACITY: usize = 512; // enough: 512 pending observations; widen only with an explicit memory budget.
const MAX_DROP_COUNT: i64 = 1_000_000_000;
const MAX_DURATION_US: i64 = 86_400_000_000;
const SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(750); // enough: 750ms shutdown grace; move persistence out of process for stronger delivery.

static EMITTER: OnceLock<Emitter> = OnceLock::new();

struct QueuedObservation {
    binding: TrustedBinding,
    event: ObservationEvent,
}

struct WriterState {
    writer_id: Uuid,
    sequence: AtomicI64,
    dropped: AtomicI64,
    accepting: AtomicBool,
    sender: mpsc::Sender<QueuedObservation>,
}

impl WriterState {
    fn next_sequence(&self) -> i64 {
        self.sequence.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn note_drop(&self) {
        let _ = self
            .dropped
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
                Some(count.saturating_add(1).min(MAX_DROP_COUNT))
            });
    }

    fn enqueue(&self, observation: QueuedObservation) {
        if !self.accepting.load(Ordering::Acquire) {
            return;
        }
        if self.sender.try_send(observation).is_err() {
            self.note_drop();
        }
    }
}

struct Emitter {
    state: Arc<WriterState>,
    shutdown: watch::Sender<bool>,
    drain: Mutex<Option<JoinHandle<()>>>,
}

pub struct EmitterSpan {
    state: Arc<WriterState>,
    binding: TrustedBinding,
    component: &'static str,
    layer: &'static str,
    operation: &'static str,
    trace_id: Uuid,
    span_id: Uuid,
    started_at: Instant,
}

pub fn system_binding() -> TrustedBinding {
    TrustedBinding {
        house_id: "solarisael".into(),
        room: "house".into(),
        spirit: "Athanor".into(),
        session_id: "service:athanor".into(),
    }
}

pub fn init_insula_emitter(pool: PgPool) {
    if disabled_by_environment() || EMITTER.get().is_some() {
        return;
    }
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return;
    };
    EMITTER.get_or_init(|| {
        let (sender, receiver) = mpsc::channel(QUEUE_CAPACITY);
        let (shutdown, shutdown_receiver) = watch::channel(false);
        let state = Arc::new(WriterState {
            writer_id: Uuid::new_v4(),
            sequence: AtomicI64::new(0),
            dropped: AtomicI64::new(0),
            accepting: AtomicBool::new(true),
            sender,
        });
        let drain_state = Arc::clone(&state);
        let drain = runtime.spawn(drain_loop(pool, receiver, shutdown_receiver, drain_state));
        Emitter {
            state,
            shutdown,
            drain: Mutex::new(Some(drain)),
        }
    });
}

pub fn start_span(
    binding: &TrustedBinding,
    component: &'static str,
    layer: &'static str,
    operation: &'static str,
) -> Option<EmitterSpan> {
    if ![component, layer, operation]
        .into_iter()
        .all(mechanical_name)
    {
        return None;
    }
    let emitter = EMITTER.get()?;
    if !emitter.state.accepting.load(Ordering::Acquire) {
        return None;
    }
    let started_at = Instant::now();
    let trace_id = Uuid::new_v4();
    let span_id = Uuid::new_v4();
    let event = observation(
        &emitter.state,
        trace_id,
        span_id,
        component,
        layer,
        operation,
        ObservationPhase::Start,
        None,
        OutcomeClass::Unknown,
        None,
        None,
        0,
    );
    emitter.state.enqueue(QueuedObservation {
        binding: binding.clone(),
        event,
    });
    Some(EmitterSpan {
        state: Arc::clone(&emitter.state),
        binding: binding.clone(),
        component,
        layer,
        operation,
        trace_id,
        span_id,
        started_at,
    })
}

pub fn end_span(span: Option<EmitterSpan>, outcome: OutcomeClass, error_class: Option<&str>) {
    let Some(span) = span else {
        return;
    };
    let duration_us = span
        .started_at
        .elapsed()
        .as_micros()
        .min(MAX_DURATION_US as u128) as i64;
    let event = observation(
        &span.state,
        span.trace_id,
        span.span_id,
        span.component,
        span.layer,
        span.operation,
        ObservationPhase::End,
        Some(duration_us),
        outcome,
        error_class,
        None,
        0,
    );
    span.state.enqueue(QueuedObservation {
        binding: span.binding,
        event,
    });
}

pub fn record_point(
    binding: &TrustedBinding,
    component: &'static str,
    layer: &'static str,
    operation: &'static str,
    outcome: OutcomeClass,
    error_class: Option<&str>,
    receipt: Option<(&str, &str)>,
) {
    if ![component, layer, operation]
        .into_iter()
        .all(mechanical_name)
        || receipt.is_some_and(|(kind, id)| !mechanical_name(kind) || !opaque_identifier(id))
    {
        return;
    }
    let Some(emitter) = EMITTER.get() else {
        return;
    };
    if !emitter.state.accepting.load(Ordering::Acquire) {
        return;
    }
    let event = observation(
        &emitter.state,
        Uuid::new_v4(),
        Uuid::new_v4(),
        component,
        layer,
        operation,
        ObservationPhase::Point,
        None,
        outcome,
        error_class,
        receipt,
        0,
    );
    emitter.state.enqueue(QueuedObservation {
        binding: binding.clone(),
        event,
    });
}

pub async fn flush_insula_emitter() {
    let Some(emitter) = EMITTER.get() else {
        return;
    };
    emitter.state.accepting.store(false, Ordering::Release);
    let _ = emitter.shutdown.send(true);
    let Some(mut drain) = emitter.drain.lock().ok().and_then(|mut task| task.take()) else {
        return;
    };
    if tokio::time::timeout(SHUTDOWN_TIMEOUT, &mut drain)
        .await
        .is_err()
    {
        drain.abort();
        let _ = drain.await;
    }
}

fn disabled_by_environment() -> bool {
    ["ATHANOR_DISABLE_INSULA", "ATHANOR_REPLAY_MODE"]
        .into_iter()
        .any(|name| std::env::var(name).is_ok_and(|value| value == "1"))
}

fn mechanical_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || (index > 0 && matches!(byte, b'_' | b'.' | b':' | b'-'))
        })
}

fn opaque_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.is_ascii()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b':' | b'/' | b'-' | b'@')
        })
}

#[allow(clippy::too_many_arguments)]
fn observation(
    state: &WriterState,
    trace_id: Uuid,
    span_id: Uuid,
    component: &'static str,
    layer: &'static str,
    operation: &'static str,
    phase: ObservationPhase,
    duration_us: Option<i64>,
    outcome_class: OutcomeClass,
    error_class: Option<&str>,
    receipt: Option<(&str, &str)>,
    drop_count: i64,
) -> ObservationEvent {
    let (receipt_kind, receipt_id) = match receipt {
        Some((kind, id)) => (Some(kind.to_owned()), Some(id.to_owned())),
        None => (None, None),
    };
    ObservationEvent {
        event_id: Uuid::new_v4().to_string(),
        span_id: span_id.to_string(),
        trace_id: trace_id.to_string(),
        parent_span_id: None,
        writer_id: state.writer_id.to_string(),
        writer_sequence: state.next_sequence(),
        component: component.into(),
        layer: layer.into(),
        operation: operation.into(),
        phase,
        observed_at: Utc::now(),
        duration_us,
        outcome_class,
        error_class: error_class
            .filter(|value| mechanical_name(value))
            .map(str::to_owned),
        bytes_in: 0,
        bytes_out: 0,
        tokens_in: 0,
        tokens_out: 0,
        tool_call_id: None,
        provider_request_id: None,
        idempotency_version: 1,
        idempotency_scope: if receipt_kind.is_some() {
            IdempotencyScope::RoomOperation
        } else {
            IdempotencyScope::TraceSpan
        },
        idempotency_key: String::new(),
        receipt_kind,
        receipt_id,
        semantic_hash: String::new(),
        drop_count,
    }
}

fn drop_observation(state: &WriterState, drop_count: i64) -> QueuedObservation {
    QueuedObservation {
        binding: system_binding(),
        event: observation(
            state,
            Uuid::new_v4(),
            Uuid::new_v4(),
            "akasha",
            "substrate",
            "insula_writer",
            ObservationPhase::Drop,
            None,
            OutcomeClass::Degraded,
            None,
            None,
            drop_count,
        ),
    }
}

async fn drain_loop(
    pool: PgPool,
    mut receiver: mpsc::Receiver<QueuedObservation>,
    mut shutdown: watch::Receiver<bool>,
    state: Arc<WriterState>,
) {
    loop {
        let first = if *shutdown.borrow() {
            receiver.try_recv().ok()
        } else {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    continue;
                }
                observation = receiver.recv() => observation,
            }
        };
        let dropped = state.dropped.swap(0, Ordering::AcqRel);
        if first.is_none() && dropped == 0 {
            if *shutdown.borrow() || receiver.is_closed() {
                return;
            }
            continue;
        }

        let mut batch = Vec::with_capacity(INSULA_MAX_BATCH_EVENTS);
        if dropped > 0 {
            batch.push(drop_observation(&state, dropped));
        }
        if let Some(first) = first {
            batch.push(first);
        }
        while batch.len() < INSULA_MAX_BATCH_EVENTS {
            match receiver.try_recv() {
                Ok(observation) => batch.push(observation),
                Err(_) => break,
            }
        }
        persist_groups(&pool, batch).await;
    }
}

async fn persist_groups(pool: &PgPool, batch: Vec<QueuedObservation>) {
    let mut groups: Vec<(TrustedBinding, Vec<ObservationEvent>)> = Vec::new();
    for observation in batch {
        if let Some((_, events)) = groups
            .iter_mut()
            .find(|(binding, _)| *binding == observation.binding)
        {
            events.push(observation.event);
        } else {
            groups.push((observation.binding, vec![observation.event]));
        }
    }

    // The writer's own persistence is behind a recursion fence: Insula database work is never observed.
    for (binding, events) in groups {
        let _ = ingest_batch(pool, &binding, IngestBatch { events }).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Row;
    use sqlx::postgres::PgPoolOptions;
    use std::sync::Barrier;

    type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

    const INSULA_MIGRATION: &str = include_str!("../../../substrate/migrations/0022_insula.sql");

    #[tokio::test]
    async fn disabled_env_init_yields_none_spans() {
        const CHILD_MARKER: &str = "ATHANOR_INSULA_DISABLED_TEST_CHILD";
        if std::env::var(CHILD_MARKER).is_ok_and(|value| value == "1") {
            let pool = PgPoolOptions::new()
                .connect_lazy("postgres://localhost/insula_disabled_test")
                .expect("lazy test pool");
            init_insula_emitter(pool);
            assert!(
                start_span(&system_binding(), "akasha", "substrate", "test").is_none()
            );
            return;
        }

        // Environment mutation belongs in a child process so parallel test threads cannot race it.
        let status = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--exact",
                "insula_writer::tests::disabled_env_init_yields_none_spans",
            ])
            .env(CHILD_MARKER, "1")
            .env("ATHANOR_DISABLE_INSULA", "1")
            .status()
            .expect("disabled-environment child");
        assert!(status.success());
    }

    #[test]
    fn drop_counter_accumulates_when_queue_is_full() {
        let (sender, _receiver) = mpsc::channel(QUEUE_CAPACITY);
        let state = WriterState {
            writer_id: Uuid::new_v4(),
            sequence: AtomicI64::new(0),
            dropped: AtomicI64::new(0),
            accepting: AtomicBool::new(true),
            sender,
        };
        for _ in 0..QUEUE_CAPACITY + 2 {
            let event = observation(
                &state,
                Uuid::new_v4(),
                Uuid::new_v4(),
                "akasha",
                "substrate",
                "test",
                ObservationPhase::Point,
                None,
                OutcomeClass::Ok,
                None,
                None,
                0,
            );
            state.enqueue(QueuedObservation {
                binding: system_binding(),
                event,
            });
        }
        assert_eq!(state.dropped.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn writer_sequence_is_monotonic_across_threads() {
        let (sender, _receiver) = mpsc::channel(1);
        let state = Arc::new(WriterState {
            writer_id: Uuid::new_v4(),
            sequence: AtomicI64::new(0),
            dropped: AtomicI64::new(0),
            accepting: AtomicBool::new(true),
            sender,
        });
        let threads = 8;
        let per_thread = 1_000;
        let barrier = Arc::new(Barrier::new(threads));
        let mut workers = Vec::with_capacity(threads);
        for _ in 0..threads {
            let state = Arc::clone(&state);
            let barrier = Arc::clone(&barrier);
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                (0..per_thread)
                    .map(|_| state.next_sequence())
                    .collect::<Vec<_>>()
            }));
        }
        let mut sequences = workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("sequence worker"))
            .collect::<Vec<_>>();
        sequences.sort_unstable();
        assert_eq!(
            sequences,
            (1..=(threads * per_thread) as i64).collect::<Vec<_>>()
        );
    }

    fn isolated_database_url() -> String {
        let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")
            .expect("Insula writer proof requires a dedicated PostgreSQL URL");
        let lower = url.to_ascii_lowercase();
        assert!(
            !lower.contains("solarisael_memory") && !lower.contains("solarisael-house"),
            "refusing a live or production-looking database"
        );
        url
    }

    async fn fresh_insula() -> TestResult<PgPool> {
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&isolated_database_url())
            .await?;
        sqlx::query("DROP SCHEMA IF EXISTS insula CASCADE")
            .execute(&pool)
            .await?;
        sqlx::raw_sql(INSULA_MIGRATION).execute(&pool).await?;
        Ok(pool)
    }

    #[tokio::test]
    #[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; resets only its dedicated Insula schema"]
    async fn postgres_writer_contract_persists_span_and_receipt_point() -> TestResult {
        let pool = fresh_insula().await?;
        let binding = system_binding();
        init_insula_emitter(pool.clone());
        let span = start_span(&binding, "akasha", "substrate", "contract_span");
        assert!(span.is_some(), "system binding must initialize a span");
        end_span(span, OutcomeClass::Ok, None);
        let receipt_id = format!("receipt-{}", Uuid::new_v4());
        record_point(
            &binding,
            "akasha",
            "substrate",
            "contract_point",
            OutcomeClass::Ok,
            None,
            Some(("test_receipt", &receipt_id)),
        );
        flush_insula_emitter().await;

        let rows = sqlx::query(
            "SELECT writer_id::text AS writer_id, writer_sequence, room, spirit, session_id, \
                    operation, phase, outcome_class, duration_us, idempotency_scope, \
                    receipt_kind, receipt_id \
               FROM insula.log ORDER BY writer_sequence",
        )
        .fetch_all(&pool)
        .await?;
        assert_eq!(rows.len(), 3);
        let writer_id = EMITTER.get().expect("initialized emitter").state.writer_id;
        let sequences = rows
            .iter()
            .map(|row| row.get::<i64, _>("writer_sequence"))
            .collect::<Vec<_>>();
        assert_eq!(sequences, vec![1, 2, 3]);
        assert!(rows.iter().all(|row| {
            row.get::<String, _>("writer_id") == writer_id.to_string()
                && row.get::<String, _>("room") == binding.room
                && row.get::<String, _>("spirit") == binding.spirit
                && row.get::<String, _>("session_id") == binding.session_id
        }));
        assert_eq!(rows[0].get::<String, _>("phase"), "start");
        assert_eq!(rows[0].get::<String, _>("outcome_class"), "unknown");
        assert_eq!(rows[0].get::<String, _>("idempotency_scope"), "trace_span");
        assert_eq!(rows[1].get::<String, _>("phase"), "end");
        assert_eq!(rows[1].get::<String, _>("outcome_class"), "ok");
        assert!(rows[1].get::<Option<i64>, _>("duration_us").is_some());
        assert_eq!(rows[2].get::<String, _>("phase"), "point");
        assert_eq!(
            rows[2].get::<String, _>("idempotency_scope"),
            "room_operation"
        );
        assert_eq!(
            rows[2].get::<Option<String>, _>("receipt_kind").as_deref(),
            Some("test_receipt")
        );
        assert_eq!(
            rows[2].get::<Option<String>, _>("receipt_id").as_deref(),
            Some(receipt_id.as_str())
        );
        Ok(())
    }
}
