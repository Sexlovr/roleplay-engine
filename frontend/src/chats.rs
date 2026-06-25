//! Global "Chats" tab: every recent conversation across all characters, newest
//! first. Each row shows the character avatar, name, a snippet of the last
//! message, and a relative timestamp. Click a row to resume; trash to delete.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use shared::dto::ChatListEntry;

use crate::api;
use crate::util::{rel_time, snippet};
use crate::Page;

#[component]
pub fn Chats() -> impl IntoView {
    let page = use_context::<RwSignal<Page>>().unwrap();

    let resource = LocalResource::new(move || async move { api::list_recent_chats().await.ok() });

    // Hydrate a local list once so deletes can update optimistically.
    let items: RwSignal<Vec<ChatListEntry>> = RwSignal::new(Vec::new());
    let loaded = RwSignal::new(false);
    Effect::new(move |_| {
        if let Some(list) = resource.get().as_deref().cloned().flatten() {
            if !loaded.get_untracked() {
                items.set(list);
                loaded.set(true);
            }
        }
    });

    let do_delete = move |id: i64| {
        items.update(|l| l.retain(|c| c.id != id));
        spawn_local(async move {
            let _ = api::delete_chat(id).await;
        });
    };

    let rows = move || {
        items
            .get()
            .into_iter()
            .map(|c| {
                let id = c.id;
                let avatar = if c.avatar.is_empty() {
                    "https://picsum.photos/seed/empty/96/96".to_string()
                } else {
                    c.avatar.clone()
                };
                let name = c.character_name.clone();
                let when = rel_time(c.updated_at);
                let snip = {
                    let s = snippet(&c.last_message, 90);
                    if s.is_empty() {
                        "No messages yet".to_string()
                    } else if c.last_from_user {
                        format!("You: {s}")
                    } else {
                        s
                    }
                };
                view! {
                    <div class="chatrow" on:click=move |_| page.set(Page::Chat(id))>
                        <img class="chatrow__avatar" src=avatar alt="" />
                        <div class="chatrow__body">
                            <div class="chatrow__top">
                                <span class="chatrow__name">{name}</span>
                                <span class="chatrow__time">{when}</span>
                            </div>
                            <div class="chatrow__snippet">{snip}</div>
                        </div>
                        <button class="chatrow__del" title="Delete chat"
                            on:click=move |ev: leptos::ev::MouseEvent| { ev.stop_propagation(); do_delete(id); }>
                            "\u{1F5D1}"
                        </button>
                    </div>
                }
            })
            .collect_view()
    };

    view! {
        <section class="chats">
            <div class="page-hdr">
                <h1 class="page-hdr__title">"Your Chats"</h1>
                <p class="page-hdr__sub">"Pick up any conversation where you left off."</p>
            </div>
            <Transition fallback=move || view! { <p class="hero__sub">"Loading\u{2026}"</p> }>
                {move || {
                    if loaded.get() && items.get().is_empty() {
                        return view! {
                            <div class="home__empty">
                                <div class="home__empty-icon">"\u{1F4AC}"</div>
                                <p class="hero__title">"No chats yet"</p>
                                <p class="hero__sub">
                                    "Open a character and start a conversation — it'll show up here."
                                </p>
                                <button class="btn btn--login" on:click=move |_| page.set(Page::Home)>
                                    "Browse characters"
                                </button>
                            </div>
                        }.into_any();
                    }
                    view! { <div class="chatlist">{rows}</div> }.into_any()
                }}
            </Transition>
        </section>
    }
}
