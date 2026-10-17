use axum::{
    Router,
    body::Bytes,
    extract::{Request, State},
    http::{Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;
use serde::Deserialize;
use std::{collections::HashMap, path::PathBuf, sync::Arc};

#[derive(RustEmbed)]
#[folder = "../gui-prototype/"]
#[exclude = "serve.ts"]
#[exclude = "*.test.js"]
#[exclude = "*.md"]
#[exclude = ".scratch/**"]
struct Assets;

#[derive(Deserialize)]
struct Route {
    path: String,
    method: String,
}

pub struct Proxy {
    routes: HashMap<String, Route>,
    client: reqwest::Client,
    base: String,
    default_room: String,
    rooms: Vec<String>,
    token: String,
    dev_dir: Option<PathBuf>,
    app_exe: PathBuf,
    authority: String,
    origin: String,
}

impl Proxy {
    pub fn new(
        host_port: u16,
        room: &str,
        rooms: Vec<String>,
        token: String,
        dev_dir: Option<PathBuf>,
        app_exe: PathBuf,
        port: u16,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            rooms.iter().any(|configured| configured == room),
            "launch room is not configured"
        );
        Ok(Self {
            routes: serde_json::from_str(include_str!("../../gui-prototype/live-routes.json"))?,
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(std::time::Duration::from_secs(20))
                .build()?,
            base: format!("http://127.0.0.1:{host_port}"),
            default_room: room.to_owned(),
            rooms,
            token,
            dev_dir: dev_dir.map(std::fs::canonicalize).transpose()?,
            app_exe,
            authority: format!("127.0.0.1:{port}"),
            origin: format!("http://127.0.0.1:{port}"),
        })
    }

    pub fn router(self) -> Router {
        Router::new().fallback(serve).with_state(Arc::new(self))
    }

    async fn local_rooms(&self, request: Request) -> Response {
        let body = match axum::body::to_bytes(request.into_body(), 1024).await {
            Ok(body) => body,
            Err(_) => return (StatusCode::BAD_REQUEST, "invalid JSON body").into_response(),
        };
        if !body.iter().all(u8::is_ascii_whitespace) {
            match serde_json::from_slice::<serde_json::Value>(&body) {
                Ok(serde_json::Value::Object(fields)) if fields.is_empty() => {}
                Ok(_) => {
                    return (StatusCode::BAD_REQUEST, "no body fields allowed").into_response();
                }
                Err(_) => return (StatusCode::BAD_REQUEST, "invalid JSON body").into_response(),
            }
        }
        axum::Json(serde_json::json!({
            "defaultRoom": self.default_room,
            "rooms": self.rooms,
            "capabilities": { "repair": true }
        }))
        .into_response()
    }

    async fn repair_status(&self) -> Response {
        self.run_repair(false, false).await
    }

    async fn repair_start(&self, request: Request) -> Response {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Start {
            service: bool,
        }
        let body = match axum::body::to_bytes(request.into_body(), 1024).await {
            Ok(body) => body,
            Err(error) => return repair_error(StatusCode::BAD_REQUEST, error),
        };
        match serde_json::from_slice::<Start>(&body) {
            Ok(start) => self.run_repair(true, start.service).await,
            Err(error) => repair_error(StatusCode::BAD_REQUEST, error),
        }
    }

    async fn run_repair(&self, start: bool, service: bool) -> Response {
        let exe = self.app_exe.clone();
        match tokio::task::spawn_blocking(move || repair_command(&exe, start, service)).await {
            Ok(Ok(value)) => axum::Json(value).into_response(),
            Ok(Err(error)) => repair_error(StatusCode::BAD_GATEWAY, error),
            Err(error) => repair_error(StatusCode::BAD_GATEWAY, error),
        }
    }

    async fn live(&self, request: Request) -> Response {
        let Some(route) = self.routes.get(request.uri().path()) else {
            return (StatusCode::NOT_FOUND, "unknown live route").into_response();
        };
        if request.method() != Method::POST {
            return (StatusCode::METHOD_NOT_ALLOWED, "POST only").into_response();
        }
        let mut room_headers = request.headers().get_all("x-athanor-room").iter();
        let room = match room_headers.next() {
            None => &self.default_room,
            Some(value) => {
                let Some(room) = value.to_str().ok().and_then(|key| {
                    self.rooms
                        .iter()
                        .find(|configured| configured.as_str() == key)
                }) else {
                    return (StatusCode::FORBIDDEN, "unknown room").into_response();
                };
                room
            }
        };
        if room_headers.next().is_some() {
            return (StatusCode::FORBIDDEN, "unknown room").into_response();
        }
        let session_route = request.uri().path() == "/live/session";
        let body = if route.method == "GET" {
            None
        } else {
            let bytes = match axum::body::to_bytes(request.into_body(), 2 * 1024 * 1024).await {
                Ok(bytes) => bytes,
                Err(_) => return (StatusCode::BAD_REQUEST, "invalid live body").into_response(),
            };
            if session_route {
                let value = match serde_json::from_slice::<serde_json::Value>(&bytes) {
                    Ok(serde_json::Value::Object(fields)) => serde_json::Value::Object(fields),
                    Ok(_) => {
                        return (
                            StatusCode::BAD_REQUEST,
                            "session body must be a JSON object",
                        )
                            .into_response();
                    }
                    Err(_) => {
                        return (StatusCode::BAD_REQUEST, "invalid JSON body").into_response();
                    }
                };
                if !matches!(
                    value["action"].as_str(),
                    Some("status" | "command" | "commandStatus")
                ) {
                    return (StatusCode::FORBIDDEN, "session action refused").into_response();
                }
                Some(Bytes::from(value.to_string()))
            } else {
                Some(bytes)
            }
        };
        // The Host answering an error passes through with its own status; only a
        // transport failure becomes 502 here, and the body names which hop failed.
        match self.forward(route, room, body).await {
            Ok(response) => response,
            Err(error) => {
                let hop = match error.downcast_ref::<reqwest::Error>() {
                    Some(inner) if inner.is_timeout() => "host_timeout",
                    Some(inner) if inner.is_connect() => "host_unreachable",
                    Some(_) => "host_transport",
                    None => "proxy",
                };
                (
                    StatusCode::BAD_GATEWAY,
                    axum::Json(serde_json::json!({
                        "error": format!("Host request failed: {error}"),
                        "hop": hop
                    })),
                )
                    .into_response()
            }
        }
    }

    async fn forward(
        &self,
        route: &Route,
        room: &str,
        body: Option<Bytes>,
    ) -> anyhow::Result<Response> {
        let method = Method::from_bytes(route.method.as_bytes())?;
        let room = percent_encoding::utf8_percent_encode(room, percent_encoding::NON_ALPHANUMERIC);
        let mut upstream = self
            .client
            .request(method, format!("{}/room/{room}{}", self.base, route.path))
            .bearer_auth(&self.token)
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(body) = body {
            upstream = upstream.body(body);
        }
        let response = upstream.send().await?;
        let status = response.status();
        let body = response.bytes().await?;
        Ok((status, [(header::CONTENT_TYPE, "application/json")], body).into_response())
    }

    async fn static_file(&self, path: &str) -> Response {
        let path = if path == "/" {
            "index.html"
        } else {
            path.trim_start_matches('/')
        };
        let mime = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();
        let body = if let Some(root) = &self.dev_dir {
            let file = root.join(path);
            match tokio::fs::canonicalize(&file).await {
                Ok(file) if file.starts_with(root) => {
                    tokio::fs::read(file).await.ok().map(Bytes::from)
                }
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
    // Guard before forwarding a bearer or starting a local repair process.
    let headers = request.headers();
    let single = |name: &str| {
        let mut values = headers.get_all(name).iter();
        let first = values.next().and_then(|value| value.to_str().ok());
        if values.next().is_some() { None } else { first }
    };
    let origin = single("origin");
    let unsafe_method = request.method() != Method::GET && request.method() != Method::HEAD;
    if single("host") != Some(proxy.authority.as_str())
        || (headers.contains_key("origin") && origin != Some(proxy.origin.as_str()))
        || (unsafe_method && origin.is_none())
        || single("sec-fetch-site") == Some("cross-site")
    {
        return (StatusCode::FORBIDDEN, "forbidden origin").into_response();
    }
    let decoded = match percent_encoding::percent_decode_str(request.uri().path()).decode_utf8() {
        Ok(path) => path,
        Err(_) => return (StatusCode::BAD_REQUEST, "refused").into_response(),
    };
    if decoded.contains("..") || decoded.contains('\\') || decoded.contains(':') {
        return (StatusCode::BAD_REQUEST, "refused").into_response();
    }
    if decoded.starts_with("/local/") {
        let path = decoded.into_owned();
        if path != "/local/rooms" && path != "/local/repair/status" && path != "/local/repair/start"
        {
            return (StatusCode::NOT_FOUND, "unknown local route").into_response();
        }
        if request.method() != Method::POST {
            return (StatusCode::METHOD_NOT_ALLOWED, "POST only").into_response();
        }
        return if path == "/local/rooms" {
            proxy.local_rooms(request).await
        } else if path == "/local/repair/status" {
            proxy.repair_status().await
        } else {
            proxy.repair_start(request).await
        };
    }
    if decoded.starts_with("/live/") {
        return proxy.live(request).await;
    }
    proxy.static_file(&decoded).await
}

fn repair_error(status: StatusCode, error: impl std::fmt::Display) -> Response {
    (
        status,
        axum::Json(serde_json::json!({ "error": error.to_string(), "hop": "repair" })),
    )
        .into_response()
}

fn repair_command(
    exe: &std::path::Path,
    start: bool,
    service: bool,
) -> anyhow::Result<serde_json::Value> {
    use anyhow::Context;
    anyhow::ensure!(
        exe.is_file(),
        "Repair executable is missing: {}",
        exe.display()
    );
    let (stdout, stderr, exit_code) = if service {
        elevated_start(exe)?
    } else {
        let mut command = if exe
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ps1"))
        {
            let mut command = std::process::Command::new("powershell.exe");
            command
                .args(["-NoProfile", "-NonInteractive", "-File"])
                .arg(exe);
            command
        } else {
            std::process::Command::new(exe)
        };
        let output = command
            .arg(if start { "start" } else { "status" })
            .output()
            .with_context(|| format!("Run repair executable {}", exe.display()))?;
        (output.stdout, output.stderr, output.status.code())
    };
    let mut value: serde_json::Value = serde_json::from_slice(&stdout).with_context(|| {
        format!(
            "Repair executable printed no JSON object; stderr: {}",
            String::from_utf8_lossy(&stderr)
        )
    })?;
    let object = value.as_object_mut().with_context(|| {
        format!(
            "Repair executable printed no JSON object; stderr: {}",
            String::from_utf8_lossy(&stderr)
        )
    })?;
    if start {
        object.insert("exitCode".into(), serde_json::json!(exit_code));
    }
    Ok(value)
}

#[cfg(windows)]
fn elevated_start(exe: &std::path::Path) -> anyhow::Result<(Vec<u8>, Vec<u8>, Option<i32>)> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_FAILED},
        System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject},
        UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW},
    };
    let capture = tempfile::tempdir()?;
    let stdout = capture.path().join("stdout.json");
    let stderr = capture.path().join("stderr.txt");
    let script = capture.path().join("start.cmd");
    let batch_path = |path: &std::path::Path| path.to_string_lossy().replace('%', "%%");
    let invocation = if exe
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("ps1"))
    {
        format!(
            "powershell.exe -NoProfile -NonInteractive -File \"{}\"",
            batch_path(exe)
        )
    } else {
        format!("call \"{}\"", batch_path(exe))
    };
    std::fs::write(
        &script,
        format!(
            "@echo off\r\n{invocation} start --service 1>\"{}\" 2>\"{}\"\r\nexit /b %errorlevel%\r\n",
            batch_path(&stdout),
            batch_path(&stderr)
        ),
    )?;
    let wide = |value: &std::ffi::OsStr| value.encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    let verb = wide(std::ffi::OsStr::new("runas"));
    let shell = wide(
        &std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("C:/Windows"))
            .join("System32/cmd.exe")
            .into_os_string(),
    );
    let parameters = wide(std::ffi::OsStr::new(&format!(
        "/d /s /c \"\"{}\"\"",
        script.display()
    )));
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = shell.as_ptr();
    info.lpParameters = parameters.as_ptr();
    // ShellExecuteExW supplies the handle needed to wait through the UAC boundary.
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    anyhow::ensure!(
        !info.hProcess.is_null(),
        "Elevated repair returned no process handle"
    );
    let mut code = 0;
    let waited = unsafe { WaitForSingleObject(info.hProcess, INFINITE) };
    let result =
        if waited == WAIT_FAILED || unsafe { GetExitCodeProcess(info.hProcess, &mut code) } == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        };
    unsafe {
        CloseHandle(info.hProcess);
    }
    result?;
    Ok((
        std::fs::read(stdout)?,
        std::fs::read(stderr)?,
        Some(code as i32),
    ))
}

