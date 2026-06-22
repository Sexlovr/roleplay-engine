//! Chat view: one-on-one conversation with a character.
//!
//! Looks the character up by id from [`crate::data::characters`]. Seeds the log
//! with the character's `description` as an opening bot message, then lets the
//! user send messages; each user message gets a canned in-character reply.

use leptos::prelude::*;

use crate::data;
use crate::types::{ChatMessage, Page};

#[component]
pub fn Chat(id: u32) -> impl IntoView {
    let page = use_context::<RwSignal<Page>>().unwrap();

    // Resolve the character once for this id.
    let character = data::characters().into_iter().find(|c| c.id == id);

    let Some(character) = character else {
        return view! {
            <div class="chat">
                <div class="chat__topbar">
                    <button class="chat__back" on:click=move |_| page.set(Page::Home)>
                        "\u{2190}"
                    </button>
                    <div class="chat__title">
                        <span class="chat__name">"Character not found"</span>
                    </div>
                </div>
            </div>
        }
        .into_any();
    };

    // Owned copies for use inside closures / static views.
    let name = character.name.clone();
    let creator = character.creator.clone();
    let avatar = character.avatar.clone();

    // Avatar shown next to each bot bubble.
    let bot_avatar = avatar.clone();
    // Name used to build the canned reply.
    let reply_name = name.clone();

    // Conversation log, seeded with the character's intro as a bot message.
    let messages: RwSignal<Vec<ChatMessage>> = RwSignal::new(vec![ChatMessage {
        from_user: false,
        text: character.description.clone(),
    }]);

    // The composer's working text.
    let draft = RwSignal::new(String::new());

    // Send the current draft (if non-empty): push the user message, then a
    // canned in-character acknowledgement, then clear the draft.
    let send = move || {
        let text = draft.get();
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        let reply = format!(
            "*{} smiles* I hear you. Tell me more...",
            reply_name
        );
        messages.update(|log| {
            log.push(ChatMessage { from_user: true, text });
            log.push(ChatMessage {
                from_user: false,
                text: reply,
            });
        });
        draft.set(String::new());
    };

    // Derived render of the log. Cloned per render since the vec changes.
    let log_view = move || {
        messages
            .get()
            .into_iter()
            .map(|m| {
                let bubble = m.text.clone();
                if m.from_user {
                    view! {
                        <div class="msg msg--user">
                            <div class="msg__bubble">{bubble}</div>
                        </div>
                    }
                    .into_any()
                } else {
                    let av = bot_avatar.clone();
                    view! {
                        <div class="msg msg--bot">
                            <img class="msg__avatar" src=av alt="" />
                            <div class="msg__bubble">{bubble}</div>
                        </div>
                    }
                    .into_any()
                }
            })
            .collect_view()
    };

    view! {
        <div class="chat">
            <div class="chat__topbar">
                <button class="chat__back" on:click=move |_| page.set(Page::Character(id))>
                    "\u{2190}"
                </button>
                <img class="chat__avatar" src=avatar alt="" />
                <div class="chat__title">
                    <span class="chat__name">{name}</span>
                    <span class="chat__creator">{format!("by {}", creator)}</span>
                </div>
            </div>

            <div class="chat__log">
                {log_view}
            </div>

            <div class="chat__composer">
                <input
                    prop:value=move || draft.get()
                    placeholder="Type a message..."
                    on:input=move |ev| draft.set(event_target_value(&ev))
                    on:keydown={
                        let send = send.clone();
                        move |ev| {
                            if ev.key() == "Enter" {
                                send();
                            }
                        }
                    }
                />
                <button class="chat__send" on:click=move |_| send()>
                    "Send"
                </button>
            </div>
        </div>
    }
    .into_any()
}
