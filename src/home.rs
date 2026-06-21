//! Home / gallery page: hero, tag filter bar, and a responsive grid of
//! character cards. All filtering is reactive over the global search query,
//! the NSFW toggle, and a local selected-tag signal.

use leptos::prelude::*;

use crate::types::Page;
use crate::data;

/// Compact a count for the card meta row: `1_234 -> "1.2k"`, `980_000 -> "980k"`.
fn compact(n: u32) -> String {
    if n < 1_000 {
        n.to_string()
    } else if n < 1_000_000 {
        let thousands = n as f64 / 1_000.0;
        // Show one decimal only when it adds information (and isn't a round k).
        if n < 10_000 && n % 1_000 != 0 {
            format!("{:.1}k", thousands)
        } else {
            format!("{}k", n / 1_000)
        }
    } else {
        let millions = n as f64 / 1_000_000.0;
        if n < 10_000_000 && n % 1_000_000 != 0 {
            format!("{:.1}M", millions)
        } else {
            format!("{}M", n / 1_000_000)
        }
    }
}

#[component]
pub fn Home() -> impl IntoView {
    let page = use_context::<RwSignal<Page>>().unwrap();
    let search = use_context::<crate::SearchQuery>().unwrap().0;
    let nsfw = use_context::<crate::NsfwEnabled>().unwrap().0;

    let selected_tag: RwSignal<Option<String>> = RwSignal::new(None);

    // Build the unique, sorted tag list from all characters (computed once).
    let all_tags: Vec<String> = {
        let mut tags: Vec<String> = data::characters()
            .into_iter()
            .flat_map(|c| c.tags.into_iter())
            .collect();
        tags.sort();
        tags.dedup();
        tags
    };

    // Reactive filtered character list.
    let filtered = move || {
        let q = search.get().trim().to_lowercase();
        let nsfw_ok = nsfw.get();
        let sel = selected_tag.get();
        data::characters()
            .into_iter()
            .filter(|c| nsfw_ok || !c.nsfw)
            .filter(|c| {
                if q.is_empty() {
                    return true;
                }
                c.name.to_lowercase().contains(&q)
                    || c.tagline.to_lowercase().contains(&q)
                    || c.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .filter(|c| match &sel {
                None => true,
                Some(tag) => c.tags.iter().any(|t| t == tag),
            })
            .collect::<Vec<_>>()
    };

    // "All" chip is active when no tag is selected.
    let all_active = move || selected_tag.get().is_none();

    // One chip per unique tag.
    let tag_chips = all_tags
        .into_iter()
        .map(|tag| {
            let tag_for_active = tag.clone();
            let tag_for_click = tag.clone();
            let is_active =
                move || selected_tag.get().as_deref() == Some(tag_for_active.as_str());
            view! {
                <button
                    class="tag-chip"
                    class=("tag-chip--active", is_active)
                    on:click=move |_| {
                        let t = tag_for_click.clone();
                        selected_tag
                            .update(|s| {
                                if s.as_deref() == Some(t.as_str()) {
                                    *s = None;
                                } else {
                                    *s = Some(t);
                                }
                            });
                    }
                >
                    {tag.clone()}
                </button>
            }
        })
        .collect_view();

    view! {
        <section class="home">
            <div class="hero">
                <h1 class="hero__title">"Discover Characters"</h1>
                <p class="hero__sub">
                    "Chat with thousands of AI personalities — or create your own."
                </p>
            </div>

            <div class="tag-bar">
                <button
                    class="tag-chip"
                    class=("tag-chip--active", all_active)
                    on:click=move |_| selected_tag.set(None)
                >
                    "All"
                </button>
                {tag_chips}
            </div>

            {move || {
                let cards = filtered();
                if cards.is_empty() {
                    view! {
                        <p class="hero__sub">
                            "No characters match your filters. Try clearing the search or picking a different tag."
                        </p>
                    }
                        .into_any()
                } else {
                    view! {
                        <div class="card-grid">
                            <For
                                each=filtered
                                key=|c| c.id
                                children=move |c| {
                                    let id = c.id;
                                    let avatar = c.avatar.clone();
                                    let name = c.name.clone();
                                    let alt = c.name.clone();
                                    let tagline = c.tagline.clone();
                                    let creator = c.creator.clone();
                                    let tags = c.tags.clone();
                                    let messages = compact(c.messages);
                                    let likes = compact(c.likes);
                                    view! {
                                        <article
                                            class="card"
                                            on:click=move |_| page.set(Page::Chat(id))
                                        >
                                            <img class="card__img" src=avatar alt=alt />
                                            <div class="card__body">
                                                <div class="card__name">{name}</div>
                                                <div class="card__tagline">{tagline}</div>
                                                <div class="card__tags">
                                                    {tags
                                                        .into_iter()
                                                        .map(|t| view! { <span class="tag">{t}</span> })
                                                        .collect_view()}
                                                </div>
                                                <div class="card__meta">
                                                    <span class="card__creator">{creator}</span>
                                                    <span class="card__stats">
                                                        {format!("💬 {}", messages)} " "
                                                        {format!("❤ {}", likes)}
                                                    </span>
                                                </div>
                                            </div>
                                        </article>
                                    }
                                }
                            />
                        </div>
                    }
                        .into_any()
                }
            }}
        </section>
    }
}
