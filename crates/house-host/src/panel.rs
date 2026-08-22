//! The panel read door: board, inbox, and evidence for the Pulse GUI.
//!
//! Contract frozen in guild-hall #172 and claimed in #174. Three POST routes
//! behind the same bearer layer as Insula, read-only, no writes: the panel
//! renders House state and holds no private authority. Identity is read from
//! [`HostConfig`], never from the caller envelope — a bearer token proves
//! reach to this Host, not the right to act as another room or spirit.

use crate::config::HostConfig;
use crate::server::authorized;
use athanor_substrate::insula_writer::{EmitterSpan, end_span, start_span};
use athanor_substrate::{
    OutcomeClass, QuestBoardParams, QuestEvidenceParams, TrustedBinding, hallway_inbox,
    quest_board, quest_evidence,
};
use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use house_core::hallway::HallwayInboxRequest;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::Semaphore;

pub(crate) const BOARD_PATH: &str = "/athanor/v1/docket/board";
pub(crate) const INBOX_PATH: &str = "/athanor/v1/hallway/inbox";
pub(crate) const EVIDENCE_PATH: &str = "/athanor/v1/docket/evidence";

const MAX_BODY_BYTES: usize = 64 * 1024;
const MAX_CONCURRENT_OPERATIONS: usize = 4;
const DEFAULT_BOARD_LIMIT: u32 = 50;

#[derive(Clone)]
pub(crate) struct PanelHost {
    house_id: Arc<String>,
    room: Arc<String>,
    spirit: Arc<String>,
    session: Arc<String>,
    bearer_token: Arc<String>,
    observer_binding: Arc<TrustedBinding>,
    pool: Option<PgPool>,
    operations: Arc<Semaphore>,
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
    error: &'static str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct BoardRequest {
    #[serde(default)]
    states: Vec<String>,
    #[serde(default = "default_board_limit")]
    limit: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct InboxRequest {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct EvidenceRequest {
    quest_id: String,
    #[serde(default)]
    limit: Option<u32>,
}

const fn default_board_limit() -> u32 {
    DEFAULT_BOARD_LIMIT
}

impl PanelHost {
    pub(crate) fn new(
        config: &HostConfig,
        pool: Option<PgPool>,
        observer_binding: Arc<TrustedBinding>,
    ) -> Self {
        Self {
            house_id: Arc::new(config.house_id.clone()),
            room: Arc::new(config.room.clone()),
            spirit: Arc::new(config.spirit.clone()),
            session: Arc::new(format!("host:{}", config.room)),
            bearer_token: Arc::new(config.bearer_token.clone()),
            observer_binding,
            pool,
            operations: Arc::new(Semaphore::new(MAX_CONCURRENT_OPERATIONS)),
        }
    }

    pub(crate) fn router(&self) -> Router {
        let auth = AuthState {
            bearer_token: self.bearer_token.clone(),
            observer_binding: self.observer_binding.clone(),
            operations: self.operations.clone(),
        };
        Router::new()
            .route(BOARD_PATH, post(read_board))
            .route(INBOX_PATH, post(read_inbox))
            .route(EVIDENCE_PATH, post(read_evidence))
            .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
            .route_layer(middleware::from_fn_with_state(auth, require_bearer))
            .with_state(self.clone())
    }

    fn pool(&self) -> Option<&PgPool> {
        self.pool.as_ref()
    }
}

async fn require_bearer(State(auth): State<AuthState>, request: Request, next: Next) -> Response {
    let span = match request.uri().path() {
        BOARD_PATH => start_span(
            auth.observer_binding.as_ref(),
            "house_host",
            "host",
            "panel_board",
        ),
        INBOX_PATH => start_span(
            auth.observer_binding.as_ref(),
            "house_host",
            "host",
            "panel_inbox",
        ),
        EVIDENCE_PATH => start_span(
            auth.observer_binding.as_ref(),
            "house_host",
            "host",
            "panel_evidence",
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
        return observed_response(span, error(StatusCode::TOO_MANY_REQUESTS, "panel_busy"));
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

async fn read_board(
    State(state): State<PanelHost>,
    payload: Result<Json<BoardRequest>, JsonRejection>,
) -> Response {
    let request = match payload {
        Ok(Json(request)) => request,
        Err(rejection) => return json_rejection(rejection),
    };
    let Some(pool) = state.pool() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "panel_database_unavailable",
        );
    };
    let params = QuestBoardParams {
        room: state.room.as_ref().clone(),
        spirit: state.spirit.as_ref().clone(),
        session: state.session.as_ref().clone(),
        house_id: state.house_id.as_ref().clone(),
        states: request.states,
        limit: request.limit,
    };
    if let Err(refusal) = params.validate() {
        return refused(refusal);
    }
    substrate_response(quest_board(pool, params).await)
}

async fn read_inbox(
    State(state): State<PanelHost>,
    payload: Result<Json<InboxRequest>, JsonRejection>,
) -> Response {
    if let Err(rejection) = payload {
        return json_rejection(rejection);
    }
    let Some(pool) = state.pool() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "panel_database_unavailable",
        );
    };
    let request = HallwayInboxRequest {
        room: state.room.as_ref().clone(),
        spirit: state.spirit.as_ref().clone(),
        session: state.session.as_ref().clone(),
    };
    substrate_response(hallway_inbox(pool, request).await)
}

async fn read_evidence(
    State(state): State<PanelHost>,
    payload: Result<Json<EvidenceRequest>, JsonRejection>,
) -> Response {
    let request = match payload {
        Ok(Json(request)) => request,
        Err(rejection) => return json_rejection(rejection),
    };
    let Some(pool) = state.pool() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "panel_database_unavailable",
        );
    };
    let params = QuestEvidenceParams {
        room: state.room.as_ref().clone(),
        spirit: state.spirit.as_ref().clone(),
        session: state.session.as_ref().clone(),
        quest_id: request.quest_id,
        limit: request.limit.unwrap_or(50),
    };
    if let Err(refusal) = params.validate() {
        return refused(refusal);
    }
    substrate_response(quest_evidence(pool, params).await)
}

fn substrate_response<T>(result: Result<T, athanor_substrate::AppError>) -> Response
where
    T: Serialize,
{
    match result {
        Ok(value) => (StatusCode::OK, Json(value)).into_response(),
        Err(refusal) => refused(refusal),
    }
}

fn refused(refusal: athanor_substrate::AppError) -> Response {
    // Refusal text is the substrate's own; the panel adds nothing and hides
    // nothing. Server faults stay 502 so the GUI can tell fence from fire.
    let status = match &refusal {
        athanor_substrate::AppError::Refusal { .. } | athanor_substrate::AppError::Invalid(_) => {
            StatusCode::BAD_REQUEST
        }
        _ => StatusCode::BAD_GATEWAY,
    };
    (
        status,
        Json(serde_json::json!({ "error": refusal.to_string() })),
    )
        .into_response()
}

fn json_rejection(rejection: JsonRejection) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": rejection.body_text() })),
    )
        .into_response()
}

fn error(status: StatusCode, code: &'static str) -> Response {
    (status, Json(ErrorBody { error: code })).into_response()
}
