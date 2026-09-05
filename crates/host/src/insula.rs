use crate::config::HostConfig;
use crate::server::authorized;
use akasha::insula_writer::{EmitterSpan, end_span, start_span};
use akasha::{
    INSULA_MAX_RETENTION_ROWS, INSULA_MAX_SPAN_ROWS, INSULA_MAX_TRACE_ROWS,
    INSULA_MAX_UNVERIFIED_EXIT_ROWS, INSULA_MAX_VITALS_ROWS, IngestBatch, InsulaError,
    ObservationPhase, OutcomeClass, RetentionReceiptRow, SpanRow, SpanWindow, SpansQuery, TraceRow,
    TraceScope, TrustedBinding, UnverifiedExitRow, VitalsQuery, VitalsRow, ingest_batch,
    query_retention, query_spans, query_trace, query_unverified_exit, query_vitals,
    validate_trusted_binding,
};
use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use tokio::sync::Semaphore;
use uuid::Uuid;

pub(crate) const EVENTS_PATH: &str = "/athanor/v1/insula/events";
pub(crate) const VITALS_PATH: &str = "/athanor/v1/insula/vitals";
pub(crate) const TRACE_PATH: &str = "/athanor/v1/insula/trace";
pub(crate) const RETENTION_PATH: &str = "/athanor/v1/insula/retention";
// The Pulse drawer's door from a lane to a trace id. A lane is a rollup and
// carries no trace identity, so without this read the trace route is
// unreachable from the surface (BUGS.md, 2026-09-05).
pub(crate) const SPANS_PATH: &str = "/athanor/v1/insula/spans";
// The restart plane's operator window: which sessions armed an exit and never
// came back. A read behind the same bearer as the rest of this family; it
// commands no restart and claims no intent.
pub(crate) const UNVERIFIED_EXIT_PATH: &str = "/athanor/v1/insula/unverified-exit";

const API_SCHEMA_VERSION: u16 = 1;
const MAX_BATCH_EVENTS: usize = 128;
const MAX_BODY_BYTES: usize = 512 * 1024;
const MAX_CONCURRENT_OPERATIONS: usize = 4;
const DEFAULT_VITALS_LIMIT: u32 = 500;
const MAX_VITALS_LIMIT: u32 = INSULA_MAX_VITALS_ROWS - 1;
const DEFAULT_TRACE_LIMIT: u32 = 100;
// Both read routes request one row beyond the caller's limit to report
// truncation honestly, so the accepted ceiling sits one under the substrate cap.
const MAX_TRACE_LIMIT: u32 = INSULA_MAX_TRACE_ROWS - 1;
const DEFAULT_RETENTION_LIMIT: u32 = 20;
const MAX_RETENTION_LIMIT: u32 = INSULA_MAX_RETENTION_ROWS - 1;
const DEFAULT_UNVERIFIED_EXIT_LIMIT: u32 = 20;
const MAX_UNVERIFIED_EXIT_LIMIT: u32 = INSULA_MAX_UNVERIFIED_EXIT_ROWS - 1;
const DEFAULT_SPANS_LIMIT: u32 = 10;
const MAX_SPANS_LIMIT: u32 = INSULA_MAX_SPAN_ROWS - 1;

const HEALTH_UNVERIFIED: u8 = 0;
const HEALTH_OK: u8 = 1;
const HEALTH_DEGRADED: u8 = 2;

#[derive(Clone)]
pub(crate) struct InsulaHost {
    binding: Arc<TrustedBinding>,
    observer_binding: Arc<TrustedBinding>,
    bearer_token: Arc<String>,
    pool: Option<PgPool>,
    operations: Arc<Semaphore>,
    health: Arc<InsulaHealth>,
}

