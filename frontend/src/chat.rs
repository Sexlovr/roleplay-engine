//! Chat view: one-on-one conversation with a character.
//!
//! The backend handles the LLM proxying and stores every message. The frontend
//! just fetches/inserts/edits/deletes rows via the REST API. Navigation comes
//! in with a `chat_id` (the conversation), loads the full `ChatDetail`, and
//! then manages the message list locally with optimistic updates.

use leptos::html::Div;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use shared::dto::MessageView;

use crate::api;
use crate::markdown::render_message;
use crate::Page;

/// Render message text as inline markdown (`*em*`, `**strong**`, `` `code` ``,
/// `> quote`, `![](img)`). Wrapped in `.msg__text` for layout.
fn render_text(text: &str) -> AnyView {
    view! { <span class="msg__text">{render_message(text)}</span> }.into_any()
}

#[component]
pub fn Chat(id: i64) -> impl IntoView {
    let page = use_context::<RwSignal<Page>>().unwrap();

    let settings_open = use_context::<crate::SettingsOpen>().unwrap().0;
    let persona_open = use_context::<crate::PersonaOpen>().unwrap().0;

    let draft = RwSignal::new(String::new());
    let memory = RwSignal::new(String::new());
    let editing: RwSignal<Option<i64>> = RwSignal::new(None); // message id being edited
    let edit_draft = RwSignal::new(String::new());
    let sending = RwSignal::new(false);
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
    let loaded = RwSignal::new(false);

    // When the server data arrives, hydrate local state.
    Effect::new(move |_| {
        if let Some(detail) = chat_resource.get().as_deref().cloned().flatten() {
            if !loaded.get_untracked() {
                messages.set(detail.messages);
                memory.set(detail.chat.memory.clone());
                character_name.set(detail.character.name.clone());
                character_avatar.set(detail.character.avatar.clone());
                character_creator.set(detail.character.creator.clone());
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
        // Optimistic: show the user message immediately.
        messages.update(|l| l.push(MessageView {
            id: 0, from_user: true, text: text.clone(), variants: Vec::new(), variant: 0,
        }));
        // In-flight placeholder.
        messages.update(|l| l.push(MessageView {
            id: -1, from_user: false, text: "\u{2026}".into(), variants: Vec::new(), variant: 0,
        }));
        sending.set(true);

        spawn_local(async move {
            let res = api::send_message(id, text.clone()).await;
            sending.set(false);
            messages.update(|l| {
                // Remove the placeholder.
                l.retain(|m| m.id != -1);
                // Remove the optimistic user message.
                l.retain(|m| m.id != 0);
                match res {
                    Ok(resp) => {
                        l.push(resp.user);
                        l.push(resp.reply);
                    }
                    Err(e) => {
                        l.push(MessageView {
                            id: -2, from_user: false, text: format!("\u{26A0} {e}"),
                            variants: Vec::new(), variant: 0,
                        });
                        // Restore the user's text so it isn't lost — they can retry.
                        draft.set(text);
                    }
                }
            });
        });
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
        messages.update(|l| l.push(MessageView {
            id: -1, from_user: false, text: "\u{2026}".into(), variants: Vec::new(), variant: 0,
        }));
        sending.set(true);

        spawn_local(async move {
            let res = api::regenerate(id).await;
            sending.set(false);
            messages.update(|l| {
                l.retain(|m| m.id != -1);
                match res {
                    Ok(resp) => l.push(resp.reply),
                    Err(e) => {
                        // Restore the previous reply, then show the error.
                        if let Some(prev) = stashed {
                            l.push(prev);
                        }
                        l.push(MessageView {
                            id: -2, from_user: false, text: format!("\u{26A0} {e}"),
                            variants: Vec::new(), variant: 0,
                        });
                    }
                }
            });
        });
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
        let too_few = messages.with_untracked(|l| l.len() <= 1);
        if too_few {
            return;
        }
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

    // ---- view helpers ----

    let log_view = move || {
        let editing_id = editing.get();
        let msgs = messages.get();
        let last = msgs.len().saturating_sub(1);
        let has_user = msgs.iter().any(|m| m.from_user);
        let can_delete = msgs.len() > 1;
        let av = character_avatar.get();
        msgs.into_iter()
            .enumerate()
            .map(move |(i, m)| {
                let from_user = m.from_user;
                let is_placeholder = m.id == -1;
                let is_error = m.id == -2;
                let is_editing = editing_id == Some(m.id);
                let is_last_bot = i == last && !from_user && has_user && !is_error && !is_placeholder;
                // Swipes don't require a prior user turn, so a fresh chat's
                // greeting (the last/only bot message) can still cycle its
                // seeded alternate-greeting variants.
                let is_swipeable = i == last && !from_user && !is_error && !is_placeholder;
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
                    view! { <div class="msg__bubble">{render_text(&text)}</div> }.into_any()
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

                let actions = (!is_placeholder && !is_error).then(|| view! {
                    <div class="msg__actions">
                        {swipes}
                        {is_last_bot.then(|| {
                            let regen = do_regenerate.clone();
                            view! { <button class="msg__act" title="Regenerate" on:click=move |_| regen()>"\u{21BB}"</button> }
                        })}
                        {(msg_id > 0).then(|| view! {
                            <button class="msg__act" title="Edit"
                                on:click=move |_| { editing.set(Some(msg_id)); edit_draft.set(edit_seed.clone()); }>"\u{270E}"</button>
                            {can_delete.then(|| view! {
                                <button class="msg__act" title="Delete"
                                    on:click=move |_| do_delete(msg_id)>"\u{1F5D1}"</button>
                            })}
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

                let name = character_name.get();
                let avatar = character_avatar.get();
                let creator = character_creator.get();
                let creator_label = if creator.is_empty() { String::new() } else { format!("by {}", creator) };

                // Memory panel
                let memory_open = RwSignal::new(false);

                view! {
                    <div class="chat">
                        <div class="chat__topbar">
                            <button class="chat__back" on:click=move |_| page.set(Page::Home)>
                                "\u{2190}"
                            </button>
                            <img class="chat__avatar" src=avatar alt="" />
                            <div class="chat__title">
                                <span class="chat__name">{name}</span>
                                <span class="chat__creator">{creator_label}</span>
                            </div>
                            <button class="chat__model" on:click=move |_| settings_open.set(true)>
                                {model_label}
                            </button>
                            <div class="chat__menuwrap">
                                <button class="chat__menubtn" on:click=move |_| memory_open.update(|v| *v = !*v)>
                                    "\u{2630}"
                                </button>
                                {move || memory_open.get().then(|| view! {
                                    <>
                                        <div class="menu-backdrop" on:click=move |_| memory_open.set(false)></div>
                                        <div class="chat__menu">
                                            <button on:click=move |_| { settings_open.set(true); memory_open.set(false); }>"\u{2699} API Settings"</button>
                                            <button on:click=move |_| { persona_open.set(true); memory_open.set(false); }>"\u{1F464} Persona"</button>
                                        </div>
                                    </>
                                })}
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
                                    let do_send = do_send.clone();
                                    move |ev| {
                                        if ev.key() == "Enter" && !ev.shift_key() {
                                            ev.prevent_default();
                                            do_send();
                                        }
                                    }
                                }
                            ></textarea>
                            <button class="chat__send" prop:disabled=move || sending.get() on:click=move |_| do_send()>
                                "Send"
                            </button>
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
