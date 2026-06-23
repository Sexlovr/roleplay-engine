//! Sticky top navigation bar: logo (home nav), global search box, NSFW toggle,
//! Create button, and model/persona access buttons. No "Log in" button — the
//! app has no auth.

use leptos::prelude::*;

use crate::Page;

#[component]
pub fn Header() -> impl IntoView {
    let page = use_context::<RwSignal<Page>>().unwrap();
    let search = use_context::<crate::SearchQuery>().unwrap().0;
    let nsfw = use_context::<crate::NsfwEnabled>().unwrap().0;
    let settings_open = use_context::<crate::SettingsOpen>().unwrap().0;
    let cfg_sig = use_context::<crate::ApiConfig>().unwrap().0;
    let persona_open = use_context::<crate::PersonaOpen>().unwrap().0;

    let model_label = move || {
        let c = cfg_sig.get();
        if c.url.trim().is_empty() {
            "\u{2699} API".to_string()
        } else {
            format!("\u{2699} {}", c.name)
        }
    };

    view! {
        <header class="header">
            <div class="header__inner">
                <div class="header__left">
                    <div
                        class="header__logo"
                        on:click=move |_| { search.set(String::new()); page.set(Page::Home) }
                    >
                        <span class="header__logo-accent">"Roleplay"</span>
                        " Engine"
                    </div>
                </div>

                // Search is only meaningful on the gallery; hide it elsewhere.
                {move || (page.get() == Page::Home).then(|| view! {
                    <input
                        class="header__search"
                        r#type="text"
                        placeholder="Search characters..."
                        aria-label="Search characters"
                        prop:value=move || search.get()
                        on:input=move |ev| search.set(event_target_value(&ev))
                    />
                })}

                <div class="header__actions">
                    <button class="header__persona" on:click=move |_| persona_open.set(true)>
                        "\u{1F464}"
                    </button>
                    <button class="header__settings" on:click=move |_| settings_open.set(true)>
                        {model_label}
                    </button>
                    <button
                        class="nsfw-toggle"
                        class=("nsfw-toggle--on", move || nsfw.get())
                        on:click=move |_| nsfw.update(|v| *v = !*v)
                    >
                        "NSFW"
                    </button>
                    <button class="btn header__create" on:click=move |_| page.set(Page::Create)>
                        "+ Create"
                    </button>
                </div>
            </div>
        </header>
    }
}
