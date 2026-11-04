use crate::insula::{
    INSULA_MAX_BATCH_EVENTS, IdempotencyScope, IngestBatch, ObservationEvent, ObservationPhase,
    OutcomeClass, TrustedBinding, ingest_batch,
};
use chrono::{DateTime, Utc};
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
/// Exit flush ceiling. One ingest batch against a multi-GB insula.log costs
/// 0.8–2 s (measured 2026-09-05, 3 GB log), so the old 750 ms grace lost the
/// last recall of every short-lived child. Unaccounted loss goes to stderr;
/// Insula must not observe its own persistence.
pub const SHUTDOWN_FLUSH_CEILING: Duration = Duration::from_secs(5);

static EMITTER: OnceLock<Emitter> = OnceLock::new();

struct QueuedObservation {
    binding: TrustedBinding,
    event: ObservationEvent,
}

struct WriterState {
    writer_id: Uuid,
    sequence: AtomicI64,
    /// Originals without a persistence or durable drop receipt.
    /// Sequence numbers also include synthetic markers and cannot measure this.
    unaccounted: AtomicI64,
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

        self.unaccounted.fetch_add(1, Ordering::AcqRel);
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
    parent_span_id: Option<Uuid>,
    bytes_in: i64,
    started_at: Instant,
    /// Set by `defer_span`: the Start row has not been enqueued and is
    /// emitted at `end_span` with this begin time, or never if the span is
    /// dropped.
    deferred_start: Option<DateTime<Utc>>,
}

impl EmitterSpan {
    /// A child phase of this span: same trace, binding, component, and
    /// layer; its own span id; this span as parent. `None` when the name is
    /// not mechanical or the emitter has stopped accepting.
    pub fn child(&self, operation: &'static str) -> Option<EmitterSpan> {
        if !mechanical_name(operation) || !self.state.accepting.load(Ordering::Acquire) {
            return None;
        }
        let started_at = Instant::now();
        let span_id = Uuid::new_v4();
        let event = observation(
            &self.state,
            self.trace_id,
            span_id,
            self.component,
            self.layer,
            operation,
            ObservationPhase::Start,
            Some(self.span_id),
            None,
            OutcomeClass::Unknown,
            None,
            None,
            0,
            0,
        );
        self.state.enqueue(QueuedObservation {
            binding: self.binding.clone(),
            event,
        });
        Some(EmitterSpan {
            state: Arc::clone(&self.state),
            binding: self.binding.clone(),
            component: self.component,
            layer: self.layer,
            operation,
            trace_id: self.trace_id,
            span_id,
            parent_span_id: Some(self.span_id),
            bytes_in: 0,
            started_at,
            deferred_start: None,
        })
    }

    /// Bytes handed to the observed call; carried on the End event.
    pub fn set_bytes_in(&mut self, bytes: usize) {
        self.bytes_in = i64::try_from(bytes).unwrap_or(i64::MAX);
    }
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
            unaccounted: AtomicI64::new(0),
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
        None,
        OutcomeClass::Unknown,
        None,
        None,
        0,
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
        parent_span_id: None,
        bytes_in: 0,
        started_at,
        deferred_start: None,
    })
}

