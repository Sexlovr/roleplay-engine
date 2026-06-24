//! Character detail page: a sticky character card on the left (art, name,
//! creator, tags, "Chat with X" CTA) and a right column with the character's
//! definition broken into collapsible sections with token estimates.
//!
//! The "Chat with X" button creates a chat on the server and navigates to it.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::home::compact;
use crate::Page;

/// Rough token estimate (~1.3 tokens per whitespace word).
fn est_tokens(s: &str) -> u32 {
    ((s.split_whitespace().count() as f32) * 1.3).ceil() as u32
}

#[component]
pub fn CharacterPage(id: i64) -> impl IntoView {
    let page = use_context::<RwSignal<Page>>().unwrap();

    // Fetch the character from the server.
    let character = LocalResource::new(move || async move { api::get_character(id).await.ok() });

    // Loading + error states.
    view! {
        <Transition fallback=move || view! {
            <div class="charpage">
                <button class="charpage__back" on:click=move |_| page.set(Page::Home)>
                    "\u{2190} Back to Home"
                </button>
                <p class="hero__sub">"Loading\u{2026}"</p>
            </div>
        }>
            {move || {
                let Some(c) = character.get().as_deref().cloned().flatten() else {
                    return view! {
                        <div class="charpage charpage--missing">
                            <button class="charpage__back" on:click=move |_| page.set(Page::Home)>
                                "\u{2190} Back to Home"
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
                let chats = compact(c.messages);
                let likes = compact(c.likes);

                let pers_tok = est_tokens(personality);
                let greet_tok = est_tokens(greeting);
                let desc_tok = est_tokens(if description.is_empty() { scenario } else { description });
                let total_tok = pers_tok + greet_tok + desc_tok;

                // Chat-button state — creates a chat on the server.
                let starting = RwSignal::new(false);
                let start_chat = move |_| {
                    if starting.get_untracked() { return; }
                    starting.set(true);
                    spawn_local(async move {
                        match api::create_chat(id).await {
                            Ok(detail) => page.set(Page::Chat(detail.chat.id)),
                            Err(_e) => {
                                starting.set(false);
                            }
                        }
                    });
                };

                view! {
                    <div class="charpage">
                        <button class="charpage__back" on:click=move |_| page.set(Page::Home)>
                            "\u{2190} Back to Home"
                        </button>

                        <div class="charpage__grid">
                            <aside class="charpage__aside">
                                <div class="charpage__card">
                                    <img class="charpage__art" src=avatar alt=alt />
                                    <div class="charpage__cardbody">
                                        <h1 class="charpage__name">{name}</h1>
                                        <div class="charpage__creator">{format!("by {}", creator)}</div>
                                        <div class="charpage__stats">
                                            <span>{format!("\u{1F4AC} {}", chats)}</span>
                                            <span>{format!("\u{2764} {}", likes)}</span>
                                        </div>
                                        <div class="card__tags">
                                            {tags
                                                .into_iter()
                                                .map(|t| view! { <span class="tag">{t}</span> })
                                                .collect_view()}
                                        </div>
                                        <button
                                            class="charpage__chat"
                                            prop:disabled=move || starting.get()
                                            on:click=start_chat
                                        >
                                            {move || if starting.get() { "Starting\u{2026}".to_string() } else { format!("Chat with {}", c.name.clone()) }}
                                        </button>
                                        <button class="charpage__edit" on:click=move |_| page.set(Page::Edit(id))>
                                            "\u{270E} Edit character"
                                        </button>
                                    </div>
                                </div>
                            </aside>

                            <main class="charpage__main">
                                <p class="charpage__tagline">{tagline}</p>

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