struct InsulaHealth {
    state: AtomicU8,
    successful_operations: AtomicU64,
    failed_operations: AtomicU64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InsulaHealthSnapshot {
    schema_version: u16,
    status: &'static str,
    successful_operations: u64,
    failed_operations: u64,
}

#[derive(Clone)]
struct AuthState {
    bearer_token: Arc<String>,
    observer_binding: Arc<TrustedBinding>,
    operations: Arc<Semaphore>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorBody {
    schema_version: u16,
    error: &'static str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct VitalsRequest {
    component: Option<String>,
    layer: Option<String>,
    operation: Option<String>,
    phase: Option<ObservationPhase>,
    outcome_class: Option<OutcomeClass>,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    #[serde(default = "default_vitals_limit")]
    limit: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VitalsResponse<'a> {
    schema_version: u16,
    query_name: String,
    query_version: i16,
    house_id: &'a str,
    room: &'a str,
    spirit: &'a str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    limit: u32,
    truncated: bool,
    rows: Vec<VitalsRow>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct TraceRequest {
    trace_id: String,
    #[serde(default = "default_trace_limit")]
    limit: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TraceResponse<'a> {
    schema_version: u16,
    query_name: String,
    query_version: i16,
    house_id: &'a str,
    room: &'a str,
    trace_id: &'a str,
    limit: u32,
    truncated: bool,
    rows: Vec<TraceRow>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SpansRequest {
    operation: String,
    phase: Option<ObservationPhase>,
    outcome_class: Option<OutcomeClass>,
    window: SpanWindow,
    #[serde(default = "default_spans_limit")]
    limit: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SpansResponse<'a> {
    schema_version: u16,
    query_name: String,
    query_version: i16,
    house_id: &'a str,
    room: &'a str,
    operation: &'a str,
    window: &'static str,
    window_secs: i64,
    limit: u32,
    truncated: bool,
    rows: Vec<SpanRow>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RetentionRequest {
    #[serde(default = "default_retention_limit")]
    limit: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RetentionResponse<'a> {
    schema_version: u16,
    query_name: String,
    query_version: i16,
    house_id: &'a str,
    limit: u32,
    truncated: bool,
    rows: Vec<RetentionReceiptRow>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct UnverifiedExitRequest {
    #[serde(default = "default_unverified_exit_limit")]
    limit: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UnverifiedExitResponse<'a> {
    schema_version: u16,
    query_name: String,
    query_version: i16,
    house_id: &'a str,
    room: &'a str,
    window_secs: i64,
    limit: u32,
    truncated: bool,
    rows: Vec<UnverifiedExitRow>,
}

const fn default_vitals_limit() -> u32 {
    DEFAULT_VITALS_LIMIT
}

const fn default_trace_limit() -> u32 {
    DEFAULT_TRACE_LIMIT
}

const fn default_retention_limit() -> u32 {
    DEFAULT_RETENTION_LIMIT
}

const fn default_unverified_exit_limit() -> u32 {
    DEFAULT_UNVERIFIED_EXIT_LIMIT
}

const fn default_spans_limit() -> u32 {
    DEFAULT_SPANS_LIMIT
}

impl InsulaHost {
    pub(crate) fn new(
        config: &HostConfig,
        pool: Option<PgPool>,
        observer_binding: Arc<TrustedBinding>,
    ) -> Result<Self, String> {
        let binding = TrustedBinding {
            house_id: config.house_id.clone(),
            room: config.room.clone(),
            spirit: config.spirit.clone(),
            session_id: config.session.clone(),
        };
        validate_trusted_binding(&binding).map_err(|error| error.to_string())?;
        Ok(Self {
            binding: Arc::new(binding),
            observer_binding,
            bearer_token: Arc::new(config.bearer_token.clone()),
            pool,
            operations: Arc::new(Semaphore::new(MAX_CONCURRENT_OPERATIONS)),
            health: Arc::new(InsulaHealth {
                state: AtomicU8::new(HEALTH_UNVERIFIED),
                successful_operations: AtomicU64::new(0),
                failed_operations: AtomicU64::new(0),
            }),
        })
    }

    pub(crate) fn router(&self) -> Router {
        let auth = AuthState {
            bearer_token: self.bearer_token.clone(),
            observer_binding: self.observer_binding.clone(),
            operations: self.operations.clone(),
        };
        Router::new()
            .route(EVENTS_PATH, post(ingest_events))
            .route(VITALS_PATH, post(read_vitals))
            .route(TRACE_PATH, post(read_trace))
            .route(RETENTION_PATH, post(read_retention))
            .route(SPANS_PATH, post(read_spans))
            .route(UNVERIFIED_EXIT_PATH, post(read_unverified_exit))
            .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
            .route_layer(middleware::from_fn_with_state(auth, require_bearer))
            .with_state(self.clone())
    }

    pub(crate) fn health(&self) -> InsulaHealthSnapshot {
        let status = if self.pool.is_none() {
            "unavailable"
        } else {
            match self.health.state.load(Ordering::Relaxed) {
                HEALTH_OK => "ok",
                HEALTH_DEGRADED => "degraded",
                _ => "unverified",
            }
        };
        InsulaHealthSnapshot {
            schema_version: API_SCHEMA_VERSION,
            status,
            successful_operations: self.health.successful_operations.load(Ordering::Relaxed),
            failed_operations: self.health.failed_operations.load(Ordering::Relaxed),
        }
    }

    fn pool(&self) -> Option<&PgPool> {
        self.pool.as_ref()
    }

    fn succeeded(&self) {
        self.health
            .successful_operations
            .fetch_add(1, Ordering::Relaxed);
        self.health.state.store(HEALTH_OK, Ordering::Relaxed);
    }

    fn failed(&self) {
        self.health
            .failed_operations
            .fetch_add(1, Ordering::Relaxed);
        self.health.state.store(HEALTH_DEGRADED, Ordering::Relaxed);
    }
}

async fn require_bearer(State(auth): State<AuthState>, request: Request, next: Next) -> Response {
    // The ingress span ends at HTTP handling; the emitter drain is outside this recursion fence.
    let span = match request.uri().path() {
        EVENTS_PATH => start_span(
            auth.observer_binding.as_ref(),
            "host",
            "host",
            "insula_ingest",
        ),
        VITALS_PATH => start_span(
            auth.observer_binding.as_ref(),
            "host",
            "host",
            "insula_vitals",
        ),
        TRACE_PATH => start_span(
            auth.observer_binding.as_ref(),
            "host",
            "host",
            "insula_trace",
        ),
        RETENTION_PATH => start_span(
            auth.observer_binding.as_ref(),
            "host",
            "host",
            "insula_retention",
        ),
        SPANS_PATH => start_span(
            auth.observer_binding.as_ref(),
            "host",
            "host",
            "insula_spans",
        ),
        UNVERIFIED_EXIT_PATH => start_span(
            auth.observer_binding.as_ref(),
            "house_host",
            "host",
            "insula_unverified_exit",
        ),
        _ => None,
    };
    if !authorized(request.headers(), auth.bearer_token.as_str()) {
        return observed_response(span, error(StatusCode::UNAUTHORIZED, "unauthenticated"));
    }
    if request.uri().query().is_some() {
        return observed_response(span, error(StatusCode::BAD_REQUEST, "unexpected_query"));
    }
    let Ok(_permit) = auth.operations.try_acquire_owned() else {
        return observed_response(span, error(StatusCode::TOO_MANY_REQUESTS, "insula_busy"));
    };
    observed_response(span, next.run(request).await)
}

fn observed_response(span: Option<EmitterSpan>, response: Response) -> Response {
    let outcome = outcome_for_status(response.status());
    end_span(span, outcome, None);
    response
}

fn outcome_for_status(status: StatusCode) -> OutcomeClass {
    if status.is_success() {
        OutcomeClass::Ok
    } else if status == StatusCode::SERVICE_UNAVAILABLE {
        OutcomeClass::Degraded
    } else if status.is_client_error() {
        OutcomeClass::Refused
    } else {
        OutcomeClass::Error
    }
}

async fn ingest_events(
    State(state): State<InsulaHost>,
    payload: Result<Json<IngestBatch>, JsonRejection>,
) -> Response {
    let Json(batch) = match payload {
        Ok(payload) => payload,
        Err(rejection) => return json_rejection(rejection),
    };
    if batch.events.is_empty() {
        return error(StatusCode::UNPROCESSABLE_ENTITY, "empty_batch");
    }
    if batch.events.len() > MAX_BATCH_EVENTS {
        return error(StatusCode::PAYLOAD_TOO_LARGE, "batch_too_large");
    }
    let Some(pool) = state.pool() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "insula_unavailable");
    };

    substrate_response(
        &state,
        ingest_batch(pool, state.binding.as_ref(), batch).await,
    )
}

async fn read_vitals(
    State(state): State<InsulaHost>,
    payload: Result<Json<VitalsRequest>, JsonRejection>,
) -> Response {
    let Json(request) = match payload {
        Ok(payload) => payload,
        Err(rejection) => return json_rejection(rejection),
    };
    if request.end <= request.start
        || request.end - request.start > Duration::days(366)
        || request.limit == 0
        || request.limit > MAX_VITALS_LIMIT
    {
        return error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_request");
    }
    let Some(pool) = state.pool() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "insula_unavailable");
    };

    let requested_limit = request.limit;
    let query = VitalsQuery {
        house_id: state.binding.house_id.clone(),
        room: Some(state.binding.room.clone()),
        spirit: Some(state.binding.spirit.clone()),
        component: request.component,
        layer: request.layer,
        operation: request.operation,
        phase: request.phase,
        outcome_class: request.outcome_class,
        start: request.start,
        end: request.end,
        limit: requested_limit + 1,
    };
    substrate_response(
        &state,
        query_vitals(pool, &query).await.map(|mut result| {
            let truncated = result.rows.len() > requested_limit as usize;
            result.rows.truncate(requested_limit as usize);
            VitalsResponse {
                schema_version: API_SCHEMA_VERSION,
                query_name: result.query_name,
                query_version: result.query_version,
                house_id: state.binding.house_id.as_str(),
                room: state.binding.room.as_str(),
                spirit: state.binding.spirit.as_str(),
                start: query.start,
                end: query.end,
                limit: requested_limit,
                truncated,
                rows: result.rows,
            }
        }),
    )
}

async fn read_trace(
    State(state): State<InsulaHost>,
    payload: Result<Json<TraceRequest>, JsonRejection>,
) -> Response {
    let Json(request) = match payload {
        Ok(payload) => payload,
        Err(rejection) => return json_rejection(rejection),
    };
    // The substrate remains the authority on trace identity; refusing a
    // malformed one here keeps a garbage read off the pool entirely.
    let canonical_trace = Uuid::parse_str(&request.trace_id)
        .is_ok_and(|parsed| parsed.to_string() == request.trace_id);
    if !canonical_trace || request.limit == 0 || request.limit > MAX_TRACE_LIMIT {
        return error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_request");
    }
    let Some(pool) = state.pool() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "insula_unavailable");
    };

