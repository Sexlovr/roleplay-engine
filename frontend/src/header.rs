//! Left sidebar app shell: brand, primary navigation (Discover / Chats /
//! Create), and a footer with the active persona, the model/API connection,
//! and the NSFW toggle. This replaces the old top header — one organized
//! navigation rail instead of controls scattered across a bar.

use leptos::prelude::*;

use crate::Page;

#[component]
pub fn Sidebar() -> impl IntoView {
    let page = use_context::<RwSignal<Page>>().unwrap();
    let nsfw = use_context::<crate::NsfwEnabled>().unwrap().0;
    let settings_open = use_context::<crate::SettingsOpen>().unwrap().0;
    let persona_open = use_context::<crate::PersonaOpen>().unwrap().0;
    let cfg_sig = use_context::<crate::ApiConfig>().unwrap().0;
    let persona_sig = use_context::<crate::PersonaCtx>().unwrap().0;

    let model_label = move || {
        let c = cfg_sig.get();
        if c.url.trim().is_empty() {
            "Connect API".to_string()
        } else {
            c.name.clone()
        }
    };
    let persona_label = move || {
        let p = persona_sig.get();
        if p.name.trim().is_empty() {
            "Persona".to_string()
        } else {
            p.name.clone()
        }
    };

    view! {
        <aside class="sidebar">
            <div class="sidebar__brand" on:click=move |_| page.set(Page::Home)>
                <span class="sidebar__mark">"\u{25C6}"</span>
                <span class="sidebar__brandtext">
                    <span class="header__logo-accent">"Roleplay"</span>" Engine"
                </span>
            </div>

            <nav class="sidebar__nav">
                <button class="navitem" class=("navitem--active", move || matches!(page.get(), Page::Home))
                    on:click=move |_| page.set(Page::Home)>
                    <span class="navitem__icon">"\u{1F3E0}"</span>
                    <span class="navitem__label">"Discover"</span>
                </button>
                <button class="navitem" class=("navitem--active", move || matches!(page.get(), Page::Chats))
                    on:click=move |_| page.set(Page::Chats)>
                    <span class="navitem__icon">"\u{1F4AC}"</span>
                    <span class="navitem__label">"Chats"</span>
                </button>
                <button class="navitem" class=("navitem--active", move || matches!(page.get(), Page::Create | Page::Edit(_)))
                    on:click=move |_| page.set(Page::Create)>
                    <span class="navitem__icon">"\u{2795}"</span>
                    <span class="navitem__label">"Create"</span>
                </button>
            </nav>

            <div class="sidebar__foot">
                <button class="sidebar__chip" title="Personas" on:click=move |_| persona_open.set(true)>
                    <span class="navitem__icon">"\u{1F464}"</span>
                    <span class="sidebar__chiptext">{persona_label}</span>
                </button>
                <button class="sidebar__chip" title="Model / API connection" on:click=move |_| settings_open.set(true)>
                    <span class="navitem__icon">"\u{2699}"</span>
                    <span class="sidebar__chiptext">{model_label}</span>
                </button>
                <button class="nsfw-toggle" class=("nsfw-toggle--on", move || nsfw.get())
                    on:click=move |_| nsfw.update(|v| *v = !*v)>
                    "NSFW"
                </button>
            </div>
        </aside>
    }
}
