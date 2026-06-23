//! Create-a-Character page: a form with a live card preview. Characters are
//! persisted on the server (SQLite) and appear on the home grid immediately.

use leptos::prelude::*;
use shared::dto::NewCharacterReq;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::Page;

const FALLBACK_AVATAR: &str = "https://picsum.photos/seed/new-character/400/400";

fn parse_tags(s: &str) -> Vec<String> {
    s.split(',')
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect()
}

#[component]
pub fn Create() -> impl IntoView {
    let page = use_context::<RwSignal<Page>>().unwrap();

    let name = RwSignal::new(String::new());
    let tagline = RwSignal::new(String::new());
    let avatar = RwSignal::new(String::new());
    let creator = RwSignal::new(String::new());
    let tags_text = RwSignal::new(String::new());
    let personality = RwSignal::new(String::new());
    let first_message = RwSignal::new(String::new());
    let scenario = RwSignal::new(String::new());
    let nsfw = RwSignal::new(false);

    let error = RwSignal::new(String::new());
    let submitting = RwSignal::new(false);

    // Reactive preview values.
    let prev_name = move || {
        let n = name.get();
        if n.trim().is_empty() { "Unnamed".to_string() } else { n }
    };
    let prev_avatar = move || {
        let a = avatar.get();
        if a.trim().is_empty() { FALLBACK_AVATAR.to_string() } else { a }
    };
    let prev_tagline = move || {
        let t = tagline.get();
        if t.trim().is_empty() { "A one-line hook for your character.".to_string() } else { t }
    };
    let prev_creator = move || {
        let c = creator.get();
        if c.trim().is_empty() { "@you".to_string() } else { c }
    };
    let prev_tags = move || parse_tags(&tags_text.get());

    let can_create = move || !name.get().trim().is_empty() && !submitting.get();

    let submit = move |_| {
        if name.get().trim().is_empty() || submitting.get_untracked() {
            return;
        }
        error.set(String::new());
        submitting.set(true);

        let req = NewCharacterReq {
            name: name.get().trim().to_string(),
            tagline: Some(tagline.get()).filter(|s| !s.is_empty()),
            description: None,
            personality: Some(personality.get()).filter(|s| !s.is_empty()),
            scenario: Some(scenario.get()).filter(|s| !s.is_empty()),
            first_message: Some(first_message.get()).filter(|s| !s.is_empty()),
            avatar: {
                let a = avatar.get();
                if a.trim().is_empty() { None } else { Some(a) }
            },
            tags: {
                let t = parse_tags(&tags_text.get());
                if t.is_empty() { None } else { Some(t) }
            },
            creator: {
                let c = creator.get();
                if c.trim().is_empty() { None } else { Some(c) }
            },
            nsfw: Some(nsfw.get()),
        };

        spawn_local(async move {
            match api::create_character(&req).await {
                Ok(c) => page.set(Page::Character(c.id)),
                Err(e) => {
                    error.set(e);
                    submitting.set(false);
                }
            }
        });
    };

    view! {
        <div class="create">
            <button class="charpage__back" on:click=move |_| page.set(Page::Home)>
                "\u{2190} Back to Home"
            </button>
            <h1 class="hero__title">"Create a Character"</h1>

            <div class="create__grid">
                <aside class="create__preview">
                    <article class="card">
                        <img class="card__img" src=prev_avatar alt="preview" />
                        <div class="card__body">
                            <div class="card__name">{prev_name}</div>
                            <div class="card__tagline">{prev_tagline}</div>
                            <div class="card__tags">
                                {move || prev_tags()
                                    .into_iter()
                                    .map(|t| view! { <span class="tag">{t}</span> })
                                    .collect_view()}
                            </div>
                            <div class="card__meta">
                                <span class="card__creator">{prev_creator}</span>
                                <span class="card__stats">"\u{1F4AC} 0 \u{2764} 0"</span>
                            </div>
                        </div>
                    </article>
                </aside>

                <div class="create__form">
                    <label class="settings-row">
                        <span>"Name*"</span>
                        <input class="field" placeholder="A unique name for your character"
                            prop:value=move || name.get()
                            on:input=move |ev| name.set(event_target_value(&ev)) />
                    </label>
                    <label class="settings-row">
                        <span>"Tagline / Bio"<small>" — shown on the card"</small></span>
                        <input class="field" placeholder="A weary knight who'd die for you..."
                            prop:value=move || tagline.get()
                            on:input=move |ev| tagline.set(event_target_value(&ev)) />
                    </label>
                    <label class="settings-row">
                        <span>"Avatar image"<small>" — upload a file, or paste a direct image URL"</small></span>
                        <div class="avatar-input">
                            <label class="avatar-upload">
                                "\u{1F4F7} Upload"
                                <input type="file" accept="image/*" class="avatar-file"
                                    on:change=move |ev| {
                                        use wasm_bindgen::JsCast;
                                        let target = ev.target().unwrap();
                                        let input: web_sys::HtmlInputElement = target.unchecked_into();
                                        if let Some(files) = input.files() {
                                            if let Some(file) = files.get(0) {
                                                // Guard size client-side too (~512KB; backend hard-caps).
                                                if file.size() > 512.0 * 1024.0 {
                                                    error.set("That image is over 512 KB — pick a smaller file or paste an image URL instead.".to_string());
                                                } else {
                                                    error.set(String::new());
                                                    crate::upload::read_as_data_url(file, move |res| {
                                                        match res {
                                                            Ok(data_url) => avatar.set(data_url),
                                                            Err(e) => error.set(format!("Upload failed: {e}")),
                                                        }
                                                    });
                                                }
                                            }
                                        }
                                    } />
                            </label>
                            <input class="field" type="url" inputmode="url"
                                placeholder="…or https://i.pravatar.cc/400?img=12"
                                prop:value=move || avatar.get()
                                on:input=move |ev| avatar.set(event_target_value(&ev)) />
                        </div>
                    </label>
                    <label class="settings-row">
                        <span>"Creator handle"</span>
                        <input class="field" placeholder="@you"
                            prop:value=move || creator.get()
                            on:input=move |ev| creator.set(event_target_value(&ev)) />
                    </label>
                    <label class="settings-row">
                        <span>"Tags"<small>" — comma separated"</small></span>
                        <input class="field" placeholder="fantasy, romance, oc"
                            prop:value=move || tags_text.get()
                            on:input=move |ev| tags_text.set(event_target_value(&ev)) />
                    </label>
                    <label class="settings-row">
                        <span>"Personality"<small>" — how the character thinks and speaks"</small></span>
                        <textarea class="field field--code" rows="4"
                            placeholder="Confident, quick-witted, loyal to a fault..."
                            prop:value=move || personality.get()
                            on:input=move |ev| personality.set(event_target_value(&ev)) ></textarea>
                    </label>
                    <label class="settings-row">
                        <span>"First message"<small>" — the character's opening line in chat"</small></span>
                        <textarea class="field field--code" rows="4"
                            placeholder="*She looks up as you enter...*"
                            prop:value=move || first_message.get()
                            on:input=move |ev| first_message.set(event_target_value(&ev)) ></textarea>
                    </label>
                    <label class="settings-row">
                        <span>"Scenario"<small>" — the setting / situation"</small></span>
                        <textarea class="field field--code" rows="3"
                            placeholder="A moonlit balcony at a royal gala..."
                            prop:value=move || scenario.get()
                            on:input=move |ev| scenario.set(event_target_value(&ev)) ></textarea>
                    </label>
                    <label class="create__check">
                        <input type="checkbox"
                            prop:checked=move || nsfw.get()
                            on:change=move |ev| nsfw.set(event_target_checked(&ev)) />
                        <span>"NSFW"</span>
                    </label>

                    {move || (!error.get().is_empty()).then(|| view! {
                        <div class="create__error">{error.get()}</div>
                    })}

                    <button class="btn btn--login create__submit"
                        prop:disabled=move || !can_create()
                        on:click=submit>
                        {move || if submitting.get() { "Creating\u{2026}" } else { "Create character" }}
                    </button>
                </div>
            </div>
        </div>
    }
}