    // Authority: the scope is this Host's configured House *and* its configured
    // room. A house-wide read (room None) would let this room walk spans another
    // room's writers recorded, which the Host has no authority to grant. Spirit
    // and session stay None on purpose: within its own room the Host may see its
    // spirit rotations and every session across restarts, and a trace routinely
    // crosses both.
    let scope = TraceScope {
        house_id: state.binding.house_id.clone(),
        room: Some(state.binding.room.clone()),
        spirit: None,
        session_id: None,
    };
    let requested_limit = request.limit;
    substrate_response(
        &state,
        query_trace(pool, &scope, &request.trace_id, requested_limit + 1)
            .await
            .map(|mut result| {
                let truncated = result.rows.len() > requested_limit as usize;
                result.rows.truncate(requested_limit as usize);
                TraceResponse {
                    schema_version: API_SCHEMA_VERSION,
                    query_name: result.query_name,
                    query_version: result.query_version,
                    house_id: state.binding.house_id.as_str(),
                    room: state.binding.room.as_str(),
                    trace_id: request.trace_id.as_str(),
                    limit: requested_limit,
                    truncated,
                    rows: result.rows,
                }
            }),
    )
}

async fn read_spans(
    State(state): State<InsulaHost>,
    payload: Result<Json<SpansRequest>, JsonRejection>,
) -> Response {
    let Json(request) = match payload {
        Ok(payload) => payload,
        Err(rejection) => return json_rejection(rejection),
    };
    if request.limit == 0 || request.limit > MAX_SPANS_LIMIT {
        return error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_request");
    }
    let Some(pool) = state.pool() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "insula_unavailable");
    };

    // Authority: House and room both come from this Host's trusted binding. The
    // caller names a lane inside its own room and nothing wider. Letting the
    // body carry a room would hand one bearer another room's span identities,
    // and every one of those is a working key into `insula/trace`.
    let requested_limit = request.limit;
    let query = SpansQuery {
        house_id: state.binding.house_id.clone(),
        room: state.binding.room.clone(),
        operation: request.operation,
        phase: request.phase,
        outcome_class: request.outcome_class,
        window: request.window,
        limit: requested_limit + 1,
    };
    substrate_response(
        &state,
        query_spans(pool, &query).await.map(|mut result| {
            let truncated = result.rows.len() > requested_limit as usize;
            result.rows.truncate(requested_limit as usize);
            SpansResponse {
                schema_version: API_SCHEMA_VERSION,
                query_name: result.query_name,
                query_version: result.query_version,
                house_id: state.binding.house_id.as_str(),
                room: state.binding.room.as_str(),
                operation: query.operation.as_str(),
                window: query.window.as_str(),
                window_secs: result.window_secs,
                limit: requested_limit,
                truncated,
                rows: result.rows,
            }
        }),
    )
}

