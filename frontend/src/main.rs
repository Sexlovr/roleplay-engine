//! Roleplay Engine — a JanitorAI-style character-chat frontend (Leptos CSR).
//!
//! All data lives in the backend (SQLite via `/api/*`); this crate is a thin
//! reactive client. Navigation is a single `RwSignal<Page>` in context (no
//! router). Settings + personas are loaded from `/api/settings` once at startup
//! and the *active* proxy + persona are held in context so the header, chat,
//! and drawers stay in sync.

mod api;
mod character;
mod chat;
mod chats;
mod create;
mod header;
mod home;
mod markdown;
mod persona;
mod settings;
mod upload;
mod util;

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use shared::template::ProxyConfig;
use shared::types::Persona;

use character::CharacterPage;
use chat::Chat;
use chats::Chats;
use create::Create;
use header::Sidebar;
use home::Home;

/// Which screen is currently shown. Stored as `RwSignal<Page>` in context;
/// any component can navigate by calling `page.set(...)`. IDs are `i64` to
/// match the SQLite primary keys the backend hands out.
#[derive(Clone, Debug, PartialEq)]
pub enum Page {
    Home,
    Chats,          // global recent-conversations tab
    Character(i64), // character id — detail page
    Chat(i64),      // chat id (a started conversation)
    Create,
    Edit(i64), // edit an existing character
}

/// Newtype wrappers so contexts of the same primitive type don't collide.
#[derive(Copy, Clone)]
pub struct SearchQuery(pub RwSignal<String>);
#[derive(Copy, Clone)]
pub struct NsfwEnabled(pub RwSignal<bool>);
/// The *active* proxy config (mirrors the server; `api_key` is blank here — the
/// real key lives only in the DB). Drives the header label; the Settings drawer
/// owns the full multi-config store.
#[derive(Copy, Clone)]
pub struct ApiConfig(pub RwSignal<ProxyConfig>);
#[derive(Copy, Clone)]
pub struct HasApiKey(pub RwSignal<bool>);
/// Whether the API Settings drawer is open.
#[derive(Copy, Clone)]
pub struct SettingsOpen(pub RwSignal<bool>);
/// The user's *active* persona (mirrors the server).
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

    let cfg = RwSignal::new(ProxyConfig::default());
    let has_key = RwSignal::new(false);
    let persona = RwSignal::new(Persona::default());
    provide_context(ApiConfig(cfg));
    provide_context(HasApiKey(has_key));
    provide_context(SettingsOpen(RwSignal::new(false)));
    provide_context(PersonaCtx(persona));
    provide_context(PersonaOpen(RwSignal::new(false)));

    // Load settings from the server once at startup; surface the active proxy +
    // persona into context.
    spawn_local(async move {
        if let Ok(s) = api::get_settings().await {
            if let Some(active) = s.proxy.active_config() {
                cfg.set(active.clone());
                has_key.set(s.proxy_has_key.contains(&active.id));
            }
            if let Some(p) = s
                .personas
                .personas
                .iter()
                .find(|p| p.id == s.personas.active)
                .or_else(|| s.personas.personas.first())
            {
                persona.set(p.clone());
            }
        }
    });

    let settings_open = use_context::<SettingsOpen>().unwrap().0;
    let persona_open = use_context::<PersonaOpen>().unwrap().0;

    view! {
        <div class="shell">
            <Sidebar/>
            <main class="shell__main">
                {move || match page.get() {
                    Page::Home => view! { <div class="content"><Home/></div> }.into_any(),
                    Page::Chats => view! { <div class="content"><Chats/></div> }.into_any(),
                    Page::Character(id) => view! { <div class="content"><CharacterPage id=id/></div> }.into_any(),
                    // The chat view owns the full viewport height (its own internal
                    // scroll); skip the padded `.content` wrapper.
                    Page::Chat(id) => view! { <div class="content content--chat"><Chat id=id/></div> }.into_any(),
                    Page::Create => view! { <div class="content"><Create edit_id=None/></div> }.into_any(),
                    Page::Edit(id) => view! { <div class="content"><Create edit_id=Some(id)/></div> }.into_any(),
                }}
            </main>
        </div>
        {move || settings_open.get().then(|| view! { <settings::Settings/> })}
        {move || persona_open.get().then(|| view! { <persona::PersonaEditor/> })}
    }
}

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}
