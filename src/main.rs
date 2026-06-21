//! AI Hub — a Janitor-AI-style character chat frontend in Rust (Leptos CSR).
//!
//! Navigation is a single `RwSignal<Page>` provided via context (no router).
//! Global UI state (search query, NSFW toggle) is likewise context-provided so
//! the header and pages stay in sync.

mod types;
mod data;
mod header;
mod home;
mod chat;

use leptos::prelude::*;

use types::Page;
use header::Header;
use home::Home;
use chat::Chat;

/// Newtype wrappers so contexts of the same primitive type don't collide.
#[derive(Copy, Clone)]
pub struct SearchQuery(pub RwSignal<String>);
#[derive(Copy, Clone)]
pub struct NsfwEnabled(pub RwSignal<bool>);

#[component]
fn App() -> impl IntoView {
    let page = RwSignal::new(Page::Home);
    provide_context(page);
    provide_context(SearchQuery(RwSignal::new(String::new())));
    provide_context(NsfwEnabled(RwSignal::new(false)));

    view! {
        <Header/>
        <main class="content">
            {move || match page.get() {
                Page::Home => view! { <Home/> }.into_any(),
                Page::Chat(id) => view! { <Chat id=id/> }.into_any(),
            }}
        </main>
    }
}

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}