async fn read_retention(
    State(state): State<InsulaHost>,
    payload: Result<Json<RetentionRequest>, JsonRejection>,
) -> Response {
    let Json(request) = match payload {
        Ok(payload) => payload,
        Err(rejection) => return json_rejection(rejection),
    };
    if request.limit == 0 || request.limit > MAX_RETENTION_LIMIT {
        return error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_request");
    }
    let Some(pool) = state.pool() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "insula_unavailable");
    };

    // A sweep receipt is House-wide by construction: retention deletes raw rows
    // for the whole House at once and the relation carries no room dimension.
    // The House still comes from config, never from the caller.
    let requested_limit = request.limit;
    substrate_response(
        &state,
        query_retention(pool, state.binding.house_id.as_str(), requested_limit + 1)
            .await
            .map(|mut result| {
                let truncated = result.rows.len() > requested_limit as usize;
                result.rows.truncate(requested_limit as usize);
                RetentionResponse {
                    schema_version: API_SCHEMA_VERSION,
                    query_name: result.query_name,
                    query_version: result.query_version,
                    house_id: state.binding.house_id.as_str(),
                    limit: requested_limit,
                    truncated,
                    rows: result.rows,
                }
            }),
    )
}

async fn read_unverified_exit(
    State(state): State<InsulaHost>,
    payload: Result<Json<UnverifiedExitRequest>, JsonRejection>,
) -> Response {
    let Json(request) = match payload {
        Ok(payload) => payload,
        Err(rejection) => return json_rejection(rejection),
    };
    if request.limit == 0 || request.limit > MAX_UNVERIFIED_EXIT_LIMIT {
        return error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_request");
    }
    let Some(pool) = state.pool() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "insula_unavailable");
    };

    // A restart intent carries no house dimension: it is scoped by workspace,
    // and the workspace belongs to a room. The room comes from this Host's
    // binding, never from the caller, or one bearer would read another room's
    // workspace path and requester session. The window and the row shape are
    // the substrate's, so nothing here decides restart policy.
    let requested_limit = request.limit;
    substrate_response(
        &state,
        query_unverified_exit(pool, state.binding.room.as_str(), requested_limit + 1)
            .await
            .map(|mut result| {
                let truncated = result.rows.len() > requested_limit as usize;
                result.rows.truncate(requested_limit as usize);
                UnverifiedExitResponse {
                    schema_version: API_SCHEMA_VERSION,
                    query_name: result.query_name,
                    query_version: result.query_version,
                    house_id: state.binding.house_id.as_str(),
                    room: state.binding.room.as_str(),
                    window_secs: result.window_secs,
                    limit: requested_limit,
                    truncated,
                    rows: result.rows,
                }
            }),
    )
}

