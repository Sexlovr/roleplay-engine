//! Character detail page: a sticky character card on the left (art, name,
//! creator, tags, and chat actions) and a right column listing your existing
//! conversations plus the character's definition (collapsible, with token
//! estimates).
//!
//! Chat actions: "Continue" resumes your most recent conversation with this
//! character; "New chat" always starts a fresh one. If you've never chatted,
//! a single "Start chat" button does the same.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use shared::dto::ChatListEntry;

use crate::api;
use crate::home::compact;
use crate::util::{rel_time, snippet};
use crate::Page;

/// Rough token estimate (~1.3 tokens per whitespace word).
fn est_tokens(s: &str) -> u32 {
    ((s.split_whitespace().count() as f32) * 1.3).ceil() as u32
}

#[component]
pub fn CharacterPage(id: i64) -> impl IntoView {
    let page = use_context::<RwSignal<Page>>().unwrap();

    // Fetch the character + this character's chat sessions.
    let character = LocalResource::new(move || async move { api::get_character(id).await.ok() });
    let chats_resource =
        LocalResource::new(move || async move { api::list_chats_for(id).await.unwrap_or_default() });

    // Local, mutable copy of the chat list so we can delete optimistically.
    let chats: RwSignal<Vec<ChatListEntry>> = RwSignal::new(Vec::new());
    let chats_loaded = RwSignal::new(false);
    Effect::new(move |_| {
        if let Some(list) = chats_resource.get().as_deref().cloned() {
            if !chats_loaded.get_untracked() {
                chats.set(list);
                chats_loaded.set(true);
            }
        }
    });

    // Chat-button state — creates a chat on the server, then opens it.
    let starting = RwSignal::new(false);
    let start_chat = move |_| {
        if starting.get_untracked() {
            return;
        }
        starting.set(true);
        spawn_local(async move {
            match api::create_chat(id).await {
                Ok(detail) => page.set(Page::Chat(detail.chat.id)),
                Err(_e) => starting.set(false),
            }
        });
    };

    let delete_chat = move |cid: i64| {
        chats.update(|l| l.retain(|c| c.id != cid));
        spawn_local(async move {
            let _ = api::delete_chat(cid).await;
        });
    };

    // Chat actions block (Continue + New, or Start) — reactive on the list.
    let chat_actions = move || {
        let list = chats.get();
        match list.first() {
            Some(first) => {
                let latest = first.id;
                view! {
                    <button class="charpage__chat" on:click=move |_| page.set(Page::Chat(latest))>
                        "\u{25B6} Continue chat"
                    </button>
                    <button class="charpage__newchat" prop:disabled=move || starting.get()
                        on:click=start_chat>
                        {move || if starting.get() { "Starting\u{2026}".to_string() } else { "\u{2795} New chat".to_string() }}
                    </button>
                }.into_any()
            }
            None => view! {
                <button class="charpage__chat" prop:disabled=move || starting.get() on:click=start_chat>
                    {move || if starting.get() { "Starting\u{2026}".to_string() } else { "\u{1F4AC} Start chat".to_string() }}
                </button>
            }.into_any(),
        }
    };

    // The list of existing conversations with this character.
    let chat_list = move || {
        let list = chats.get();
        (!list.is_empty()).then(|| {
            let rows = list
                .into_iter()
                .map(|c| {
                    let cid = c.id;
                    let when = rel_time(c.updated_at);
                    let snip = {
                        let s = snippet(&c.last_message, 80);
                        if s.is_empty() {
                            "No messages yet".to_string()
                        } else if c.last_from_user {
                            format!("You: {s}")
                        } else {
                            s
                        }
                    };
                    view! {
                        <div class="charchats__row" on:click=move |_| page.set(Page::Chat(cid))>
                            <div class="charchats__snip">{snip}</div>
                            <span class="charchats__time">{when}</span>
                            <button class="charchats__del" title="Delete chat"
                                on:click=move |ev: leptos::ev::MouseEvent| { ev.stop_propagation(); delete_chat(cid); }>
                                "\u{1F5D1}"
                            </button>
                        </div>
                    }
                })
                .collect_view();
            view! {
                <div class="charchats">
                    <div class="charchats__hdr">"Your conversations"</div>
                    {rows}
                </div>
            }
        })
    };

    view! {
        <Transition fallback=move || view! {
            <div class="charpage">
                <button class="charpage__back" on:click=move |_| page.set(Page::Home)>
                    "\u{2190} Back to Discover"
                </button>
                <p class="hero__sub">"Loading\u{2026}"</p>
            </div>
        }>
            {move || {
                let Some(c) = character.get().as_deref().cloned().flatten() else {
                    return view! {
                        <div class="charpage charpage--missing">
                            <button class="charpage__back" on:click=move |_| page.set(Page::Home)>
                                "\u{2190} Back to Discover"
                            </button>
                            <p class="hero__sub">"Character not found."</p>
                        </div>
                    }.into_any();
                };

                let name = c.name.clone();
                let alt = c.name.clone();
                let creator = if c.creator.is_empty() { "\u{2014}".to_string() } else { format!("@{}", c.creator) };
                let tagline = c.tagline.clone();
                let personality = if c.personality.is_empty() { &c.tagline } else { &c.personality };
                let greeting = if c.first_message.is_empty() { &c.tagline } else { &c.first_message };
                let description = &c.description;
                let scenario = &c.scenario;
                let avatar = if c.avatar.is_empty() {
                    "https://picsum.photos/seed/empty/400/533".to_string()
                } else {
                    c.avatar.clone()
                };
                let tags = c.tags.clone();
                let chats_count = compact(c.messages);
                let likes = compact(c.likes);
                let lore_count = c.lorebook.iter().filter(|e| e.enabled).count();

                let pers_tok = est_tokens(personality);
                let greet_tok = est_tokens(greeting);
                let desc_tok = est_tokens(if description.is_empty() { scenario } else { description });
                let total_tok = pers_tok + greet_tok + desc_tok;

                view! {
                    <div class="charpage">
                        <button class="charpage__back" on:click=move |_| page.set(Page::Home)>
                            "\u{2190} Back to Discover"
                        </button>

                        <div class="charpage__grid">
                            <aside class="charpage__aside">
                                <div class="charpage__card">
                                    <img class="charpage__art" src=avatar alt=alt />
                                    <div class="charpage__cardbody">
                                        <h1 class="charpage__name">{name}</h1>
                                        <div class="charpage__creator">{format!("by {}", creator)}</div>
                                        <div class="charpage__stats">
                                            <span>{format!("\u{1F4AC} {}", chats_count)}</span>
                                            <span>{format!("\u{2764} {}", likes)}</span>
                                            {(lore_count > 0).then(|| view! { <span>{format!("\u{1F4D6} {}", lore_count)}</span> })}
                                        </div>
                                        <div class="card__tags">
                                            {tags
                                                .into_iter()
                                                .map(|t| view! { <span class="tag">{t}</span> })
                                                .collect_view()}
                                        </div>
                                        <div class="charpage__actions">
                                            {chat_actions}
                                        </div>
                                        <button class="charpage__edit" on:click=move |_| page.set(Page::Edit(id))>
                                            "\u{270E} Edit character"
                                        </button>
                                    </div>
                                </div>
                            </aside>

                            <main class="charpage__main">
                                <p class="charpage__tagline">{tagline}</p>

                                {chat_list}

                                <div class="charpage__defhdr">
                                    <span>"Character Definition"</span>
                                    <span class="charpage__toktotal">{format!("~{} tokens", total_tok)}</span>
                                </div>

                                {(!greeting.is_empty()).then(|| view! {
                                    <details class="def" open>
                                        <summary>
                                            "First Message"
                                            <span class="def__tok">{format!("{} tokens", greet_tok)}</span>
                                        </summary>
                                        <div class="def__body">{greeting.clone()}</div>
                                    </details>
                                })}

                                {(!personality.is_empty()).then(|| view! {
                                    <details class="def">
                                        <summary>
                                            "Personality"
                                            <span class="def__tok">{format!("{} tokens", pers_tok)}</span>
                                        </summary>
                                        <div class="def__body">{personality.clone()}</div>
                                    </details>
                                })}

                                {(!description.is_empty()).then(|| view! {
                                    <details class="def">
                                        <summary>
                                            "Description"
                                            <span class="def__tok">{format!("{} tokens", desc_tok)}</span>
                                        </summary>
                                        <div class="def__body">{description.clone()}</div>
                                    </details>
                                })}

                                {(!scenario.is_empty()).then(|| view! {
                                    <details class="def">
                                        <summary>"Scenario"</summary>
                                        <div class="def__body">{scenario.clone()}</div>
                                    </details>
                                })}
                            </main>
                        </div>
                    </div>
                }.into_any()
            }}
        </Transition>
    }
}
