mod health;

use leptos::prelude::*;

use health::{Health, HealthView};

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Draft,
    Health,
}

fn main() {
    leptos::mount::mount_to_body(App);
}

#[component]
fn App() -> impl IntoView {
    // The root owns every piece of state, so switching views unmounts only
    // markup: the draft and the last health answer survive the remount.
    let current = RwSignal::new(View::Draft);
    let draft = RwSignal::new(String::new());
    let health = RwSignal::new(Health::Idle);

    view! {
        <header class="masthead">
            <h1>"Athanor · Leptos trial"</h1>
            <p class="notice">
                "Isolated trial. No durable writes: the draft lives only in this tab, "
                "and Host health is a read."
            </p>
        </header>

        <nav class="views" aria-label="Views">
            <ViewButton current target=View::Draft label="Draft" />
            <ViewButton current target=View::Health label="Host health" />
        </nav>

        <main class="panel">
            {move || match current.get() {
                View::Draft => view! { <DraftView draft /> }.into_any(),
                View::Health => view! { <HealthView health /> }.into_any(),
            }}
        </main>
    }
}

#[component]
fn ViewButton(current: RwSignal<View>, target: View, label: &'static str) -> impl IntoView {
    view! {
        <button
            type="button"
            aria-pressed=move || (current.get() == target).to_string()
            on:click=move |_| current.set(target)
        >
            {label}
        </button>
    }
}

#[component]
fn DraftView(draft: RwSignal<String>) -> impl IntoView {
    view! {
        <section aria-labelledby="draft-title">
            <h2 id="draft-title">"Draft"</h2>
            <label for="draft-text">"Draft text"</label>
            <textarea
                id="draft-text"
                rows="8"
                prop:value=move || draft.get()
                on:input:target=move |ev| draft.set(ev.target().value())
            ></textarea>
            <p class="meta">
                {move || format!("{} characters", draft.with(|text| text.chars().count()))}
            </p>
        </section>
    }
}
