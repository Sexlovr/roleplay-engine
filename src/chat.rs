//! Chat view: one-on-one conversation with a character.
//!
//! Each user turn is sent to the user-configured endpoint (see [`crate::api`]),
//! with a system prompt built from the character + the user's persona + an
//! optional per-chat memory note. Supports per-message regenerate / edit /
//! delete, and a menu for persona / memory / restart.

use leptos::html::Div;
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
        pending: false,
    }]);
    let draft = RwSignal::new(String::new());
    let memory = RwSignal::new(String::new());
    let editing: RwSignal<Option<usize>> = RwSignal::new(None);
    let edit_draft = RwSignal::new(String::new());
    let menu_open = RwSignal::new(false);
    let memory_open = RwSignal::new(false);
    // True while an API request is in flight — locks out concurrent sends so
    // we never fire duplicate requests or shift the log under a pending reply.
    let sending = RwSignal::new(false);
    // Ref to the scroll container so we can keep the newest message in view.
    let log_ref: NodeRef<Div> = NodeRef::new();

    // Auto-scroll to the bottom whenever the message list changes (or the log
    // first mounts). Reading messages + log_ref subscribes the effect to both.
    Effect::new(move |_| {
        messages.with(|_| ());
        if let Some(el) = log_ref.get() {
            el.set_scroll_top(el.scroll_height());
        }
    });

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
    // call the endpoint, then fill in the placeholder with the reply/error.
    let run = {
        let build_system = build_system.clone();
        move || {
            if sending.get_untracked() {
                return; // a request is already in flight
            }
            let cfg = cfg_sig.get_untracked();
            if cfg.url.trim().is_empty() {
                messages.update(|l| {
                    l.push(ChatMessage {
                        from_user: false,
                        text: "\u{26A0} No endpoint configured yet. Tap the model button (top-right) to point me at your proxy/API.".into(),
                        pending: false,
                    })
                });
                return;
            }
            let system = build_system();
            let history = messages.get_untracked();
            sending.set(true);
            messages.update(|l| l.push(ChatMessage { from_user: false, text: "\u{2026}".into(), pending: true }));
            spawn_local(async move {
                let res = api::send_chat(cfg, history, system).await;
                // Locate the placeholder by its `pending` flag, not a captured
                // index, so it survives any edit/delete of other messages.
                messages.update(|l| {
                    if let Some(m) = l.iter_mut().find(|m| m.pending) {
                        m.text = match res {
                            Ok(r) => r,
                            Err(e) => format!("\u{26A0} {e}"),
                        };
                        m.pending = false;
                    }
                });
                sending.set(false);
            });
        }
    };

    let send = {
        let run = run.clone();
        move || {
            if sending.get_untracked() {
                return;
            }
            let text = draft.get().trim().to_string();
            if text.is_empty() {
                return;
            }
            messages.update(|l| l.push(ChatMessage { from_user: true, text, pending: false }));
            draft.set(String::new());
            run();
        }
    };

    let regenerate = {
        let run = run.clone();
        move || {
            if sending.get_untracked() {
                return;
            }
            // Nothing to regenerate from if the user hasn't spoken yet (only the
            // opening greeting exists) — leave it intact rather than sending an
            // empty, user-less request.
            let has_user = messages.with_untracked(|l| l.iter().any(|m| m.from_user));
            if !has_user {
                return;
            }
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
            messages.set(vec![ChatMessage { from_user: false, text: greeting.clone(), pending: false }]);
            sending.set(false);
            menu_open.set(false);
        }
    };

    let log_view = move || {
        let editing_idx = editing.get();
        let msgs = messages.get();
        let last = msgs.len().saturating_sub(1);
        let has_user = msgs.iter().any(|m| m.from_user);
        let can_delete = msgs.len() > 1;
        msgs.into_iter()
            .enumerate()
            .map(|(i, m)| {
                let from_user = m.from_user;
                let pending = m.pending;
                let is_editing = editing_idx == Some(i);
                // Show Regenerate only on the trailing bot reply, and only once
                // the user has actually said something.
                let is_last_bot = i == last && !from_user && has_user;
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

                // The in-flight "…" placeholder gets no action buttons.
                let actions = (!pending).then(|| view! {
                    <div class="msg__actions">
                        {is_last_bot.then(|| {
                            let regen = regen.clone();
                            view! { <button class="msg__act" title="Regenerate" on:click=move |_| regen()>"\u{21BB}"</button> }
                        })}
                        <button class="msg__act" title="Edit"
                            on:click=move |_| { editing.set(Some(i)); edit_draft.set(edit_seed.clone()); }>"\u{270E}"</button>
                        {can_delete.then(|| view! {
                            <button class="msg__act" title="Delete"
                                on:click=move |_| messages.update(|l| { if i < l.len() && l.len() > 1 { l.remove(i); } })>"\u{1F5D1}"</button>
                        })}
                    </div>
                });

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

            <div class="chat__log" node_ref=log_ref>{log_view}</div>

            <div class="chat__composer">
                <textarea
                    class="chat__input"
                    rows="1"
                    aria-label="Type a message"
                    prop:value=move || draft.get()
                    placeholder="Type a message..."
                    on:input=move |ev| draft.set(event_target_value(&ev))
                    on:keydown={
                        let send = send.clone();
                        move |ev| {
                            // Enter sends; Shift+Enter inserts a newline.
                            if ev.key() == "Enter" && !ev.shift_key() {
                                ev.prevent_default();
                                send();
                            }
                        }
                    }
                ></textarea>
                <button class="chat__send" prop:disabled=move || sending.get() on:click=move |_| send()>
                    "Send"
                </button>
            </div>

            {move || memory_open.get().then(|| view! {
                <>
                    <div class="settings-backdrop" on:click=move |_| memory_open.set(false)></div>
                    <aside class="settings-panel">
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
