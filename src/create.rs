//! Create-a-Character page: a form with a live card preview. New characters are
//! persisted (localStorage) and appear on the home grid immediately.

use leptos::prelude::*;

use crate::data;
use crate::types::{Character, Page};

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
    let description = RwSignal::new(String::new());
    let nsfw = RwSignal::new(false);

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

    let can_create = move || !name.get().trim().is_empty();

    let create = move |_| {
        if name.get().trim().is_empty() {
            return;
        }
        let avatar_val = {
            let a = avatar.get();
            if a.trim().is_empty() { FALLBACK_AVATAR.to_string() } else { a }
        };
        let creator_val = {
            let c = creator.get();
            if c.trim().is_empty() { "@you".to_string() } else { c }
        };
        let c = Character {
            id: 0,
            name: name.get(),
            tagline: tagline.get(),
            description: description.get(),
            avatar: avatar_val,
            tags: parse_tags(&tags_text.get()),
            creator: creator_val,
            messages: 0,
            likes: 0,
            nsfw: nsfw.get(),
        };
        let id = data::add_user_character(c);
        page.set(Page::Character(id));
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
                        <span>"Avatar URL"</span>
                        <input class="field" placeholder="https://i.pravatar.cc/400?img=12"
                            prop:value=move || avatar.get()
                            on:input=move |ev| avatar.set(event_target_value(&ev)) />
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
                        <span>"First message"<small>" — the character's opening line in chat"</small></span>
                        <textarea class="field field--code" rows="5"
                            placeholder="*She looks up as you enter...*"
                            prop:value=move || description.get()
                            on:input=move |ev| description.set(event_target_value(&ev)) ></textarea>
                    </label>
                    <label class="create__check">
                        <input type="checkbox"
                            prop:checked=move || nsfw.get()
                            on:change=move |ev| nsfw.set(event_target_checked(&ev)) />
                        <span>"NSFW"</span>
                    </label>

                    <button class="btn btn--login create__submit"
                        prop:disabled=move || !can_create()
                        on:click=create>
                        "Create character"
                    </button>
                </div>
            </div>
        </div>
    }
}
