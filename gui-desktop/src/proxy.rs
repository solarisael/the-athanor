use std::{collections::HashMap, path::PathBuf, sync::Arc};
use axum::{body::Bytes, extract::{Request, State}, http::{header, Method, StatusCode}, response::{IntoResponse, Response}, Router};
use rust_embed::RustEmbed;
use serde::Deserialize;

#[derive(RustEmbed)]
#[folder = "../gui-prototype/"]
#[exclude = "serve.ts"]
#[exclude = "*.test.js"]
#[exclude = "*.md"]
#[exclude = ".scratch/**"]
struct Assets;

#[derive(Deserialize)]
struct Route { path: String, method: String }

pub struct Proxy {
    routes: HashMap<String, Route>,
    client: reqwest::Client,
    base: String,
    token: String,
    dev_dir: Option<PathBuf>,
}

impl Proxy {
    pub fn new(host_port: u16, room: &str, token: String, dev_dir: Option<PathBuf>) -> anyhow::Result<Self> {
        let room = percent_encoding::utf8_percent_encode(room, percent_encoding::NON_ALPHANUMERIC);
        Ok(Self {
            routes: serde_json::from_str(include_str!("../../gui-prototype/live-routes.json"))?,
            client: reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build()?,
            base: format!("http://127.0.0.1:{host_port}/room/{room}"),
            token,
            dev_dir: dev_dir.map(std::fs::canonicalize).transpose()?,
        })
    }

    pub fn router(self) -> Router {
        Router::new().fallback(serve).with_state(Arc::new(self))
    }

    async fn live(&self, request: Request) -> Response {
        let Some(route) = self.routes.get(request.uri().path()) else {
            return (StatusCode::NOT_FOUND, "unknown live route").into_response();
        };
        if request.method() != Method::POST {
            return (StatusCode::METHOD_NOT_ALLOWED, "POST only").into_response();
        }
        match self.forward(route, request).await {
            Ok(response) => response,
            Err(error) => (StatusCode::BAD_GATEWAY, axum::Json(serde_json::json!({"error": format!("Host request failed: {error}")}))).into_response(),
        }
    }

    async fn forward(&self, route: &Route, request: Request) -> anyhow::Result<Response> {
        let method = Method::from_bytes(route.method.as_bytes())?;
        let mut upstream = self.client.request(method.clone(), format!("{}{}", self.base, route.path))
            .bearer_auth(&self.token).header(header::CONTENT_TYPE, "application/json");
        if method != Method::GET {
            upstream = upstream.body(axum::body::to_bytes(request.into_body(), 2 * 1024 * 1024).await?);
        }
        let response = upstream.send().await?;
        let status = response.status();
        let body = response.bytes().await?;
        Ok((status, [(header::CONTENT_TYPE, "application/json")], body).into_response())
    }

    async fn static_file(&self, path: &str) -> Response {
        let path = if path == "/" { "index.html" } else { path.trim_start_matches('/') };
        let mime = mime_guess::from_path(path).first_or_octet_stream().to_string();
        let body = if let Some(root) = &self.dev_dir {
            let file = root.join(path);
            match tokio::fs::canonicalize(&file).await {
                Ok(file) if file.starts_with(root) => tokio::fs::read(file).await.ok().map(Bytes::from),
                _ => None,
            }
        } else {
            Assets::get(path).map(|file| match file.data {
                std::borrow::Cow::Borrowed(bytes) => Bytes::from_static(bytes),
                std::borrow::Cow::Owned(bytes) => Bytes::from(bytes),
            })
        };
        match body {
            Some(body) => ([(header::CONTENT_TYPE, mime)], body).into_response(),
            None => (StatusCode::NOT_FOUND, "not found").into_response(),
        }
    }
}

async fn serve(State(proxy): State<Arc<Proxy>>, request: Request) -> Response {
    let decoded = match percent_encoding::percent_decode_str(request.uri().path()).decode_utf8() {
        Ok(path) => path,
        Err(_) => return (StatusCode::BAD_REQUEST, "refused").into_response(),
    };
    if decoded.contains("..") || decoded.contains('\\') || decoded.contains(':') {
        return (StatusCode::BAD_REQUEST, "refused").into_response();
    }
    if decoded.starts_with("/live/") {
        return proxy.live(request).await;
    }
    proxy.static_file(&decoded).await
}
