//! Chat view: one-on-one conversation with a character.
//!
//! Looks the character up by id from [`crate::data::characters`], seeds the log
//! with the character's `description` as the opening message, then sends each
//! user turn to the user-configured endpoint (see [`crate::api`]). If no
//! endpoint is configured, it points the user at the API Settings drawer.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::data;
use crate::types::{ChatMessage, Page};

#[component]
pub fn Chat(id: u32) -> impl IntoView {
    let page = use_context::<RwSignal<Page>>().unwrap();

    let Some(character) = data::characters().into_iter().find(|c| c.id == id) else {
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

    let cfg_sig = use_context::<crate::ApiConfig>().unwrap().0;
    let settings_open = use_context::<crate::SettingsOpen>().unwrap().0;

    let name = character.name.clone();
    let creator = character.creator.clone();
    let avatar = character.avatar.clone();
    let bot_avatar = avatar.clone();

    // System prompt that frames the model as this character.
    let system = format!(
        "You are {name}, a roleplay character. {tagline}\n\nStay fully in \
         character as {name}: write vivid, immersive, in-character replies and \
         never mention being an AI.",
        name = character.name,
        tagline = character.tagline,
    );

    // Conversation log, seeded with the character's intro as a bot message.
    let messages: RwSignal<Vec<ChatMessage>> = RwSignal::new(vec![ChatMessage {
        from_user: false,
        text: character.description.clone(),
    }]);
    let draft = RwSignal::new(String::new());

    // Send the draft: push the user turn, then stream a reply from the endpoint
    // into a placeholder bubble (or an inline error if it fails).
    let send = {
        let system = system.clone();
        move || {
            let text = draft.get().trim().to_string();
            if text.is_empty() {
                return;
            }
            messages.update(|l| l.push(ChatMessage { from_user: true, text }));
            draft.set(String::new());

            let cfg = cfg_sig.get_untracked();
            if cfg.url.trim().is_empty() {
                messages.update(|l| {
                    l.push(ChatMessage {
                        from_user: false,
                        text: "\u{26A0} No endpoint configured yet. Tap the model button (top-right) to point me at your proxy/API.".into(),
                    })
                });
                return;
            }

            let history = messages.get_untracked();
            let idx = messages.with_untracked(|l| l.len());
            messages.update(|l| l.push(ChatMessage { from_user: false, text: "\u{2026}".into() }));

            let system = system.clone();
            spawn_local(async move {
                let res = api::send_chat(cfg, history, system).await;
                messages.update(|l| {
                    if let Some(m) = l.get_mut(idx) {
                        m.text = match res {
                            Ok(r) => r,
                            Err(e) => format!("\u{26A0} {e}"),
                        };
                    }
                });
            });
        }
    };

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

    let model_label = move || {
        let c = cfg_sig.get();
        if c.url.trim().is_empty() {
            "\u{2699} set up model".to_string()
        } else {
            format!("\u{2699} using {}", c.name)
        }
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
                <button class="chat__model" on:click=move |_| settings_open.set(true)>
                    {model_label}
                </button>
            </div>

            <div class="chat__log">{log_view}</div>

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
