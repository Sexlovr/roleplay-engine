//! Chat view: one-on-one conversation with a character.
//!
//! The backend handles the LLM proxying and stores every message. The frontend
//! just fetches/inserts/edits/deletes rows via the REST API. Navigation comes
//! in with a `chat_id` (the conversation), loads the full `ChatDetail`, and
//! then manages the message list locally with optimistic updates.

use leptos::html::Div;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use shared::dto::{MessageView, StreamMsg};

use crate::api;
use crate::markdown::{render_message, split_thinking};
use crate::Page;

/// Render a *user* message: inline markdown only (no thinking blocks).
fn render_text(text: &str) -> AnyView {
    view! { <span class="msg__text">{render_message(text)}</span> }.into_any()
}

/// Render a *bot* message: pull any `<think>…</think>` reasoning into a collapsed
/// `<details>` box above the answer, then render the answer as markdown.
fn render_bot(text: &str) -> AnyView {
    let (thinking, answer) = split_thinking(text);
    let think_box = thinking.map(|t| {
        view! {
            <details class="msg__think">
                <summary class="msg__think-sum">
                    <span class="msg__think-ic">"\u{1F4AD}"</span>
                    "Thought process"
                </summary>
                <div class="msg__think-body">{render_message(&t)}</div>
            </details>
        }
    });
    // A pure-reasoning chunk (still streaming, no answer yet) shows just the box.
    let answer_view = (!answer.trim().is_empty())
        .then(|| view! { <span class="msg__text">{render_message(&answer)}</span> });
    view! { {think_box} {answer_view} }.into_any()
}

