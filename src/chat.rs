//! Chat view: one-on-one conversation with a character.
//!
//! Each user turn is sent to the user-configured endpoint (see [`crate::api`]),
//! with a system prompt built from the character + the user's persona + an
//! optional per-chat memory note. Supports per-message regenerate / edit /
//! delete, and a menu for persona / memory / restart.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::data;
use crate::types::{ChatMessage, Page};

#[component]
pub fn Chat(id: u32) -> impl IntoView {
    let page = use_context::<RwSignal<Page>>().unwrap();

    let Some(character) = data::find(id) else {
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
    let persona = use_context::<crate::PersonaCtx>().unwrap().0;
    let persona_open = use_context::<crate::PersonaOpen>().unwrap().0;

    let name = character.name.clone();
    let creator = character.creator.clone();
    let avatar = character.avatar.clone();
    let bot_avatar = avatar.clone();
    let greeting = character.description.clone();

    let messages: RwSignal<Vec<ChatMessage>> = RwSignal::new(vec![ChatMessage {
        from_user: false,
        text: greeting.clone(),
    }]);
    let draft = RwSignal::new(String::new());
    let memory = RwSignal::new(String::new());
    let editing: RwSignal<Option<usize>> = RwSignal::new(None);
    let edit_draft = RwSignal::new(String::new());
    let menu_open = RwSignal::new(false);
    let memory_open = RwSignal::new(false);

    // Build the system prompt from character + persona + memory (read at call time).
    let build_system = {
        let cname = character.name.clone();
        let ctag = character.tagline.clone();
        move || -> String {
            let mut s = format!(
                "You are {cname}, a roleplay character. {ctag}\n\nStay fully in \
                 character as {cname}: write vivid, immersive, in-character replies \
                 and never mention being an AI.",
            );
            let p = persona.get_untracked();
            if !p.name.trim().is_empty() || !p.description.trim().is_empty() {
                let who = if p.name.trim().is_empty() { "the user".to_string() } else { p.name.clone() };
                s.push_str(&format!("\n\nThe user is roleplaying as {who}."));
                if !p.description.trim().is_empty() {
                    s.push_str(&format!(" {}", p.description));
                }
            }
            let m = memory.get_untracked();
            if !m.trim().is_empty() {
                s.push_str(&format!("\n\nImportant context to remember:\n{m}"));
            }
            s
        }
    };

    // Shared completion runner: validate config, append a placeholder bubble,
    // call the endpoint, then replace the placeholder with the reply/error.
    let run = {
        let build_system = build_system.clone();
        move || {
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
            let system = build_system();
            let history = messages.get_untracked();
            let idx = messages.with_untracked(|l| l.len());
            messages.update(|l| l.push(ChatMessage { from_user: false, text: "\u{2026}".into() }));
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

    let send = {
        let run = run.clone();
        move || {
            let text = draft.get().trim().to_string();
            if text.is_empty() {
                return;
            }
            messages.update(|l| l.push(ChatMessage { from_user: true, text }));
            draft.set(String::new());
            run();
        }
    };

    let regenerate = {
        let run = run.clone();
        move || {
            let last_is_bot = messages.with_untracked(|l| l.last().map_or(false, |m| !m.from_user));
            if last_is_bot {
                messages.update(|l| {
                    l.pop();
                });
            }
            run();
        }
    };

    let restart = {
        let greeting = greeting.clone();
        move || {
            messages.set(vec![ChatMessage { from_user: false, text: greeting.clone() }]);
            menu_open.set(false);
        }
    };

    let log_view = move || {
        let editing_idx = editing.get();
        let msgs = messages.get();
        let last = msgs.len().saturating_sub(1);
        msgs.into_iter()
            .enumerate()
            .map(|(i, m)| {
                let from_user = m.from_user;
                let is_editing = editing_idx == Some(i);
                let is_last_bot = i == last && !from_user;
                let av = bot_avatar.clone();
                let text = m.text.clone();
                let edit_seed = m.text.clone();
                let regen = regenerate.clone();

                let body = if is_editing {
                    view! {
                        <div class="msg__edit">
                            <textarea class="field field--code" rows="3"
                                prop:value=move || edit_draft.get()
                                on:input=move |ev| edit_draft.set(event_target_value(&ev)) ></textarea>
                            <div class="msg__editbtns">
                                <button class="btn" on:click=move |_| editing.set(None)>"Cancel"</button>
                                <button class="btn btn--login" on:click=move |_| {
                                    let t = edit_draft.get();
                                    messages.update(|l| if let Some(mm) = l.get_mut(i) { mm.text = t; });
                                    editing.set(None);
                                }>"Save"</button>
                            </div>
                        </div>
                    }
                    .into_any()
                } else {
                    view! { <div class="msg__bubble">{text}</div> }.into_any()
                };

                let actions = view! {
                    <div class="msg__actions">
                        {is_last_bot.then(|| {
                            let regen = regen.clone();
                            view! { <button class="msg__act" title="Regenerate" on:click=move |_| regen()>"\u{21BB}"</button> }
                        })}
                        <button class="msg__act" title="Edit"
                            on:click=move |_| { editing.set(Some(i)); edit_draft.set(edit_seed.clone()); }>"\u{270E}"</button>
                        <button class="msg__act" title="Delete"
                            on:click=move |_| messages.update(|l| { if i < l.len() { l.remove(i); } })>"\u{1F5D1}"</button>
                    </div>
                };

                if from_user {
                    view! {
                        <div class="msg msg--user">
                            <div class="msg__wrap">{body}{actions}</div>
                        </div>
                    }
                    .into_any()
                } else {
                    view! {
                        <div class="msg msg--bot">
                            <img class="msg__avatar" src=av alt="" />
                            <div class="msg__wrap">{body}{actions}</div>
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
                <div class="chat__menuwrap">
                    <button class="chat__menubtn" on:click=move |_| menu_open.update(|v| *v = !*v)>
                        "\u{2630}"
                    </button>
                    {move || {
                        let restart = restart.clone();
                        menu_open.get().then(move || view! {
                            <>
                                <div class="menu-backdrop" on:click=move |_| menu_open.set(false)></div>
                                <div class="chat__menu">
                                    <button on:click=move |_| { settings_open.set(true); menu_open.set(false); }>"\u{2699} API Settings"</button>
                                    <button on:click=move |_| { persona_open.set(true); menu_open.set(false); }>"\u{1F464} Persona"</button>
                                    <button on:click=move |_| { memory_open.set(true); menu_open.set(false); }>"\u{1F9E0} Chat Memory"</button>
                                    <button on:click=move |_| restart()>"\u{21BB} Restart chat"</button>
                                </div>
                            </>
                        })
                    }}
                </div>
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

            {move || memory_open.get().then(|| view! {
                <>
                    <div class="settings-backdrop" on:click=move |_| memory_open.set(false)></div>
                    <aside class="settings-panel" on:click=|ev| ev.stop_propagation()>
                        <div class="settings-panel__hdr">
                            <span>"\u{1F9E0} Chat Memory"</span>
                            <button class="settings-close" on:click=move |_| memory_open.set(false)>"\u{2715}"</button>
                        </div>
                        <div class="settings-body">
                            <div class="field-hint">
                                "Notes that get added to the prompt every turn — relationships, plot so far, facts the character should never forget."
                            </div>
                            <textarea class="field field--code" rows="10"
                                prop:value=move || memory.get()
                                on:input=move |ev| memory.set(event_target_value(&ev)) ></textarea>
                        </div>
                        <div class="settings-actions">
                            <button class="btn btn--login" on:click=move |_| memory_open.set(false)>"Done"</button>
                        </div>
                    </aside>
                </>
            })}
        </div>
    }
    .into_any()
}