/// A span whose Start row is withheld until `end_span`, which then emits the
/// Start (at the real begin time) and the End together. Dropping the span
/// without ending it emits nothing. For polls that are usually empty: the
/// caller decides after the fact whether this turn was worth a row.
pub fn defer_span(
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
    Some(EmitterSpan {
        state: Arc::clone(&emitter.state),
        binding: binding.clone(),
        component,
        layer,
        operation,
        trace_id: Uuid::new_v4(),
        span_id: Uuid::new_v4(),
        parent_span_id: None,
        bytes_in: 0,
        started_at: Instant::now(),
        deferred_start: Some(Utc::now()),
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
    if let Some(began_at) = span.deferred_start {
        let mut start = observation(
            &span.state,
            span.trace_id,
            span.span_id,
            span.component,
            span.layer,
            span.operation,
            ObservationPhase::Start,
            span.parent_span_id,
            None,
            OutcomeClass::Unknown,
            None,
            None,
            0,
            0,
        );
        start.observed_at = began_at;
        span.state.enqueue(QueuedObservation {
            binding: span.binding.clone(),
            event: start,
        });
    }
    let event = observation(
        &span.state,
        span.trace_id,
        span.span_id,
        span.component,
        span.layer,
        span.operation,
        ObservationPhase::End,
        span.parent_span_id,
        Some(duration_us),
        outcome,
        error_class,
        None,
        span.bytes_in,
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
        None,
        outcome,
        error_class,
        receipt,
        0,
        0,
    );
    emitter.state.enqueue(QueuedObservation {
        binding: binding.clone(),
        event,
    });
}

/// A point that carries the duration of work already finished — for cost
/// paid before the emitter existed (the bootstrap that creates it) and so
/// impossible to span. `elapsed` is clamped to the schema's one-day maximum.
pub fn record_timed_point(
    binding: &TrustedBinding,
    component: &'static str,
    layer: &'static str,
    operation: &'static str,
    elapsed: Duration,
    outcome: OutcomeClass,
    error_class: Option<&str>,
) {
    if ![component, layer, operation]
        .into_iter()
        .all(mechanical_name)
    {
        return;
    }
    let Some(emitter) = EMITTER.get() else {
        return;
    };
    if !emitter.state.accepting.load(Ordering::Acquire) {
        return;
    }
    let duration_us = elapsed.as_micros().min(MAX_DURATION_US as u128) as i64;
    let event = observation(
        &emitter.state,
        Uuid::new_v4(),
        Uuid::new_v4(),
        component,
        layer,
        operation,
        ObservationPhase::Point,
        None,
        Some(duration_us),
        outcome,
        error_class,
        None,
        0,
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
    let Some(drain) = emitter.drain.lock().ok().and_then(|mut task| task.take()) else {
        return;
    };
    let unaccounted = drain_with_ceiling(drain, SHUTDOWN_FLUSH_CEILING, &emitter.state).await;

    if unaccounted > 0 {
        eprintln!(
            "insula_writer: shutdown left {unaccounted} observation(s) without a persistence or drop receipt (writer {})",
            emitter.state.writer_id
        );
    }
}

/// Wait for the drain until `ceiling`, then abort it if necessary.
/// Return originals without a persistence or durable drop receipt.
async fn drain_with_ceiling(
    mut drain: JoinHandle<()>,
    ceiling: Duration,
    state: &WriterState,
) -> i64 {
    if tokio::time::timeout(ceiling, &mut drain).await.is_err() {
        drain.abort();
        let _ = drain.await;
    }

    state.unaccounted.load(Ordering::Acquire)
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
    parent_span_id: Option<Uuid>,
    duration_us: Option<i64>,
    outcome_class: OutcomeClass,
    error_class: Option<&str>,
    receipt: Option<(&str, &str)>,
    bytes_in: i64,
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
        parent_span_id: parent_span_id.map(|id| id.to_string()),
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
        bytes_in,
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
            None,
            OutcomeClass::Degraded,
            None,
            None,
            0,
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
        persist_groups(&pool, batch, &state).await;
    }
}

async fn persist_groups(pool: &PgPool, batch: Vec<QueuedObservation>, state: &WriterState) {
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
        let drop_marker = events.iter().enumerate().find_map(|(index, event)| {
            (event.phase == ObservationPhase::Drop).then_some((index as u32, event.drop_count))
        });
        let result = ingest_batch(pool, &binding, IngestBatch { events }).await;

        let Ok(receipt) = result else {
            // Keep failed originals unaccounted; do not retry or observe persistence.
            continue;
        };

        let mut accounted = i64::from(receipt.accepted_count) + i64::from(receipt.duplicate_count);

        if let Some((index, drop_count)) = drop_marker {
            let marker_refused = receipt
                .conflicts
                .iter()
                .any(|item| item.event_index == index);

            if !marker_refused {
                accounted += drop_count - 1;
            }
        }

        if accounted > 0 {
            state.unaccounted.fetch_sub(accounted, Ordering::AcqRel);
        }
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
            assert!(start_span(&system_binding(), "akasha", "substrate", "test").is_none());
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

    fn test_state(sender: mpsc::Sender<QueuedObservation>) -> Arc<WriterState> {
        Arc::new(WriterState {
            writer_id: Uuid::new_v4(),
            sequence: AtomicI64::new(0),
            unaccounted: AtomicI64::new(0),
            dropped: AtomicI64::new(0),
            accepting: AtomicBool::new(true),
            sender,
        })
    }

    /// Queue overflow and an in-flight original each count once at the ceiling.
    #[tokio::test]
    async fn flush_ceiling_abandons_stuck_drain_and_counts_the_tail() {
        let (sender, mut receiver) = mpsc::channel(3);
        let state = test_state(sender);
        let binding = hardening_binding();

        for _ in 0..5 {
            state.enqueue(hardening_point(&state, &binding, None));
        }

        let in_flight = receiver.recv().await.expect("queued original");
        let stuck = tokio::spawn(async move {
            std::future::pending::<()>().await;
            drop(in_flight);
        });
        let started = Instant::now();
        let unflushed = drain_with_ceiling(stuck, Duration::from_millis(40), &state).await;
        assert!(started.elapsed() >= Duration::from_millis(40));
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "ceiling is bounded"
        );
        assert_eq!(unflushed, 5);
    }

    #[tokio::test]
    async fn hardening_completed_drain_keeps_unpersisted_originals() {
        let (sender, _receiver) = mpsc::channel(1);
        let state = test_state(sender);
        state.enqueue(hardening_point(&state, &hardening_binding(), None));

        let settling = tokio::spawn(async {});
        let unflushed = drain_with_ceiling(settling, Duration::from_secs(2), &state).await;

        assert_eq!(unflushed, 1, "task completion is not a persistence receipt");
    }

    /// A deferred span is silent until ended, then emits its Start at the
    /// real begin time followed by its End; dropped, it emits nothing.
    #[tokio::test]
    async fn deferred_span_emits_start_and_end_only_when_ended() {
        let (sender, mut receiver) = mpsc::channel(QUEUE_CAPACITY);
        let state = test_state(sender);
        let began_at = Utc::now() - chrono::Duration::milliseconds(250);
        let deferred = |operation: &'static str| EmitterSpan {
            state: Arc::clone(&state),
            binding: system_binding(),
            component: "akasha",
            layer: "substrate",
            operation,
            trace_id: Uuid::new_v4(),
            span_id: Uuid::new_v4(),
            parent_span_id: None,
            bytes_in: 0,
            started_at: Instant::now(),
            deferred_start: Some(began_at),
        };

        drop(deferred("knock_poll_empty"));
        assert!(
            receiver.try_recv().is_err(),
            "a dropped deferred span emits nothing"
        );

        let span = deferred("knock_poll_claimed");
        let span_id = span.span_id.to_string();
        end_span(Some(span), OutcomeClass::Ok, None);
        let start = receiver.try_recv().expect("start row").event;
        let end = receiver.try_recv().expect("end row").event;
        assert!(receiver.try_recv().is_err());
        assert_eq!(start.phase, ObservationPhase::Start);
        assert_eq!(start.observed_at, began_at);
        assert_eq!(start.span_id, span_id);
        assert!(start.duration_us.is_none());
        assert_eq!(end.phase, ObservationPhase::End);
        assert_eq!(end.span_id, span_id);
        assert!(end.duration_us.is_some());
        assert!(start.writer_sequence < end.writer_sequence);
    }

    #[test]
    fn drop_counter_accumulates_when_queue_is_full() {
        let (sender, _receiver) = mpsc::channel(QUEUE_CAPACITY);
        let state = WriterState {
            writer_id: Uuid::new_v4(),
            sequence: AtomicI64::new(0),
            unaccounted: AtomicI64::new(0),
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
                None,
                OutcomeClass::Ok,
                None,
                None,
                0,
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
            unaccounted: AtomicI64::new(0),
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
        let options: sqlx::postgres::PgConnectOptions = url.parse().expect("valid test URL");
        let database = options
            .get_database()
            .expect("explicit test database")
            .to_ascii_lowercase();
        assert!(
            database.contains("test") && !database.contains("solarisael"),
            "refusing a non-test or live database, including percent-encoded names"
        );
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
        sqlx::raw_sql(include_str!(
            "../../../substrate/migrations/0032_insula_seven_day_retention.sql"
        ))
        .execute(&pool)
        .await?;
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

    async fn hardening_pool() -> TestResult<PgPool> {
        let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")
            .expect("Insula writer proof requires a dedicated PostgreSQL URL");
        let options: sqlx::postgres::PgConnectOptions = url.parse()?;
        let database = options.get_database().expect("explicit test database");
        let database = database.to_ascii_lowercase();
        let is_test = database.contains("test") || database.starts_with("athanor_hardening_");
        let lower = url.to_ascii_lowercase();

        assert!(
            is_test
                && !database.contains("solarisael")
                && !database.contains("prod")
                && !database.contains("live")
                && !lower.contains("solarisael_memory")
                && !lower.contains("solarisael-house"),
            "refusing a non-test or production-looking database"
        );

        let pool = PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(Duration::from_secs(2))
            .connect_with(options)
            .await?;
        crate::migrations::run_migrations(&pool).await?;

        Ok(pool)
    }

    fn hardening_binding() -> TrustedBinding {
        TrustedBinding {
            house_id: format!("hardening-{}", Uuid::new_v4().simple()),
            room: "hardening".into(),
            spirit: "Hardening".into(),
            session_id: Uuid::new_v4().to_string(),
        }
    }

    fn hardening_point(
        state: &WriterState,
        binding: &TrustedBinding,
        error_class: Option<&str>,
    ) -> QueuedObservation {
        QueuedObservation {
            binding: binding.clone(),
            event: observation(
                state,
                Uuid::new_v4(),
                Uuid::new_v4(),
                "akasha",
                "substrate",
                "hardening_writer",
                ObservationPhase::Point,
                None,
                None,
                OutcomeClass::Ok,
                error_class,
                None,
                0,
                0,
            ),
        }
    }

    async fn hardening_flush(
        pool: &PgPool,
        receiver: mpsc::Receiver<QueuedObservation>,
        state: &Arc<WriterState>,
    ) -> i64 {
        state.accepting.store(false, Ordering::Release);
        let (shutdown, shutdown_receiver) = watch::channel(true);
        let drain = tokio::spawn(drain_loop(
            pool.clone(),
            receiver,
            shutdown_receiver,
            Arc::clone(state),
        ));
        let unflushed = drain_with_ceiling(drain, SHUTDOWN_FLUSH_CEILING, state).await;
        drop(shutdown);

        unflushed
    }

    async fn hardening_rows_and_cleanup(
        pool: &PgPool,
        state: &WriterState,
        binding: &TrustedBinding,
    ) -> TestResult<Vec<(String, String)>> {
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT event_id::text, session_id FROM insula.log
             WHERE writer_id = $1::uuid ORDER BY writer_sequence",
        )
        .bind(state.writer_id.to_string())
        .fetch_all(pool)
        .await;
        let mut tx = pool.begin().await?;
        sqlx::query("DELETE FROM insula.log WHERE writer_id = $1::uuid")
            .bind(state.writer_id.to_string())
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM insula.vitals_minute WHERE house_id = $1")
            .bind(&binding.house_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;

        Ok(rows?)
    }

    async fn hardening_success_control(pool: &PgPool) -> TestResult {
        let binding = hardening_binding();
        let (sender, receiver) = mpsc::channel(1);
        let state = test_state(sender);
        let point = hardening_point(&state, &binding, None);
        let event_id = point.event.event_id.clone();
        state.enqueue(point);

        let unflushed = hardening_flush(pool, receiver, &state).await;
        let rows = hardening_rows_and_cleanup(pool, &state, &binding).await?;

        assert_eq!(rows, vec![(event_id, binding.session_id)]);
        assert_eq!(unflushed, 0, "a persisted original is not a loss");

        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires a dedicated ATHANOR_SUBSTRATE_TEST_DATABASE_URL"]
    async fn hardening_graceful_drain_reports_failed_persistence() -> TestResult {
        let pool = hardening_pool().await?;
        hardening_success_control(&pool).await?;
        let unavailable = hardening_pool().await?;
        unavailable.close().await;
        let binding = hardening_binding();
        let (sender, receiver) = mpsc::channel(2);
        let state = test_state(sender);
        state.enqueue(hardening_point(&state, &binding, None));
        state.enqueue(hardening_point(&state, &binding, None));

        let unflushed = hardening_flush(&unavailable, receiver, &state).await;
        let rows = hardening_rows_and_cleanup(&pool, &state, &binding).await?;

        assert!(
            rows.is_empty(),
            "the closed pool cannot persist either original"
        );
        assert_eq!(
            unflushed, 2,
            "a completed drain must report both failed original observations"
        );

        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires a dedicated ATHANOR_SUBSTRATE_TEST_DATABASE_URL"]
    async fn hardening_failed_drop_marker_preserves_original_loss_count() -> TestResult {
        let pool = hardening_pool().await?;
        hardening_success_control(&pool).await?;
        let unavailable = hardening_pool().await?;
        unavailable.close().await;
        let binding = hardening_binding();
        let (sender, receiver) = mpsc::channel(1);
        let state = test_state(sender);

        for _ in 0..3 {
            state.enqueue(hardening_point(&state, &binding, None));
        }

        assert_eq!(receiver.len(), 1, "one original reaches the bounded queue");
        assert_eq!(state.dropped.load(Ordering::Acquire), 2);

        let unflushed = hardening_flush(&unavailable, receiver, &state).await;
        let rows = hardening_rows_and_cleanup(&pool, &state, &binding).await?;

        assert!(
            rows.is_empty(),
            "neither originals nor a drop marker reached PostgreSQL"
        );
        assert_eq!(
            unflushed, 3,
            "count the queued original and both overflow originals, not the synthetic marker"
        );

        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires a dedicated ATHANOR_SUBSTRATE_TEST_DATABASE_URL"]
    async fn hardening_mixed_bindings_account_only_the_refused_group() -> TestResult {
        let pool = hardening_pool().await?;
        let refused_binding = hardening_binding();
        let accepted_binding = TrustedBinding {
            session_id: Uuid::new_v4().to_string(),
            ..refused_binding.clone()
        };
        let (sender, receiver) = mpsc::channel(3);
        let state = test_state(sender);
        let invalid = hardening_point(&state, &refused_binding, Some("contradictory_outcome"));
        let valid_sibling = hardening_point(&state, &refused_binding, None);
        let accepted = hardening_point(&state, &accepted_binding, None);
        let accepted_id = accepted.event.event_id.clone();
        let refusal = ingest_batch(
            &pool,
            &refused_binding,
            IngestBatch {
                events: vec![invalid.event.clone(), valid_sibling.event.clone()],
            },
        )
        .await;
        state.enqueue(invalid);
        state.enqueue(valid_sibling);
        state.enqueue(accepted);

        let unflushed = hardening_flush(&pool, receiver, &state).await;
        let rows = hardening_rows_and_cleanup(&pool, &state, &refused_binding).await?;

        assert!(
            refusal.is_err(),
            "the real ingest door must refuse the contradictory group"
        );
        assert_eq!(
            rows,
            vec![(accepted_id, accepted_binding.session_id)],
            "the later binding must persist while the refused group stays absent"
        );
        assert_eq!(
            unflushed, 2,
            "both originals in the refused group are lost; the successful group is not"
        );

        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires a dedicated ATHANOR_SUBSTRATE_TEST_DATABASE_URL"]
    async fn hardening_conflict_receipt_counts_loss_but_duplicate_does_not() -> TestResult {
        let pool = hardening_pool().await?;
        let binding = hardening_binding();
        let (seed_sender, _seed_receiver) = mpsc::channel(1);
        let seed_state = test_state(seed_sender);
        let seed = hardening_point(&seed_state, &binding, None).event;
        let inserted = ingest_batch(
            &pool,
            &binding,
            IngestBatch {
                events: vec![seed.clone()],
            },
        )
        .await;

        let (duplicate_sender, duplicate_receiver) = mpsc::channel(1);
        let duplicate_state = test_state(duplicate_sender);
        let mut duplicate = hardening_point(&duplicate_state, &binding, None);
        duplicate.event.trace_id = seed.trace_id.clone();
        duplicate.event.span_id = seed.span_id.clone();
        duplicate_state.enqueue(duplicate);

        let duplicate_loss = hardening_flush(&pool, duplicate_receiver, &duplicate_state).await;
        let (conflict_sender, conflict_receiver) = mpsc::channel(2);
        let conflict_state = test_state(conflict_sender);
        let mut conflicting = hardening_point(&conflict_state, &binding, None);
        conflicting.event.trace_id = seed.trace_id.clone();
        conflicting.event.span_id = seed.span_id.clone();
        conflicting.event.bytes_in = 1;
        let conflict_id = conflicting.event.event_id.clone();
        let receipt = ingest_batch(
            &pool,
            &binding,
            IngestBatch {
                events: vec![conflicting.event.clone()],
            },
        )
        .await;
        let accepted = hardening_point(&conflict_state, &binding, None);
        let accepted_id = accepted.event.event_id.clone();
        conflict_state.enqueue(conflicting);
        conflict_state.enqueue(accepted);

        let conflict_loss = hardening_flush(&pool, conflict_receiver, &conflict_state).await;
        let incumbent = sqlx::query_as::<_, (i64, i64)>(
            "SELECT bytes_in, duplicate_count FROM insula.log WHERE event_id = $1::uuid",
        )
        .bind(&seed.event_id)
        .fetch_one(&pool)
        .await;
        let duplicate_rows = hardening_rows_and_cleanup(&pool, &duplicate_state, &binding).await?;
        let conflict_rows = hardening_rows_and_cleanup(&pool, &conflict_state, &binding).await?;
        let seed_rows = hardening_rows_and_cleanup(&pool, &seed_state, &binding).await?;
        let inserted = inserted?;
        let receipt = receipt?;

        assert_eq!(inserted.accepted_count, 1);
        assert!(inserted.conflicts.is_empty());
        assert_eq!(
            seed_rows,
            vec![(seed.event_id.clone(), binding.session_id.clone())]
        );
        assert_eq!(
            incumbent?,
            (0, 1),
            "only the matching redelivery increments duplicates"
        );
        assert!(
            duplicate_rows.is_empty(),
            "the matching original already exists"
        );
        assert_eq!(
            duplicate_loss, 0,
            "a matching duplicate is not a lost observation"
        );
        assert_eq!(receipt.accepted_count, 0);
        assert_eq!(receipt.duplicate_count, 0);
        assert_eq!(
            receipt.conflicts.len(),
            1,
            "Ok can still carry a refused original"
        );
        assert_eq!(receipt.conflicts[0].event_id, conflict_id);
        assert_eq!(receipt.conflicts[0].incumbent_event_id, seed.event_id);
        assert_eq!(
            conflict_rows,
            vec![(accepted_id, binding.session_id)],
            "the nonconflicting original in the same group must still persist"
        );
        assert_eq!(conflict_loss, 1, "the conflicting original did not persist");

        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires a dedicated ATHANOR_SUBSTRATE_TEST_DATABASE_URL and --test-threads=1"]
    async fn hardening_successful_drop_marker_accounts_for_overflow_originals() -> TestResult {
        let pool = hardening_pool().await?;
        let system = system_binding();
        let preexisting: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM insula.vitals_minute
                WHERE house_id = $1 AND room = $2 AND spirit = $3
                  AND component = 'akasha' AND layer = 'substrate'
                  AND operation = 'insula_writer' AND phase = 'drop'
             )",
        )
        .bind(&system.house_id)
        .bind(&system.room)
        .bind(&system.spirit)
        .fetch_one(&pool)
        .await?;

        assert!(
            !preexisting,
            "refusing a pre-existing system drop-marker bucket"
        );

        let binding = hardening_binding();
        let (sender, receiver) = mpsc::channel(1);
        let state = test_state(sender);

        for _ in 0..3 {
            state.enqueue(hardening_point(&state, &binding, None));
        }

        let unaccounted = hardening_flush(&pool, receiver, &state).await;
        let rows = sqlx::query_as::<_, (String, i64)>(
            "SELECT phase, drop_count FROM insula.log
             WHERE writer_id = $1::uuid ORDER BY writer_sequence",
        )
        .bind(state.writer_id.to_string())
        .fetch_all(&pool)
        .await;

        // A system marker shares rollup keys; delete its bucket only when every source row is ours.
        let deleted = sqlx::query(
            "WITH marker_bucket AS (
                SELECT date_trunc('minute', observed_at AT TIME ZONE 'UTC')
                           AT TIME ZONE 'UTC' AS minute,
                       house_id, room, spirit, component, layer, operation, phase, outcome_class
                FROM insula.log WHERE writer_id = $1::uuid AND phase = 'drop'
             )
             DELETE FROM insula.vitals_minute v USING marker_bucket b
             WHERE v.query_name = 'insula.vitals.minute' AND v.query_version = 1
               AND (v.minute, v.house_id, v.room, v.spirit, v.component,
                    v.layer, v.operation, v.phase, v.outcome_class)
                   = (b.minute, b.house_id, b.room, b.spirit, b.component,
                      b.layer, b.operation, b.phase, b.outcome_class)
               AND NOT EXISTS(
                   SELECT 1 FROM insula.log other
                   WHERE other.writer_id <> $1::uuid
                     AND (date_trunc('minute', other.observed_at AT TIME ZONE 'UTC')
                              AT TIME ZONE 'UTC',
                          other.house_id, other.room, other.spirit, other.component,
                          other.layer, other.operation, other.phase, other.outcome_class)
                         = (b.minute, b.house_id, b.room, b.spirit, b.component,
                            b.layer, b.operation, b.phase, b.outcome_class)
               )",
        )
        .bind(state.writer_id.to_string())
        .execute(&pool)
        .await?
        .rows_affected();

        assert_eq!(
            deleted, 1,
            "cleanup requires an exclusively owned marker bucket"
        );

        hardening_rows_and_cleanup(&pool, &state, &binding).await?;

        assert_eq!(
            rows?,
            vec![("point".into(), 0), ("drop".into(), 2)],
            "one original persists and a durable marker records both lost originals"
        );
        assert_eq!(
            unaccounted, 0,
            "durably recorded overflow is not an unaccounted shutdown tail"
        );

        Ok(())
    }
}
