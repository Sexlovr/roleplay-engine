//! Home / gallery page: hero, sort tabs, tag filter bar, a responsive grid of
//! character cards, and pagination. Filtering/sorting is reactive over the
//! global search query, the NSFW toggle, a selected tag, the sort mode, and the
//! current page.

use leptos::prelude::*;

use crate::data;
use crate::types::{Character, Page};

const PAGE_SIZE: usize = 8;

#[derive(Clone, Copy, PartialEq)]
enum Sort {
    Popular,
    New,
    Trending,
}

/// Compact a count for the card meta row: `1_234 -> "1.2k"`, `980_000 -> "980k"`.
pub fn compact(n: u32) -> String {
    if n < 1_000 {
        n.to_string()
    } else if n < 1_000_000 {
        if n < 10_000 && n % 1_000 != 0 {
            // Floor to one decimal so 9_999 reads "9.9k", never "10.0k"
            // (which `{:.1}` rounding would otherwise produce just under 10k).
            format!("{:.1}k", (n as f64 / 100.0).floor() / 10.0)
        } else {
            format!("{}k", n / 1_000)
        }
    } else if n < 10_000_000 && n % 1_000_000 != 0 {
        format!("{:.1}M", (n as f64 / 100_000.0).floor() / 10.0)
    } else {
        format!("{}M", n / 1_000_000)
    }
}

#[component]
pub fn Home() -> impl IntoView {
    let page = use_context::<RwSignal<Page>>().unwrap();
    let search = use_context::<crate::SearchQuery>().unwrap().0;
    let nsfw = use_context::<crate::NsfwEnabled>().unwrap().0;

    let selected_tag: RwSignal<Option<String>> = RwSignal::new(None);
    let sort = RwSignal::new(Sort::Popular);
    let cur_page = RwSignal::new(1usize);

    // Reset to page 1 whenever a filter/sort changes.
    Effect::new(move |_| {
        search.get();
        selected_tag.get();
        sort.get();
        nsfw.get();
        cur_page.set(1);
    });

    // Unique, sorted tag list across all characters (recomputed if user adds).
    let all_tags = move || {
        let mut tags: Vec<String> = data::all()
            .into_iter()
            .flat_map(|c| c.tags.into_iter())
            .collect();
        tags.sort();
        tags.dedup();
        tags
    };

    // Filtered + sorted list (before pagination).
    let list = move || {
        let q = search.get().trim().to_lowercase();
        let nsfw_ok = nsfw.get();
        let sel = selected_tag.get();
        let mut v: Vec<Character> = data::all()
            .into_iter()
            .filter(|c| nsfw_ok || !c.nsfw)
            .filter(|c| {
                q.is_empty()
                    || c.name.to_lowercase().contains(&q)
                    || c.tagline.to_lowercase().contains(&q)
                    || c.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .filter(|c| match &sel {
                None => true,
                Some(tag) => c.tags.iter().any(|t| t == tag),
            })
            .collect();
        match sort.get() {
            Sort::Popular => v.sort_by(|a, b| b.messages.cmp(&a.messages)),
            Sort::Trending => v.sort_by(|a, b| b.likes.cmp(&a.likes)),
            Sort::New => v.sort_by(|a, b| b.id.cmp(&a.id)),
        }
        v
    };

    // (slice for current page, total pages, clamped current page)
    let paged = move || {
        let v = list();
        let total = v.len();
        let pages = total.div_ceil(PAGE_SIZE).max(1);
        let p = cur_page.get().clamp(1, pages);
        let slice: Vec<Character> = v.into_iter().skip((p - 1) * PAGE_SIZE).take(PAGE_SIZE).collect();
        (slice, pages, p)
    };

    let tag_chips = move || {
        all_tags()
            .into_iter()
            .map(|tag| {
                let tag_active = tag.clone();
                let tag_click = tag.clone();
                let is_active = move || selected_tag.get().as_deref() == Some(tag_active.as_str());
                view! {
                    <button
                        class="tag-chip"
                        class=("tag-chip--active", is_active)
                        on:click=move |_| {
                            let t = tag_click.clone();
                            selected_tag.update(|s| {
                                if s.as_deref() == Some(t.as_str()) { *s = None } else { *s = Some(t) }
                            });
                        }
                    >
                        {tag.clone()}
                    </button>
                }
            })
            .collect_view()
    };

    let tab = move |label: &'static str, this: Sort| {
        view! {
            <button class="tab" class=("tab--active", move || sort.get() == this)
                on:click=move |_| sort.set(this)>
                {label}
            </button>
        }
    };

    view! {
        <section class="home">
            <div class="hero">
                <h1 class="hero__title">"Discover Characters"</h1>
                <p class="hero__sub">"Chat with thousands of AI personalities — or create your own."</p>
            </div>

            <div class="tab-bar">
                {tab("\u{2B50} Popular", Sort::Popular)}
                {tab("\u{1F195} New", Sort::New)}
                {tab("\u{1F525} Trending", Sort::Trending)}
            </div>

            <div class="tag-bar">
                <button class="tag-chip" class=("tag-chip--active", move || selected_tag.get().is_none())
                    on:click=move |_| selected_tag.set(None)>
                    "All"
                </button>
                {tag_chips}
            </div>

            {move || {
                let (cards, pages, p) = paged();
                if cards.is_empty() {
                    return view! {
                        <p class="hero__sub">"No characters match your filters. Try clearing the search or picking a different tag."</p>
                    }.into_any();
                }
                view! {
                    <>
                    <div class="card-grid">
                        <For
                            each=move || paged().0
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
                                    <article class="card" on:click=move |_| page.set(Page::Character(id))>
                                        <img class="card__img" src=avatar alt=alt />
                                        <div class="card__body">
                                            <div class="card__name">{name}</div>
                                            <div class="card__tagline">{tagline}</div>
                                            <div class="card__tags">
                                                {tags.into_iter().map(|t| view! { <span class="tag">{t}</span> }).collect_view()}
                                            </div>
                                            <div class="card__meta">
                                                <span class="card__creator">{creator}</span>
                                                <span class="card__stats">
                                                    {format!("\u{1F4AC} {}", messages)} " "
                                                    {format!("\u{2764} {}", likes)}
                                                </span>
                                            </div>
                                        </div>
                                    </article>
                                }
                            }
                        />
                    </div>
                    {(pages > 1).then(|| view! {
                        <div class="pager">
                            <button class="pager__btn" prop:disabled={move || p <= 1}
                                on:click={move |_| cur_page.update(|n| { if *n > 1 { *n -= 1 } })}>"‹ Prev"</button>
                            <span class="pager__info">{format!("Page {p} of {pages}")}</span>
                            <button class="pager__btn" prop:disabled={move || p >= pages}
                                on:click={move |_| cur_page.update(|n| { if *n < pages { *n += 1 } })}>"Next ›"</button>
                        </div>
                    })}
                    </>
                }.into_any()
            }}
        </section>
    }
}
