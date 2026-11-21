use gloo_net::http::Request;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsValue;

// serve.ts proxies this one path to the Host's GET /health; the page always POSTs.
const HEALTH_ROUTE: &str = "/live/health";

#[derive(Clone)]
pub enum Health {
    Idle,
    Pending,
    Answered {
        status: u16,
        status_text: String,
        body: String,
    },
    Failed(String),
}

#[component]
pub fn HealthView(health: RwSignal<Health>) -> impl IntoView {
    let pending = move || health.with(|state| matches!(state, Health::Pending));

    view! {
        <section aria-labelledby="health-title">
            <h2 id="health-title">"Host health"</h2>
            <p>"Source: " <code>{HEALTH_ROUTE}</code></p>

            // aria-disabled keeps focus on the button while a request is in flight;
            // refresh() is what actually refuses the duplicate.
            <button
                type="button"
                class="refresh"
                aria-disabled=move || pending().to_string()
                on:click=move |_| refresh(health)
            >
                {move || if pending() { "Waiting for the Host…" } else { "Refresh" }}
            </button>

            <div class="result" role="status" aria-live="polite">
                {move || match health.get() {
                    Health::Idle => view! { <p>"No request yet."</p> }.into_any(),
                    Health::Pending => view! { <p>"Request pending…"</p> }.into_any(),
                    Health::Answered { status, status_text, body } => {
                        answered(status, status_text, body)
                    }
                    Health::Failed(message) => {
                        view! { <p class="error">"Error: request failed. " {message}</p> }.into_any()
                    }
                }}
            </div>
        </section>
    }
}

fn answered(status: u16, status_text: String, body: String) -> AnyView {
    let outcome = if (200..300).contains(&status) {
        view! { <p class="outcome">{format!("HTTP {status} {status_text}")}</p> }.into_any()
    } else {
        view! { <p class="outcome error">{format!("Error: HTTP {status} {status_text}")}</p> }
            .into_any()
    };

    // Rendered as a text node, never as markup, so the Host's bytes stay escaped.
    let shown = if body.is_empty() {
        "Empty body.".to_owned()
    } else {
        pretty_json(&body).unwrap_or(body)
    };

    view! {
        {outcome}
        <pre class="body">{shown}</pre>
    }
    .into_any()
}

fn refresh(health: RwSignal<Health>) {
    if health.with_untracked(|state| matches!(state, Health::Pending)) {
        return;
    }

    health.set(Health::Pending);
    spawn_local(async move { health.set(request_health().await) });
}

async fn request_health() -> Health {
    let request = match Request::post(HEALTH_ROUTE)
        .header("content-type", "application/json")
        .body("{}")
    {
        Ok(request) => request,
        Err(error) => return Health::Failed(error.to_string()),
    };

    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => return Health::Failed(error.to_string()),
    };

    let status = response.status();
    let status_text = response.status_text();
    match response.text().await {
        Ok(body) => Health::Answered {
            status,
            status_text,
            body,
        },
        Err(error) => Health::Failed(format!(
            "HTTP {status} arrived but its body was unreadable: {error}"
        )),
    }
}

/// The browser's own JSON parser decides; anything it refuses is shown as received.
fn pretty_json(body: &str) -> Option<String> {
    let value = js_sys::JSON::parse(body).ok()?;
    let text =
        js_sys::JSON::stringify_with_replacer_and_space(&value, &JsValue::NULL, &JsValue::from(2))
            .ok()?;
    Some(text.into())
}
