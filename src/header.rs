//! Sticky top navigation bar: logo (home nav), global search box, NSFW toggle,
//! and a decorative login button.

use leptos::prelude::*;

use crate::types::Page;

#[component]
pub fn Header() -> impl IntoView {
    let page = use_context::<RwSignal<Page>>().unwrap();
    let search = use_context::<crate::SearchQuery>().unwrap().0;
    let nsfw = use_context::<crate::NsfwEnabled>().unwrap().0;

    view! {
        <header class="header">
            <div class="header__inner">
                <div
                    class="header__logo"
                    on:click=move |_| page.set(Page::Home)
                >
                    <span class="header__logo-accent">"Roleplay"</span>
                    " Engine"
                </div>

                <input
                    class="header__search"
                    r#type="text"
                    placeholder="Search characters..."
                    prop:value=move || search.get()
                    on:input=move |ev| search.set(event_target_value(&ev))
                />

                <div class="header__actions">
                    <button class="btn header__create" on:click=move |_| page.set(Page::Create)>
                        "+ Create"
                    </button>
                    <button
                        class="nsfw-toggle"
                        class=("nsfw-toggle--on", move || nsfw.get())
                        on:click=move |_| nsfw.update(|v| *v = !*v)
                    >
                        "NSFW"
                    </button>
                    <button class="btn btn--login">"Log in"</button>
                </div>
            </div>
        </header>
    }
}
