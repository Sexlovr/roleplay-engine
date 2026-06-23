//! API Settings drawer: configure the chat connector (provider-agnostic).
//!
//! Default view shows only the essentials (preset, URL, key, multi-key toggle).
//! An "Advanced" disclosure reveals model, temperature, max tokens, context
//! window, headers, body template, and response path.
//!
//! Edits a working copy of the active [`ProxyConfig`]; on Save it PUTs to the
//! server. Preset buttons fill the template fields without changing the API key
//! (the key is never visible after saving — the server only returns a flag).

use leptos::prelude::*;
use shared::dto::SettingsReq;
use shared::template;
use wasm_bindgen_futures::spawn_local;

use crate::api;

fn headers_to_text(h: &[(String, String)]) -> String {
    h.iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn text_to_headers(s: &str) -> Vec<(String, String)> {
    s.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let (k, v) = line.split_once(':')?;
            Some((k.trim().to_string(), v.trim().to_string()))
        })
        .collect()
}

#[component]
pub fn Settings() -> impl IntoView {
    let cfg_sig = use_context::<crate::ApiConfig>().unwrap().0;
    let has_key = use_context::<crate::HasApiKey>().unwrap().0;
    let open = use_context::<crate::SettingsOpen>().unwrap().0;

    // Working draft, seeded from the live config when the drawer opens.
    let draft = RwSignal::new(cfg_sig.get_untracked());
    let saving = RwSignal::new(false);
    let show_advanced = RwSignal::new(false);
    let save_error: RwSignal<Option<String>> = RwSignal::new(None);

    // Re-seed draft each time the drawer opens so it reflects the latest config.
    Effect::new(move |_| {
        if open.get() {
            draft.set(cfg_sig.get_untracked());
            show_advanced.set(false);
            save_error.set(None);
        }
    });

    let save = move |_| {
        if saving.get_untracked() {
            return;
        }
        let cfg = draft.get_untracked();
        let prev = cfg_sig.get_untracked();
        // The backend applies OpenAI-style defaults when only URL + key are set.
        saving.set(true);
        save_error.set(None);
        cfg_sig.set(cfg.clone());
        spawn_local(async move {
            let req = SettingsReq { proxy: Some(cfg), persona: None };
            match api::put_settings(&req).await {
                Ok(()) => {
                    // Reload to pick up the server's response (with defaults applied).
                    if let Ok(s) = api::get_settings().await {
                        cfg_sig.set(s.proxy);
                    }
                    has_key.set(true);
                    saving.set(false);
                    open.set(false); // close only on success
                }
                Err(e) => {
                    cfg_sig.set(prev); // revert optimistic update
                    save_error.set(Some(format!("Couldn't save: {e}")));
                    saving.set(false);
                    // Keep the drawer open so the user sees the error and can retry.
                }
            }
        });
    };

    let preset_buttons = template::presets()
        .into_iter()
        .map(|p| {
            let label = p.name.clone();
            view! {
                <button
                    class="preset-chip"
                    on:click=move |_| draft.update(|d| {
                        let key = d.api_key.clone();
                        let multi = d.multi_key;
                        *d = p.clone();
                        d.api_key = key;
                        d.multi_key = multi;
                    })
                >
                    {label}
                </button>
            }
        })
        .collect_view();

    view! {
        <div class="settings-backdrop" on:click=move |_| open.set(false)></div>
        <aside class="settings-panel">
            <div class="settings-panel__hdr">
                <span>"\u{2699} API Settings"</span>
                <button class="settings-close" on:click=move |_| open.set(false)>"\u{2715}"</button>
            </div>

            <div class="settings-body">
                <div class="field-hint">
                    "Bring your own endpoint. Key stays on the server — never in the browser."
                </div>

                // ---- Presets ----
                <label class="settings-row">
                    <span>"Preset"</span>
                    <div class="preset-row">{preset_buttons}</div>
                </label>

                // ---- URL ----
                <label class="settings-row">
                    <span>"Endpoint URL"</span>
                    <input class="field" type="text"
                        placeholder="https://your-proxy.example/v1/chat/completions"
                        prop:value=move || draft.with(|d| d.url.clone())
                        on:input=move |ev| draft.update(|d| d.url = event_target_value(&ev)) />
                </label>

                // ---- API Key (no maxlength) ----
                <label class="settings-row">
                    <span>"API Key"</span>
                    <input class="field" type="password"
                        placeholder={move || if has_key.get() { "(saved — type to replace)" } else { "sk-..." }}
                        prop:value=move || draft.with(|d| d.api_key.clone())
                        on:input=move |ev| draft.update(|d| d.api_key = event_target_value(&ev)) />
                </label>

                // ---- Multiple keys toggle ----
                <label class="settings-row settings-row--check">
                    <input type="checkbox"
                        prop:checked=move || draft.with(|d| d.multi_key)
                        on:change=move |ev| draft.update(|d| d.multi_key = event_target_checked(&ev)) />
                    <span>"Multiple keys (comma-separated: key1,key2,…)"</span>
                </label>
                {move || draft.with(|d| d.multi_key).then(|| view! {
                    <div class="field-hint">
                        "The server will rotate between keys to spread requests across accounts. Useful for rate limits."
                    </div>
                })}

                // ---- Advanced toggle ----
                <button
                    class="advanced-toggle"
                    on:click=move |_| show_advanced.update(|v| *v = !*v)
                >
                    {move || if show_advanced.get() { "\u{25BE} Advanced" } else { "\u{25B8} Advanced" }}
                </button>

                // ---- Advanced section ----
                {move || show_advanced.get().then(|| view! {
                    <>
                    <label class="settings-row">
                        <span>"Name"<small>" — label shown in the UI"</small></span>
                        <input class="field" type="text" placeholder="My Proxy"
                            prop:value=move || draft.with(|d| d.name.clone())
                            on:input=move |ev| draft.update(|d| d.name = event_target_value(&ev)) />
                    </label>

                    <label class="settings-row">
                        <span>"Model"</span>
                        <input class="field" type="text" placeholder="gpt-4o-mini / claude-3-opus / your-model"
                            prop:value=move || draft.with(|d| d.model.clone())
                            on:input=move |ev| draft.update(|d| d.model = event_target_value(&ev)) />
                    </label>

                    <div class="settings-row settings-row--split">
                        <label>
                            <span>"Temperature"</span>
                            <input class="field" type="number" step="0.1" min="0" max="2"
                                prop:value=move || draft.with(|d| d.temperature.to_string())
                                on:input=move |ev| {
                                    if let Ok(v) = event_target_value(&ev).parse::<f32>() {
                                        draft.update(|d| d.temperature = v);
                                    }
                                } />
                        </label>
                        <label>
                            <span>"Max Tokens"</span>
                            <input class="field" type="number" min="1" step="1"
                                prop:value=move || draft.with(|d| d.max_tokens.to_string())
                                on:input=move |ev| {
                                    if let Ok(v) = event_target_value(&ev).parse::<u32>() {
                                        draft.update(|d| d.max_tokens = v);
                                    }
                                } />
                        </label>
                    </div>

                    <label class="settings-row">
                        <span>"Context Window"<small>" — 0 = unlimited (no truncation)"</small></span>
                        <input class="field" type="number" min="0" step="100" placeholder="0"
                            prop:value=move || draft.with(|d| d.context_tokens.to_string())
                            on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<i64>() {
                                    draft.update(|d| d.context_tokens = v.max(0));
                                }
                            } />
                    </label>

                    <label class="settings-row">
                        <span>"Headers"<small>" — one per line, Key: Value"</small></span>
                        <textarea class="field field--code" rows="3"
                            prop:value=move || draft.with(|d| headers_to_text(&d.headers))
                            on:input=move |ev| {
                                let h = text_to_headers(&event_target_value(&ev));
                                draft.update(|d| d.headers = h);
                            } ></textarea>
                    </label>

                    <label class="settings-row">
                        <span>"Request Body Template"
                            <small>" — {{model}} {{messages}} {{messages_system}} {{system}} {{prompt}} {{temperature}} {{max_tokens}} {{context_tokens}} {{api_key}}"</small>
                        </span>
                        <textarea class="field field--code" rows="8"
                            prop:value=move || draft.with(|d| d.body_template.clone())
                            on:input=move |ev| draft.update(|d| d.body_template = event_target_value(&ev)) ></textarea>
                    </label>

                    <label class="settings-row">
                        <span>"Response Path"<small>" — dot/index path to the reply text"</small></span>
                        <input class="field field--code" placeholder="choices.0.message.content"
                            prop:value=move || draft.with(|d| d.response_path.clone())
                            on:input=move |ev| draft.update(|d| d.response_path = event_target_value(&ev)) />
                    </label>
                    </>
                })}
            </div>

            <div class="settings-actions">
                {move || save_error.get().map(|msg| view! {
                    <div class="settings-error" role="alert">{msg}</div>
                })}
                <button class="btn" on:click=move |_| open.set(false)>"Cancel"</button>
                <button class="btn btn--login" prop:disabled=move || saving.get() on:click=save>
                    {move || if saving.get() { "Saving\u{2026}" } else { "Save" }}
                </button>
            </div>
        </aside>
    }
}
