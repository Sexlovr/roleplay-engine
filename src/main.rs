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
mod persona;
mod create;

use leptos::prelude::*;

use api::ProxyConfig;
use types::{Page, Persona};
use header::Header;
use home::Home;
use character::CharacterPage;
use chat::Chat;
use create::Create;

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
/// The user's persona (persisted to localStorage).
#[derive(Copy, Clone)]
pub struct PersonaCtx(pub RwSignal<Persona>);
/// Whether the persona editor drawer is open.
#[derive(Copy, Clone)]
pub struct PersonaOpen(pub RwSignal<bool>);

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

    // Persona (who the user is in the roleplay).
    let persona_open = RwSignal::new(false);
    provide_context(PersonaCtx(RwSignal::new(persona::load())));
    provide_context(PersonaOpen(persona_open));

    view! {
        <Header/>
        <main class="content">
            {move || match page.get() {
                Page::Home => view! { <Home/> }.into_any(),
                Page::Character(id) => view! { <CharacterPage id=id/> }.into_any(),
                Page::Chat(id) => view! { <Chat id=id/> }.into_any(),
                Page::Create => view! { <Create/> }.into_any(),
            }}
        </main>
        {move || settings_open.get().then(|| view! { <settings::Settings/> })}
        {move || persona_open.get().then(|| view! { <persona::PersonaEditor/> })}
    }
}

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}
