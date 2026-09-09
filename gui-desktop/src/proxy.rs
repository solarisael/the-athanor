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
    app_exe: PathBuf,
}

impl Proxy {
    pub fn new(host_port: u16, room: &str, token: String, dev_dir: Option<PathBuf>, app_exe: PathBuf) -> anyhow::Result<Self> {
        let room = percent_encoding::utf8_percent_encode(room, percent_encoding::NON_ALPHANUMERIC);
        Ok(Self {
            routes: serde_json::from_str(include_str!("../../gui-prototype/live-routes.json"))?,
            client: reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).timeout(std::time::Duration::from_secs(20)).build()?,
            base: format!("http://127.0.0.1:{host_port}/room/{room}"),
            token,
            dev_dir: dev_dir.map(std::fs::canonicalize).transpose()?,
            app_exe,
        })
    }

    pub fn router(self) -> Router {
        Router::new().fallback(serve).with_state(Arc::new(self))
    }

    async fn repair_status(&self) -> Response {
        self.run_repair(false, false).await
    }

    async fn repair_start(&self, request: Request) -> Response {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Start { service: bool }
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
        // The Host answering an error passes through with its own status; only a
        // transport failure becomes 502 here, and the body names which hop failed.
        match self.forward(route, request).await {
            Ok(response) => response,
            Err(error) => {
                let hop = match error.downcast_ref::<reqwest::Error>() {
                    Some(inner) if inner.is_timeout() => "host_timeout",
                    Some(inner) if inner.is_connect() => "host_unreachable",
                    Some(_) => "host_transport",
                    None => "proxy",
                };
                (StatusCode::BAD_GATEWAY, axum::Json(serde_json::json!({
                    "error": format!("Host request failed: {error}"),
                    "hop": hop
                }))).into_response()
            }
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
    if decoded.starts_with("/local/") {
        let path = decoded.into_owned();
        if path != "/local/repair/status" && path != "/local/repair/start" {
            return (StatusCode::NOT_FOUND, "unknown local route").into_response();
        }
        if request.method() != Method::POST {
            return (StatusCode::METHOD_NOT_ALLOWED, "POST only").into_response();
        }
        return if path == "/local/repair/status" {
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
    (status, axum::Json(serde_json::json!({ "error": error.to_string(), "hop": "repair" }))).into_response()
}

fn repair_command(exe: &std::path::Path, start: bool, service: bool) -> anyhow::Result<serde_json::Value> {
    use anyhow::Context;
    anyhow::ensure!(exe.is_file(), "Repair executable is missing: {}", exe.display());
    let (stdout, stderr, exit_code) = if service {
        elevated_start(exe)?
    } else {
        let mut command = if exe.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("ps1")) {
            let mut command = std::process::Command::new("powershell.exe");
            command.args(["-NoProfile", "-NonInteractive", "-File"]).arg(exe);
            command
        } else {
            std::process::Command::new(exe)
        };
        let output = command.arg(if start { "start" } else { "status" }).output()
            .with_context(|| format!("Run repair executable {}", exe.display()))?;
        (output.stdout, output.stderr, output.status.code())
    };
    let mut value: serde_json::Value = serde_json::from_slice(&stdout)
        .with_context(|| format!("Repair executable printed no JSON object; stderr: {}", String::from_utf8_lossy(&stderr)))?;
    let object = value.as_object_mut().with_context(|| format!("Repair executable printed no JSON object; stderr: {}", String::from_utf8_lossy(&stderr)))?;
    if start { object.insert("exitCode".into(), serde_json::json!(exit_code)); }
    Ok(value)
}

#[cfg(windows)]
fn elevated_start(exe: &std::path::Path) -> anyhow::Result<(Vec<u8>, Vec<u8>, Option<i32>)> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_FAILED},
        System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE},
        UI::Shell::{ShellExecuteExW, SHELLEXECUTEINFOW, SEE_MASK_NOCLOSEPROCESS},
    };
    let capture = tempfile::tempdir()?;
    let stdout = capture.path().join("stdout.json");
    let stderr = capture.path().join("stderr.txt");
    let script = capture.path().join("start.cmd");
    let batch_path = |path: &std::path::Path| path.to_string_lossy().replace('%', "%%");
    let invocation = if exe.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("ps1")) {
        format!("powershell.exe -NoProfile -NonInteractive -File \"{}\"", batch_path(exe))
    } else {
        format!("call \"{}\"", batch_path(exe))
    };
    std::fs::write(&script, format!("@echo off\r\n{invocation} start --service 1>\"{}\" 2>\"{}\"\r\nexit /b %errorlevel%\r\n", batch_path(&stdout), batch_path(&stderr)))?;
    let wide = |value: &std::ffi::OsStr| value.encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    let verb = wide(std::ffi::OsStr::new("runas"));
    let shell = wide(&std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:/Windows")).join("System32/cmd.exe").into_os_string());
    let parameters = wide(std::ffi::OsStr::new(&format!("/d /s /c \"\"{}\"\"", script.display())));
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = shell.as_ptr();
    info.lpParameters = parameters.as_ptr();
    // ShellExecuteExW supplies the handle needed to wait through the UAC boundary.
    if unsafe { ShellExecuteExW(&mut info) } == 0 { return Err(std::io::Error::last_os_error().into()); }
    anyhow::ensure!(!info.hProcess.is_null(), "Elevated repair returned no process handle");
    let mut code = 0;
    let waited = unsafe { WaitForSingleObject(info.hProcess, INFINITE) };
    let result = if waited == WAIT_FAILED || unsafe { GetExitCodeProcess(info.hProcess, &mut code) } == 0 {
        Err(std::io::Error::last_os_error())
    } else { Ok(()) };
    unsafe { CloseHandle(info.hProcess); }
    result?;
    Ok((std::fs::read(stdout)?, std::fs::read(stderr)?, Some(code as i32)))
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
    async fn local_repair_runs_contract_command_without_host() {
        let fixture = tempfile::tempdir().unwrap();
        let stub = fixture.path().join("stub-athanor.cmd");
        std::fs::write(&stub, concat!(
            "@echo off\r\nif \"%~1\"==\"start\" goto start\r\n",
            "echo {\"ok\":false,\"components\":[{\"name\":\"service\",\"installed\":true,\"running\":false,\"reachable\":null,\"healthy\":false,\"detail\":\"stopped\"}],\"missing\":[\"service\"],\"elevationRequired\":[\"service\"]}\r\nexit /b 0\r\n",
            ":start\r\necho {\"ok\":false,\"started\":[\"host\"],\"skipped\":[\"nats\"],\"refused\":[{\"component\":\"service\",\"reason\":\"needs_elevation\"}]}\r\nexit /b 3\r\n"
        )).unwrap();
        let exe = std::env::var_os("ATHANOR_APP_EXE").map(PathBuf::from).unwrap_or(stub);
        let router = Proxy::new(1, "test", String::new(), None, exe).unwrap().router();
        for (path, body, expected) in [
            ("/local/repair/status", "{}", serde_json::json!({"ok":false,"components":[{"name":"service","installed":true,"running":false,"reachable":null,"healthy":false,"detail":"stopped"}],"missing":["service"],"elevationRequired":["service"]})),
            ("/local/repair/start", "{\"service\":false}", serde_json::json!({"ok":false,"started":["host"],"skipped":["nats"],"refused":[{"component":"service","reason":"needs_elevation"}],"exitCode":3})),
        ] {
            let response = router.clone().oneshot(Request::builder().method("POST").uri(path)
                .body(axum::body::Body::from(body)).unwrap()).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
            assert_eq!(serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(), expected);
        }
        for (path, method, status) in [
            ("/live/local/repair/start", "POST", StatusCode::NOT_FOUND),
            ("/local/repair/status", "GET", StatusCode::METHOD_NOT_ALLOWED),
            ("/local/unknown", "POST", StatusCode::NOT_FOUND),
        ] {
            let response = router.clone().oneshot(Request::builder().method(method).uri(path)
                .body(axum::body::Body::empty()).unwrap()).await.unwrap();
            assert_eq!(response.status(), status);
        }
        let missing = Proxy::new(1, "test", String::new(), None, PathBuf::from("missing-repair.exe")).unwrap().router();
        let response = missing.oneshot(Request::builder().method("POST").uri("/local/repair/status")
            .body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        let bytes = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
        let error: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(error["hop"], "repair");
        assert!(error["error"].as_str().unwrap().contains("missing-repair.exe"));
        let broken_exe = fixture.path().join("broken.cmd");
        std::fs::write(&broken_exe, "@echo off\r\necho diagnostic from stub 1>&2\r\nexit /b 1\r\n").unwrap();
        let broken = Proxy::new(1, "test", String::new(), None, broken_exe).unwrap().router();
        let response = broken.oneshot(Request::builder().method("POST").uri("/local/repair/status")
            .body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        let bytes = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
        let error: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(error["hop"], "repair");
        assert!(error["error"].as_str().unwrap().contains("diagnostic from stub"));
    }
}