#[cfg(not(windows))]
fn elevated_start(_exe: &std::path::Path) -> anyhow::Result<(Vec<u8>, Vec<u8>, Option<i32>)> {
    anyhow::bail!("Administrator repair requires Windows")
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use tower::ServiceExt;

    #[tokio::test]
    async fn room_discovery_and_session_refusals_stay_local() {
        let router = Proxy::new(
            1,
            "test",
            vec!["test".into(), "other".into()],
            "must-not-leak".into(),
            None,
            PathBuf::from("must-not-expose.exe"),
            4175,
        )
        .unwrap()
        .router();
        for body in ["", "{}"] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/local/rooms")
                        .header("host", "127.0.0.1:4175")
                        .header("origin", "http://127.0.0.1:4175")
                        .body(axum::body::Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = axum::body::to_bytes(response.into_body(), 8192)
                .await
                .unwrap();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
                serde_json::json!({
                    "defaultRoom": "test",
                    "rooms": ["test", "other"],
                    "capabilities": { "repair": true }
                })
            );
        }
        for (path, body, room, status) in [
            (
                "/local/rooms",
                "{\"room\":\"other\"}",
                "test",
                StatusCode::BAD_REQUEST,
            ),
            ("/local/rooms", "[]", "test", StatusCode::BAD_REQUEST),
            ("/local/rooms", "null", "test", StatusCode::BAD_REQUEST),
            ("/local/rooms", "not JSON", "test", StatusCode::BAD_REQUEST),
            (
                "/live/session",
                "{\"action\":\"status\"}",
                "missing",
                StatusCode::FORBIDDEN,
            ),
            (
                "/live/session",
                "{\"action\":\"status\"}",
                "",
                StatusCode::FORBIDDEN,
            ),
            ("/live/health", "{}", "missing", StatusCode::FORBIDDEN),
            ("/live/session", "not JSON", "test", StatusCode::BAD_REQUEST),
            ("/live/session", "[]", "test", StatusCode::BAD_REQUEST),
            ("/live/session", "{}", "test", StatusCode::FORBIDDEN),
            (
                "/live/session",
                "{\"action\":\"status\",\"action\":\"detach\"}",
                "test",
                StatusCode::FORBIDDEN,
            ),
            (
                "/live/session",
                "{\"action\":\"attach\"}",
                "test",
                StatusCode::FORBIDDEN,
            ),
            (
                "/live/session",
                "{\"action\":\"detach\"}",
                "test",
                StatusCode::FORBIDDEN,
            ),
            (
                "/live/session",
                "{\"action\":\"context\"}",
                "test",
                StatusCode::FORBIDDEN,
            ),
            (
                "/live/session",
                "{\"action\":\"resourceList\"}",
                "test",
                StatusCode::FORBIDDEN,
            ),
            (
                "/live/session",
                "{\"action\":\"resourceRead\"}",
                "test",
                StatusCode::FORBIDDEN,
            ),
            (
                "/live/session",
                "{\"action\":\"resourceWrite\"}",
                "test",
                StatusCode::FORBIDDEN,
            ),
            (
                "/live/session",
                "{\"action\":\"heartbeat\"}",
                "test",
                StatusCode::FORBIDDEN,
            ),
            (
                "/live/session",
                "{\"action\":\"poll\"}",
                "test",
                StatusCode::FORBIDDEN,
            ),
            (
                "/live/session",
                "{\"action\":\"acknowledge\"}",
                "test",
                StatusCode::FORBIDDEN,
            ),
            (
                "/live/session",
                "{\"action\":\"chatDelivery\"}",
                "test",
                StatusCode::FORBIDDEN,
            ),
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(path)
                        .header("host", "127.0.0.1:4175")
                        .header("origin", "http://127.0.0.1:4175")
                        .header("x-athanor-room", room)
                        .body(axum::body::Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), status, "{path} {body} {room}");
        }
    }

    #[tokio::test]
    async fn foreign_requests_cannot_reach_repair_or_host() {
        let router = Proxy::new(
            1,
            "test",
            vec!["test".into()],
            String::new(),
            None,
            PathBuf::from("must-not-run.exe"),
            4175,
        )
        .unwrap()
        .router();
        for path in [
            "/live/chat/say",
            "/live/session",
            "/local/rooms",
            "/local/repair/start",
            "/",
        ] {
            for (host, origin) in [
                ("evil.example", Some("http://evil.example")),
                ("127.0.0.1:4175", Some("https://evil.example")),
                ("127.0.0.1:4175", Some("null")),
                ("127.0.0.1:4175", Some("http://127.0.0.1:4176")),
                ("127.0.0.1:4175", None),
            ] {
                let mut request = Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("host", host);
                if let Some(origin) = origin {
                    request = request.header("origin", origin);
                }
                let response = router
                    .clone()
                    .oneshot(request.body(axum::body::Body::empty()).unwrap())
                    .await
                    .unwrap();
                assert_eq!(
                    response.status(),
                    StatusCode::FORBIDDEN,
                    "{path} {host} {origin:?}"
                );
            }
        }
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("host", "127.0.0.1:4175")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn local_repair_runs_contract_command_without_host() {
        let fixture = tempfile::tempdir().unwrap();
        let stub = fixture.path().join("stub-athanor.cmd");
        std::fs::write(&stub, concat!(
            "@echo off\r\nif \"%~1\"==\"start\" goto start\r\n",
            "echo {\"ok\":false,\"components\":[{\"name\":\"service\",\"installed\":true,\"running\":false,\"reachable\":null,\"healthy\":false,\"detail\":\"stopped\"}],\"missing\":[\"service\"],\"elevationRequired\":[\"service\"]}\r\nexit /b 0\r\n",
            ":start\r\necho {\"ok\":false,\"started\":[\"host\"],\"skipped\":[\"nats\"],\"refused\":[{\"component\":\"service\",\"reason\":\"needs_elevation\"}]}\r\nexit /b 3\r\n"
        )).unwrap();
        let exe = std::env::var_os("ATHANOR_APP_EXE")
            .map(PathBuf::from)
            .unwrap_or(stub);
        let router = Proxy::new(
            1,
            "test",
            vec!["test".into()],
            String::new(),
            None,
            exe,
            4175,
        )
        .unwrap()
        .router();
        for (path, body, expected) in [
            (
                "/local/repair/status",
                "{}",
                serde_json::json!({"ok":false,"components":[{"name":"service","installed":true,"running":false,"reachable":null,"healthy":false,"detail":"stopped"}],"missing":["service"],"elevationRequired":["service"]}),
            ),
            (
                "/local/repair/start",
                "{\"service\":false}",
                serde_json::json!({"ok":false,"started":["host"],"skipped":["nats"],"refused":[{"component":"service","reason":"needs_elevation"}],"exitCode":3}),
            ),
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(path)
                        .header("host", "127.0.0.1:4175")
                        .header("origin", "http://127.0.0.1:4175")
                        .body(axum::body::Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = axum::body::to_bytes(response.into_body(), 8192)
                .await
                .unwrap();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
                expected
            );
        }
        for (path, method, status) in [
            ("/live/local/repair/start", "POST", StatusCode::NOT_FOUND),
            (
                "/local/repair/status",
                "GET",
                StatusCode::METHOD_NOT_ALLOWED,
            ),
            ("/local/unknown", "POST", StatusCode::NOT_FOUND),
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .header("host", "127.0.0.1:4175")
                        .header("origin", "http://127.0.0.1:4175")
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), status);
        }
        let missing = Proxy::new(
            1,
            "test",
            vec!["test".into()],
            String::new(),
            None,
            PathBuf::from("missing-repair.exe"),
            4175,
        )
        .unwrap()
        .router();
        let response = missing
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/local/repair/status")
                    .header("host", "127.0.0.1:4175")
                    .header("origin", "http://127.0.0.1:4175")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        let bytes = axum::body::to_bytes(response.into_body(), 8192)
            .await
            .unwrap();
        let error: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(error["hop"], "repair");
        assert!(
            error["error"]
                .as_str()
                .unwrap()
                .contains("missing-repair.exe")
        );
        let broken_exe = fixture.path().join("broken.cmd");
        std::fs::write(
            &broken_exe,
            "@echo off\r\necho diagnostic from stub 1>&2\r\nexit /b 1\r\n",
        )
        .unwrap();
        let broken = Proxy::new(
            1,
            "test",
            vec!["test".into()],
            String::new(),
            None,
            broken_exe,
            4175,
        )
        .unwrap()
        .router();
        let response = broken
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/local/repair/status")
                    .header("host", "127.0.0.1:4175")
                    .header("origin", "http://127.0.0.1:4175")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        let bytes = axum::body::to_bytes(response.into_body(), 8192)
            .await
            .unwrap();
        let error: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(error["hop"], "repair");
        assert!(
            error["error"]
                .as_str()
                .unwrap()
                .contains("diagnostic from stub")
        );
    }
}