fn substrate_response<T>(state: &InsulaHost, result: Result<T, InsulaError>) -> Response
where
    T: Serialize,
{
    match result {
        Ok(value) => {
            state.succeeded();
            Json(value).into_response()
        }
        Err(InsulaError::Validation { .. }) => {
            error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_request")
        }
        Err(InsulaError::Database(_)) => {
            state.failed();
            error(StatusCode::SERVICE_UNAVAILABLE, "insula_degraded")
        }
        Err(InsulaError::Invariant(_)) => {
            state.failed();
            error(StatusCode::INTERNAL_SERVER_ERROR, "insula_invariant")
        }
    }
}

fn json_rejection(rejection: JsonRejection) -> Response {
    if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
        error(StatusCode::PAYLOAD_TOO_LARGE, "body_too_large")
    } else {
        error(StatusCode::BAD_REQUEST, "invalid_json")
    }
}

fn error(status: StatusCode, code: &'static str) -> Response {
    (
        status,
        Json(ErrorBody {
            schema_version: API_SCHEMA_VERSION,
            error: code,
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::{
        InsulaHost, MAX_SPANS_LIMIT, SPANS_PATH, TRACE_PATH, VITALS_PATH, outcome_for_status,
    };
    use crate::config::{HostConfig, KnockAutonomy};
    use akasha::{OutcomeClass, TrustedBinding};
    use axum::http::StatusCode;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    const BEARER: &str = "spans-route-proof-token";

    fn config() -> HostConfig {
        HostConfig {
            bind: "127.0.0.1:0".parse().expect("loopback bind"),
            bearer_token: BEARER.to_owned(),
            room_dir: PathBuf::from("."),
            state_dir: PathBuf::from("."),
            house_id: "solarisael".to_owned(),
            room: "kodo".to_owned(),
            spirit: "Kodo".to_owned(),
            session: "service:kodo".to_owned(),
            database_url: None,
            nats_url: None,
            knock_autonomy: KnockAutonomy::Off,
        }
    }

    /// The bearer gate lives in middleware ahead of every handler, so the proof
    /// speaks real HTTP to a real listener rather than calling the handler
    /// directly. `pool` is None on purpose: an authenticated request would
    /// answer 503 `insula_unavailable`, which is exactly how the test can tell
    /// the gate refused *before* the handler rather than after it.
    async fn ask(path: &str, authorization: Option<&str>) -> (StatusCode, String) {
        let binding = Arc::new(TrustedBinding {
            house_id: "solarisael".to_owned(),
            room: "kodo".to_owned(),
            spirit: "Kodo".to_owned(),
            session_id: "service:kodo".to_owned(),
        });
        let insula = InsulaHost::new(&config(), None, binding).expect("bound Insula host");
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("bound address");
        let server = tokio::spawn(async move {
            axum::serve(listener, insula.router())
                .with_graceful_shutdown(async {
                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                })
                .await
                .ok();
        });

        let body = "{\"operation\":\"tool_call\",\"window\":\"1h\",\"limit\":5}";
        let header = match authorization {
            Some(token) => format!("Authorization: Bearer {token}\r\n"),
            None => String::new(),
        };
        let request = format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\ncontent-type: application/json\r\n\
             {header}content-length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let mut stream = TcpStream::connect(address).await.expect("connect");
        stream
            .write_all(request.as_bytes())
            .await
            .expect("write request");
        let mut answer = String::new();
        stream
            .read_to_string(&mut answer)
            .await
            .expect("read response");
        server.abort();

        let status = answer
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse::<u16>().ok())
            .and_then(|code| StatusCode::from_u16(code).ok())
            .unwrap_or_else(|| panic!("no status line in {answer}"));
        (status, answer)
    }

    #[tokio::test]
    async fn spans_route_refuses_an_absent_bearer_before_the_handler() {
        let (status, body) = ask(SPANS_PATH, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(body.contains("unauthenticated"), "{body}");
        // Not the handler's own unavailable answer: the gate stopped it first.
        assert!(!body.contains("insula_unavailable"), "{body}");
    }

    #[tokio::test]
    async fn spans_route_refuses_a_wrong_bearer() {
        let (status, body) = ask(SPANS_PATH, Some("not-the-token")).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(body.contains("unauthenticated"), "{body}");
    }

    /// The gate is the same one the proven routes sit behind, and the spans
    /// route is mounted inside it rather than beside it.
    #[tokio::test]
    async fn spans_route_shares_the_bearer_gate_with_vitals_and_trace() {
        for path in [VITALS_PATH, TRACE_PATH, SPANS_PATH] {
            let (status, _) = ask(path, None).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{path}");
        }
    }

    /// With the bearer present the request reaches the handler, which proves the
    /// route is really mounted. No pool, so the honest answer is 503.
    #[tokio::test]
    async fn spans_route_with_a_bearer_reaches_the_handler() {
        let (status, body) = ask(SPANS_PATH, Some(BEARER)).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(body.contains("insula_unavailable"), "{body}");
    }

    #[test]
    fn spans_limit_leaves_room_for_the_truncation_probe() {
        assert_eq!(MAX_SPANS_LIMIT, 100);
        assert!(MAX_SPANS_LIMIT + 1 <= akasha::INSULA_MAX_SPAN_ROWS);
    }

    #[test]
    fn insula_http_observation_outcomes_follow_response_class() {
        for status in [
            StatusCode::BAD_REQUEST,
            StatusCode::UNAUTHORIZED,
            StatusCode::PAYLOAD_TOO_LARGE,
            StatusCode::UNPROCESSABLE_ENTITY,
            StatusCode::TOO_MANY_REQUESTS,
        ] {
            assert_eq!(outcome_for_status(status), OutcomeClass::Refused);
        }
        assert_eq!(outcome_for_status(StatusCode::OK), OutcomeClass::Ok);
        assert_eq!(
            outcome_for_status(StatusCode::SERVICE_UNAVAILABLE),
            OutcomeClass::Degraded
        );
        assert_eq!(
            outcome_for_status(StatusCode::INTERNAL_SERVER_ERROR),
            OutcomeClass::Error
        );
    }
}