#[component]
pub fn Chat(id: i64) -> impl IntoView {
    let page = use_context::<RwSignal<Page>>().unwrap();

    let settings_open = use_context::<crate::SettingsOpen>().unwrap().0;
    let persona_open = use_context::<crate::PersonaOpen>().unwrap().0;
    let nsfw = use_context::<crate::NsfwEnabled>().unwrap().0;
    let stream_on = use_context::<crate::theme::StreamCtx>().unwrap().0;

    let draft = RwSignal::new(String::new());
    let memory = RwSignal::new(String::new());
    let editing: RwSignal<Option<i64>> = RwSignal::new(None); // message id being edited
    let edit_draft = RwSignal::new(String::new());
    let sending = RwSignal::new(false);
    // Set true to ask an in-flight streaming generation to stop (polled by the
    // stream reader; the server still finishes + persists the reply).
    let stop_stream = RwSignal::new(false);
    // Transient LLM/upstream error shown as a dismissable banner above the
    // composer — never written into the message log (so it can't pollute swipe
    // variants or be mistaken for a real reply).
    let err_banner: RwSignal<Option<String>> = RwSignal::new(None);
    let log_ref: NodeRef<Div> = NodeRef::new();

    // Load the full chat detail from the server.
    let chat_resource = LocalResource::new(move || async move {
        api::get_chat(id).await.ok()
    });

    // Local message list — seeded once the server returns, then mutated locally.
    let messages: RwSignal<Vec<MessageView>> = RwSignal::new(Vec::new());
    let character_name = RwSignal::new(String::new());
    let character_avatar = RwSignal::new(String::new());
    let character_creator = RwSignal::new(String::new());
    let character_id = RwSignal::new(0i64);
    // Editable per-chat title (defaults to the character name on the server).
    let chat_title = RwSignal::new(String::new());
    let editing_title = RwSignal::new(false);
    let title_draft = RwSignal::new(String::new());
    let loaded = RwSignal::new(false);

    // When the server data arrives, hydrate local state.
    Effect::new(move |_| {
        if let Some(detail) = chat_resource.get().as_deref().cloned().flatten() {
            if !loaded.get_untracked() {
                messages.set(detail.messages);
                memory.set(detail.chat.memory.clone());
                chat_title.set(detail.chat.title.clone());
                character_name.set(detail.character.name.clone());
                character_avatar.set(detail.character.avatar.clone());
                character_creator.set(detail.character.creator.clone());
                character_id.set(detail.character.id);
                loaded.set(true);
            }
        }
    });

    // Auto-scroll to the bottom whenever the message list changes.
    Effect::new(move |_| {
        messages.with(|_| ());
        if let Some(el) = log_ref.get() {
            el.set_scroll_top(el.scroll_height());
        }
    });

    // ---- actions ----

    let do_send = move || {
        if sending.get_untracked() {
            return;
        }
        let text = draft.get().trim().to_string();
        if text.is_empty() {
            return;
        }
        draft.set(String::new());
        err_banner.set(None);
        let streaming = stream_on.get_untracked();
        // Optimistic: show the user message immediately.
        messages.update(|l| l.push(MessageView {
            id: 0, from_user: true, text: text.clone(), variants: Vec::new(), variant: 0,
        }));
        // In-flight bot placeholder — empty when streaming (it fills token by
        // token), an ellipsis otherwise.
        let ph = if streaming { String::new() } else { "\u{2026}".to_string() };
        messages.update(|l| l.push(MessageView {
            id: -1, from_user: false, text: ph, variants: Vec::new(), variant: 0,
        }));
        sending.set(true);

        if streaming {
            stop_stream.set(false);
            spawn_local(async move {
                let url = format!("/api/chats/{id}/send/stream");
                let res = crate::stream::stream_post(
                    &url,
                    serde_json::json!({ "text": text }),
                    move |msg| match msg {
                        // Reconcile the optimistic user bubble's id.
                        StreamMsg::User { id: uid } => messages.update(|l| {
                            if let Some(m) = l.iter_mut().find(|m| m.id == 0 && m.from_user) {
                                m.id = uid;
                            }
                        }),
                        StreamMsg::Delta { v } => messages.update(|l| {
                            if let Some(m) = l.iter_mut().find(|m| m.id == -1) {
                                m.text.push_str(&v);
                            }
                        }),
                        StreamMsg::Done { id: rid, variants, variant } => messages.update(|l| {
                            if let Some(m) = l.iter_mut().find(|m| m.id == -1) {
                                m.id = rid;
                                if !variants.is_empty() {
                                    m.variant = variant;
                                    if let Some(t) = variants.get(variant.max(0) as usize) {
                                        m.text = t.clone();
                                    }
                                    m.variants = variants;
                                }
                            }
                        }),
                        StreamMsg::Error { v } => err_banner.set(Some(v)),
                    },
                    move || stop_stream.get_untracked(),
                )
                .await;
                sending.set(false);
                // Drop an empty placeholder (errored before any token). A
                // non-empty one that never got `done` (the user tapped Stop)
                // stays as the partial — the server saved the full reply, which
                // appears on reload.
                messages.update(|l| l.retain(|m| !(m.id == -1 && m.text.trim().is_empty())));
                if let Err(e) = res {
                    err_banner.set(Some(e));
                }
            });
        } else {
            spawn_local(async move {
                let res = api::send_message(id, text.clone()).await;
                sending.set(false);
                // Drop the placeholder + optimistic user message before reconciling.
                messages.update(|l| l.retain(|m| m.id != -1 && m.id != 0));
                match res {
                    Ok(resp) => {
                        messages.update(|l| l.push(resp.user));
                        if let Some(reply) = resp.reply {
                            messages.update(|l| l.push(reply));
                        }
                        if let Some(err) = resp.error {
                            // LLM/upstream failure: the user message IS saved, but no
                            // reply was generated. Banner offers Retry (regenerate).
                            err_banner.set(Some(err));
                        }
                    }
                    Err(e) => {
                        // Network / backend error: the user message wasn't saved.
                        // Restore the draft so nothing is lost.
                        draft.set(text);
                        err_banner.set(Some(e));
                    }
                }
            });
        }
    };

    let do_regenerate = move || {
        if sending.get_untracked() {
            return;
        }
        let has_user = messages.with_untracked(|l| l.iter().any(|m| m.from_user));
        if !has_user {
            return;
        }
        // Stash the trailing bot message so we can restore it on error; show a
        // placeholder in its place (the server appends a new swipe variant and
        // returns the same message id).
        let stashed = messages.with_untracked(|l| {
            l.last().filter(|m| !m.from_user && m.id > 0).cloned()
        });
        if stashed.is_some() {
            messages.update(|l| { l.pop(); });
        }
        err_banner.set(None);
        let streaming = stream_on.get_untracked();
        let ph = if streaming { String::new() } else { "\u{2026}".to_string() };
        messages.update(|l| l.push(MessageView {
            id: -1, from_user: false, text: ph, variants: Vec::new(), variant: 0,
        }));
        sending.set(true);

        if streaming {
            stop_stream.set(false);
            let stashed_s = stashed.clone();
            spawn_local(async move {
                let url = format!("/api/chats/{id}/regenerate/stream");
                let res = crate::stream::stream_post(
                    &url,
                    serde_json::Value::Null,
                    move |msg| match msg {
                        StreamMsg::Delta { v } => messages.update(|l| {
                            if let Some(m) = l.iter_mut().find(|m| m.id == -1) {
                                m.text.push_str(&v);
                            }
                        }),
                        StreamMsg::Done { id: rid, variants, variant } => messages.update(|l| {
                            if let Some(m) = l.iter_mut().find(|m| m.id == -1) {
                                m.id = rid;
                                if !variants.is_empty() {
                                    m.variant = variant;
                                    if let Some(t) = variants.get(variant.max(0) as usize) {
                                        m.text = t.clone();
                                    }
                                    m.variants = variants;
                                }
                            }
                        }),
                        StreamMsg::Error { v } => err_banner.set(Some(v)),
                        StreamMsg::User { .. } => {}
                    },
                    move || stop_stream.get_untracked(),
                )
                .await;
                sending.set(false);
                // If the placeholder is still empty (errored before any token),
                // drop it and restore the prior reply so nothing is lost.
                let empty = messages
                    .with_untracked(|l| l.iter().any(|m| m.id == -1 && m.text.trim().is_empty()));
                if empty {
                    messages.update(|l| l.retain(|m| m.id != -1));
                    if let Some(prev) = stashed_s {
                        messages.update(|l| l.push(prev));
                    }
                }
                if let Err(e) = res {
                    err_banner.set(Some(e));
                }
            });
        } else {
            spawn_local(async move {
                let res = api::regenerate(id).await;
                sending.set(false);
                messages.update(|l| l.retain(|m| m.id != -1));
                match res {
                    // Success: the server appended a new swipe variant (same id).
                    Ok(resp) if resp.reply.is_some() => {
                        messages.update(|l| l.push(resp.reply.unwrap()));
                    }
                    // Structured error OR transport error: restore the prior reply
                    // and surface a dismissable banner. No error bubble is created.
                    Ok(resp) => {
                        if let Some(prev) = stashed {
                            messages.update(|l| l.push(prev));
                        }
                        err_banner.set(Some(resp.error.unwrap_or_else(|| "Generation failed.".into())));
                    }
                    Err(e) => {
                        if let Some(prev) = stashed {
                            messages.update(|l| l.push(prev));
                        }
                        err_banner.set(Some(e));
                    }
                }
            });
        }
    };

    // Switch which stored variant (swipe) of a bot message is shown.
    let do_swipe = move |msg_id: i64, dir: i64| {
        if sending.get_untracked() || msg_id <= 0 {
            return;
        }
        let target = messages.with_untracked(|l| {
            l.iter().find(|m| m.id == msg_id).and_then(|m| {
                let count = m.variants.len() as i64;
                if count <= 1 {
                    return None;
                }
                let next = (m.variant + dir).rem_euclid(count);
                Some((next, m.variants[next as usize].clone()))
            })
        });
        let Some((next, text)) = target else { return; };
        // Optimistic local switch.
        messages.update(|l| {
            if let Some(m) = l.iter_mut().find(|m| m.id == msg_id) {
                m.variant = next;
                m.text = text;
            }
        });
        spawn_local(async move {
            let _ = api::select_variant(msg_id, next).await;
        });
    };

    let do_edit = move |msg_id: i64| {
        let text = edit_draft.get().trim().to_string();
        if text.is_empty() || msg_id <= 0 {
            editing.set(None);
            return;
        }
        // Snapshot prior text so we can roll back if the server rejects the edit.
        let prev = messages.with_untracked(|l| {
            l.iter().find(|m| m.id == msg_id).map(|m| m.text.clone())
        });
        // Optimistic local update...
        messages.update(|l| {
            if let Some(m) = l.iter_mut().find(|m| m.id == msg_id) {
                m.text = text.clone();
            }
        });
        editing.set(None);
        // ...then persist; roll back + surface the error on failure.
        spawn_local(async move {
            if let Err(e) = api::edit_message(msg_id, text).await {
                if let Some(prev) = prev {
                    messages.update(|l| {
                        if let Some(m) = l.iter_mut().find(|m| m.id == msg_id) {
                            m.text = prev;
                        }
                    });
                }
                messages.update(|l| l.push(MessageView {
                    id: -2,
                    from_user: false,
                    text: format!("\u{26A0} edit failed: {e}"),
                    variants: Vec::new(),
                    variant: 0,
                }));
            }
        });
    };

    let do_delete = move |msg_id: i64| {
        if msg_id <= 0 {
            return;
        }
        // Any message may be deleted, including the last one (an empty chat is
        // valid — the user can send again or regenerate to seed a fresh reply).
        messages.update(|l| l.retain(|m| m.id != msg_id));
        spawn_local(async move {
            let _ = api::delete_message(msg_id).await;
        });
    };

    let save_memory = move |_| {
        let m = memory.get_untracked();
        spawn_local(async move {
            let _ = api::update_memory(id, m).await;
        });
    };

    let save_title = move || {
        editing_title.set(false);
        let t = title_draft.get_untracked().trim().to_string();
        if t.is_empty() || t == chat_title.get_untracked() {
            return;
        }
        chat_title.set(t.clone());
        spawn_local(async move {
            let _ = api::rename_chat(id, t).await;
        });
    };

    // ---- view helpers ----

    let log_view = move || {
        let editing_id = editing.get();
        let msgs = messages.get();
        let last = msgs.len().saturating_sub(1);
        let has_user = msgs.iter().any(|m| m.from_user);
        let av = character_avatar.get();
        msgs.into_iter()
            .enumerate()
            .map(move |(i, m)| {
                let from_user = m.from_user;
                let is_placeholder = m.id == -1;
                let is_editing = editing_id == Some(m.id);
                let is_last_bot = i == last && !from_user && has_user && !is_placeholder;
                // Swipes don't require a prior user turn, so a fresh chat's
                // greeting (the last/only bot message) can still cycle its
                // seeded alternate-greeting variants.
                let is_swipeable = i == last && !from_user && !is_placeholder;
                let text = m.text.clone();
                let msg_id = m.id;
                let edit_seed = m.text.clone();
                let variant_count = m.variants.len();
                let variant_idx = m.variant;

                let body = if is_editing {
                    view! {
                        <div class="msg__edit">
                            <textarea class="field field--code" rows="3"
                                prop:value=move || edit_draft.get()
                                on:input=move |ev| edit_draft.set(event_target_value(&ev)) ></textarea>
                            <div class="msg__editbtns">
                                <button class="btn" on:click=move |_| editing.set(None)>"Cancel"</button>
                                <button class="btn btn--login" on:click=move |_| {
                                    do_edit(msg_id);
                                }>"Save"</button>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    let bubble = if from_user {
                        view! { <div class="msg__bubble">{render_text(&text)}</div> }.into_any()
                    } else {
                        view! { <div class="msg__bubble">{render_bot(&text)}</div> }.into_any()
                    };
                    bubble
                };

                // Swipe controls: shown on the last bot message when it has
                // more than one stored variant.
                let swipes = (is_swipeable && variant_count > 1).then(|| {
                    let swipe_l = do_swipe;
                    let swipe_r = do_swipe;
                    view! {
                        <div class="msg__swipe">
                            <button class="msg__act" title="Previous" on:click=move |_| swipe_l(msg_id, -1)>"\u{2039}"</button>
                            <span class="msg__swipecount">{format!("{}/{}", variant_idx + 1, variant_count)}</span>
                            <button class="msg__act" title="Next" on:click=move |_| swipe_r(msg_id, 1)>"\u{203A}"</button>
                        </div>
                    }
                });

                let actions = (!is_placeholder).then(|| view! {
                    <div class="msg__actions">
                        {swipes}
                        {is_last_bot.then(|| {
                            let regen = do_regenerate.clone();
                            view! { <button class="msg__act" title="Regenerate" on:click=move |_| regen()>"\u{21BB}"</button> }
                        })}
                        {(msg_id > 0).then(|| view! {
                            <button class="msg__act" title="Edit"
                                on:click=move |_| { editing.set(Some(msg_id)); edit_draft.set(edit_seed.clone()); }>"\u{270E}"</button>
                            <button class="msg__act" title="Delete"
                                on:click=move |_| do_delete(msg_id)>"\u{1F5D1}"</button>
                        })}
                    </div>
                });

                if from_user {
                    view! {
                        <div class="msg msg--user">
                            <div class="msg__wrap">{body}{actions}</div>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <div class="msg msg--bot">
                            <img class="msg__avatar" src=av.clone() alt="" />
                            <div class="msg__wrap">{body}{actions}</div>
                        </div>
                    }.into_any()
                }
            })
            .collect_view()
    };

    let model_label = {
        let cfg_sig = use_context::<crate::ApiConfig>().unwrap().0;
        move || {
            let c = cfg_sig.get();
            if c.url.trim().is_empty() {
                "\u{2699} API".to_string()
            } else {
                format!("\u{2699} {}", c.name)
            }
        }
    };

    view! {
        <Transition fallback=move || view! {
            <div class="chat">
                <div class="chat__topbar">
                    <button class="chat__back" on:click=move |_| page.set(Page::Home)>
                        "\u{2190}"
                    </button>
                    <div class="chat__title">
                        <span class="chat__name">"Loading\u{2026}"</span>
                    </div>
                </div>
            </div>
        }>
            {move || {
                if !loaded.get() {
                    return view! {
                        <div class="chat">
                            <div class="chat__topbar">
                                <button class="chat__back" on:click=move |_| page.set(Page::Home)>
                                    "\u{2190}"
                                </button>
                                <div class="chat__title">
                                    <span class="chat__name">"Loading\u{2026}"</span>
                                </div>
                            </div>
                        </div>
                    }.into_any();
                }

                let avatar = character_avatar.get();

                // Memory panel
                let memory_open = RwSignal::new(false);
                // Hamburger dropdown (everything except the proxy/settings icon)
                let menu_open = RwSignal::new(false);

                view! {
                    <div class="chat">
                        <div class="chat__topbar">
                            <button class="chat__back" title="Back to character"
                                on:click=move |_| page.set(Page::Character(character_id.get()))>
                                "\u{2190}"
                            </button>
                            <img class="chat__avatar" src=avatar alt=""
                                on:click=move |_| page.set(Page::Character(character_id.get())) />
                            <div class="chat__title">
                                {move || if editing_title.get() {
                                    view! {
                                        <input class="chat__titleinput" autofocus=true
                                            prop:value=move || title_draft.get()
                                            on:input=move |ev| title_draft.set(event_target_value(&ev))
                                            on:blur=move |_| save_title()
                                            on:keydown=move |ev| {
                                                if ev.key() == "Enter" { ev.prevent_default(); save_title(); }
                                                else if ev.key() == "Escape" { editing_title.set(false); }
                                            } />
                                    }.into_any()
                                } else {
                                    view! {
                                        <span class="chat__name" title="Click to rename"
                                            on:click=move |_| {
                                                let cur = chat_title.get();
                                                title_draft.set(if cur.trim().is_empty() { character_name.get() } else { cur });
                                                editing_title.set(true);
                                            }>
                                            {move || {
                                                let t = chat_title.get();
                                                if t.trim().is_empty() { character_name.get() } else { t }
                                            }}
                                        </span>
                                    }.into_any()
                                }}
                            </div>
                            <button class="chat__model" title="API / model settings"
                                on:click=move |_| settings_open.set(true)>
                                {model_label}
                            </button>
                            <div class="chat__menuwrap">
                                <button class="chat__menubtn" aria-label="Menu"
                                    on:click=move |_| menu_open.update(|v| *v = !*v)>
                                    "\u{2630}"
                                </button>
                                {move || menu_open.get().then(|| view! {
                                    <>
                                        <div class="menu-backdrop" on:click=move |_| menu_open.set(false)></div>
                                        <div class="chat__menu">
                                            // Navigating unmounts this whole chat view (and the menu with
                                            // it), so DON'T also touch `menu_open` here — notifying the
                                            // menu's reactive closure while its scope is being disposed
                                            // panics ("accessed a disposed reactive value"). Just navigate.
                                            <button on:click=move |_| page.set(Page::Home)>"\u{1F3E0} Discover"</button>
                                            <button on:click=move |_| page.set(Page::Chats)>"\u{1F4AC} Chats"</button>
                                            <button on:click=move |_| page.set(Page::Create)>"\u{2795} Create"</button>
                                            <div class="chat__menu-sep"></div>
                                            <button on:click=move |_| { persona_open.set(true); menu_open.set(false); }>"\u{1F464} Persona"</button>
                                            <button on:click=move |_| { memory_open.set(true); menu_open.set(false); }>"\u{1F9E0} Chat memory"</button>
                                            <button on:click=move |_| {
                                                let cur = chat_title.get();
                                                title_draft.set(if cur.trim().is_empty() { character_name.get() } else { cur });
                                                editing_title.set(true);
                                                menu_open.set(false);
                                            }>"\u{270E} Rename chat"</button>
                                            <button class=("chat__menu--on", move || nsfw.get())
                                                on:click=move |_| nsfw.update(|v| *v = !*v)>
                                                {move || if nsfw.get() { "\u{1F513} NSFW \u{00B7} on" } else { "\u{1F512} NSFW \u{00B7} off" }}
                                            </button>
                                            <button class=("chat__menu--on", move || stream_on.get())
                                                on:click=move |_| {
                                                    let v = !stream_on.get_untracked();
                                                    stream_on.set(v);
                                                    crate::theme::save_stream(v);
                                                }>
                                                {move || if stream_on.get() { "\u{26A1} Streaming \u{00B7} on" } else { "\u{26A1} Streaming \u{00B7} off" }}
                                            </button>
                                            <div class="chat__menu-sep"></div>
                                            <div class="chat__menu-label">"Theme"</div>
                                            <crate::theme::ThemePicker/>
                                        </div>
                                    </>
                                })}
                            </div>
                        </div>

                        <div class="chat__log" node_ref=log_ref>{log_view}</div>

                        {move || err_banner.get().map(|e| {
                            let regen = do_regenerate;
                            view! {
                                <div class="chat__error" role="alert">
                                    <span class="chat__error-msg">{e}</span>
                                    <span class="chat__error-acts">
                                        <button class="chat__error-retry"
                                            on:click=move |_| { err_banner.set(None); regen(); }>
                                            "\u{21BB} Retry"
                                        </button>
                                        <button class="chat__error-x" title="Dismiss"
                                            on:click=move |_| err_banner.set(None)>"\u{2715}"</button>
                                    </span>
                                </div>
                            }
                        })}

                        <div class="chat__composer">
                            <textarea
                                class="chat__input"
                                rows="1"
                                aria-label="Type a message"
                                prop:value=move || draft.get()
                                placeholder="Type a message..."
                                on:input=move |ev| draft.set(event_target_value(&ev))
                                on:keydown={
                                    let do_send = do_send.clone();
                                    move |ev| {
                                        if ev.key() == "Enter" && !ev.shift_key() {
                                            ev.prevent_default();
                                            do_send();
                                        }
                                    }
                                }
                            ></textarea>
                            {move || if sending.get() && stream_on.get() {
                                // Mid-stream: offer Stop (cancels rendering; the
                                // server still finishes and saves the reply).
                                view! {
                                    <button class="chat__send chat__send--stop"
                                        on:click=move |_| stop_stream.set(true)>
                                        "\u{25A0} Stop"
                                    </button>
                                }.into_any()
                            } else {
                                view! {
                                    <button class="chat__send" prop:disabled=move || sending.get()
                                        on:click=move |_| do_send()>
                                        "Send"
                                    </button>
                                }.into_any()
                            }}
                        </div>

                        // Chat memory panel (slide-in)
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
                                            "Notes injected into every prompt — relationships, plot points, facts to remember."
                                        </div>
                                        <textarea class="field field--code" rows="10"
                                            prop:value=move || memory.get()
                                            on:input=move |ev| memory.set(event_target_value(&ev)) ></textarea>
                                    </div>
                                    <div class="settings-actions">
                                        <button class="btn btn--login" on:click=move |_| {
                                            save_memory(());
                                            memory_open.set(false);
                                        }>"Save"</button>
                                    </div>
                                </aside>
                            </>
                        })}
                    </div>
                }.into_any()
            }}
        </Transition>
    }
}
