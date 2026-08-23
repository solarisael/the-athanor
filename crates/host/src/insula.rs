use crate::config::HostConfig;
use crate::server::authorized;
use akasha::insula_writer::{EmitterSpan, end_span, start_span};
use akasha::{
    INSULA_MAX_RETENTION_ROWS, INSULA_MAX_TRACE_ROWS, INSULA_MAX_VITALS_ROWS, IngestBatch,
    InsulaError, ObservationPhase, OutcomeClass, RetentionReceiptRow, TraceRow, TraceScope,
    TrustedBinding, VitalsQuery, VitalsRow, ingest_batch, query_retention, query_trace,
    query_vitals, validate_trusted_binding,
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

const fn default_vitals_limit() -> u32 {
    DEFAULT_VITALS_LIMIT
}

const fn default_trace_limit() -> u32 {
    DEFAULT_TRACE_LIMIT
}

const fn default_retention_limit() -> u32 {
    DEFAULT_RETENTION_LIMIT
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
        ingest_batch(pool, state.binding.as_ref(), &batch).await,
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
    use super::outcome_for_status;
    use akasha::OutcomeClass;
    use axum::http::StatusCode;

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
