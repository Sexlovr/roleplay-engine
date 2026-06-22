//! Character detail page (Janitor-AI style): a sticky character card on the
//! left (art, name, creator, tags, "Chat with X" CTA) and a right column with
//! the character's definition broken into collapsible sections with token
//! estimates, plus a comments area.

use leptos::prelude::*;

use crate::data;
use crate::home::compact;
use crate::types::Page;

/// Rough token estimate (~1.3 tokens per whitespace word) for the definition
/// breakdown. ponytail: good enough for a display badge; the real tokenizer
/// only matters once chat is wired to a model.
fn est_tokens(s: &str) -> u32 {
    ((s.split_whitespace().count() as f32) * 1.3).ceil() as u32
}

#[component]
pub fn CharacterPage(id: u32) -> impl IntoView {
    let page = use_context::<RwSignal<Page>>().unwrap();

    let Some(c) = data::find(id) else {
        return view! {
            <div class="charpage charpage--missing">
                <button class="charpage__back" on:click=move |_| page.set(Page::Home)>
                    "\u{2190} Back to Home"
                </button>
                <p class="hero__sub">"Character not found."</p>
            </div>
        }
        .into_any();
    };

    let name = c.name.clone();
    let cta_name = c.name.clone();
    let alt = c.name.clone();
    let creator = c.creator.clone();
    let tagline = c.tagline.clone();
    let personality = c.tagline.clone();
    let greeting = c.description.clone();
    let avatar = c.avatar.clone();
    let tags = c.tags.clone();
    let chats = compact(c.messages);
    let likes = compact(c.likes);

    let pers_tok = est_tokens(&c.tagline);
    let greet_tok = est_tokens(&c.description);
    let total_tok = pers_tok + greet_tok;

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
                                on:click=move |_| page.set(Page::Chat(id))
                            >
                                {format!("Chat with {}", cta_name)}
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

                    <details class="def" open>
                        <summary>
                            "First Message"
                            <span class="def__tok">{format!("{} tokens", greet_tok)}</span>
                        </summary>
                        <div class="def__body">{greeting}</div>
                    </details>

                    <details class="def">
                        <summary>
                            "Personality"
                            <span class="def__tok">{format!("{} tokens", pers_tok)}</span>
                        </summary>
                        <div class="def__body">{personality}</div>
                    </details>

                    <div class="comments">
                        <div class="comments__hdr">"Comments"</div>
                        <div class="comment">
                            <div class="comment__author">"@traveler"</div>
                            <div class="comment__text">
                                "This card is incredible — instant favorite."
                            </div>
                        </div>
                        <div class="comment">
                            <div class="comment__author">"@nightowl"</div>
                            <div class="comment__text">
                                "Great writing, stays in character the whole way. 10/10."
                            </div>
                        </div>
                    </div>
                </main>
            </div>
        </div>
    }
    .into_any()
}
