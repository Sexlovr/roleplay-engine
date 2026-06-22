//! AI Hub — a Janitor-AI-style character chat frontend in Rust (Leptos CSR).
//!
//! Navigation is a single `RwSignal<Page>` provided via context (no router).
//! Global UI state (search query, NSFW toggle) is likewise context-provided so
//! the header and pages stay in sync.

mod types;
mod data;
mod api;
mod header;
mod home;
mod character;
mod chat;
mod settings;

use leptos::prelude::*;

use api::ProxyConfig;
use types::Page;
use header::Header;
use home::Home;
use character::CharacterPage;
use chat::Chat;

/// Newtype wrappers so contexts of the same primitive type don't collide.
#[derive(Copy, Clone)]
pub struct SearchQuery(pub RwSignal<String>);
#[derive(Copy, Clone)]
pub struct NsfwEnabled(pub RwSignal<bool>);
/// The active chat connector config (persisted to localStorage).
#[derive(Copy, Clone)]
pub struct ApiConfig(pub RwSignal<ProxyConfig>);
/// Whether the API Settings drawer is open.
#[derive(Copy, Clone)]
pub struct SettingsOpen(pub RwSignal<bool>);

#[component]
fn App() -> impl IntoView {
    let page = RwSignal::new(Page::Home);
    provide_context(page);
    provide_context(SearchQuery(RwSignal::new(String::new())));
    provide_context(NsfwEnabled(RwSignal::new(false)));

    // Load any saved connector config from localStorage, else a default.
    let cfg = api::load().unwrap_or_default();
    let settings_open = RwSignal::new(false);
    provide_context(ApiConfig(RwSignal::new(cfg)));
    provide_context(SettingsOpen(settings_open));

    view! {
        <Header/>
        <main class="content">
            {move || match page.get() {
                Page::Home => view! { <Home/> }.into_any(),
                Page::Character(id) => view! { <CharacterPage id=id/> }.into_any(),
                Page::Chat(id) => view! { <Chat id=id/> }.into_any(),
            }}
        </main>
        {move || settings_open.get().then(|| view! { <settings::Settings/> })}
    }
}

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}
