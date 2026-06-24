//! API Settings drawer — manage several proxy configurations (JAI-style
//! "+ Add Configuration") and pick which one is active. The selected config is
//! the active one used for chats. Keys never come back from the server, so a
//! blank key on save means "keep the saved one".

use leptos::prelude::*;
use shared::dto::SettingsReq;
use shared::template::{self, ProxyConfig, ProxyStore};
use wasm_bindgen_futures::spawn_local;

use crate::api;

fn headers_to_text(h: &[(String, String)]) -> String {
    h.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join("\n")
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
    let has_key_ctx = use_context::<crate::HasApiKey>().unwrap().0;
    let open = use_context::<crate::SettingsOpen>().unwrap().0;

    let configs: RwSignal<Vec<ProxyConfig>> = RwSignal::new(Vec::new());
    let active = RwSignal::new(0i64);
    let has_keys: RwSignal<Vec<i64>> = RwSignal::new(Vec::new());
    let saving = RwSignal::new(false);
    let show_advanced = RwSignal::new(false);
    let save_error: RwSignal<Option<String>> = RwSignal::new(None);
    let loaded = RwSignal::new(false);

    // Load the store fresh each time the drawer opens.
    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        loaded.set(false);
        show_advanced.set(false);
        save_error.set(None);
        spawn_local(async move {
            match api::get_settings().await {
                Ok(s) => {
                    let mut store = s.proxy;
                    if store.configs.is_empty() {
                        let mut c = ProxyConfig::openai();
                        c.id = 1;
                        c.name = "My Proxy".into();
                        store = ProxyStore { configs: vec![c], active: 1 };
                    }
                    active.set(store.active_config().map(|c| c.id).unwrap_or(0));
                    configs.set(store.configs);
                    has_keys.set(s.proxy_has_key);
                    loaded.set(true);
                }
                Err(e) => {
                    save_error.set(Some(format!("Couldn't load settings: {e}")));
                    loaded.set(true);
                }
            }
        });
    });

    let next_id =
        move || configs.with_untracked(|v| v.iter().map(|c| c.id).max().unwrap_or(0)) + 1;

    let add_config = move |_| {
        let id = next_id();
        configs.update(|v| {
            let mut c = ProxyConfig::openai();
            c.id = id;
            c.name = format!("Config {}", v.len() + 1);
            v.push(c);
        });
        active.set(id);
        show_advanced.set(true);
    };

    let delete_active = move |_| {
        let aid = active.get_untracked();
        configs.update(|v| {
            v.retain(|c| c.id != aid);
            // Never leave an empty store: re-seed a default so the form stays
            // editable (matching the load-time seeding) instead of going blank.
            if v.is_empty() {
                let mut c = ProxyConfig::openai();
                c.id = 1;
                c.name = "My Proxy".into();
                v.push(c);
            }
        });
        let first = configs.with_untracked(|v| v.first().map(|c| c.id).unwrap_or(1));
        active.set(first);
    };

    // Apply a preset's templating fields to the active config (keeps id/name/key).
    let apply_preset = move |preset: ProxyConfig| {
        let aid = active.get_untracked();
        configs.update(|v| {
            if let Some(c) = v.iter_mut().find(|c| c.id == aid) {
                c.url = preset.url;
                c.headers = preset.headers;
                c.body_template = preset.body_template;
                c.response_path = preset.response_path;
            }
        });
    };

    let save = move |_| {
        if saving.get_untracked() {
            return;
        }
        let store = ProxyStore { configs: configs.get_untracked(), active: active.get_untracked() };
        saving.set(true);
        save_error.set(None);
        spawn_local(async move {
            let req = SettingsReq { proxy: Some(store), personas: None };
            match api::put_settings(&req).await {
                Ok(()) => match api::get_settings().await {
                    Ok(s) => {
                        if let Some(a) = s.proxy.active_config() {
                            cfg_sig.set(a.clone());
                            has_key_ctx.set(s.proxy_has_key.contains(&a.id));
                        }
                        saving.set(false);
                        open.set(false);
                    }
                    Err(_) => {
                        saving.set(false);
                        open.set(false);
                    }
                },
                Err(e) => {
                    save_error.set(Some(format!("Couldn't save: {e}")));
                    saving.set(false);
                }
            }
        });
    };

    // --- per-field helpers bound to the active config ---
    macro_rules! field_get {
        ($getter:expr) => {
            move || {
                let aid = active.get();
                configs.with(|v| v.iter().find(|c| c.id == aid).map($getter).unwrap_or_default())
            }
        };
    }
    macro_rules! field_set {
        ($setter:expr) => {
            move |ev: leptos::ev::Event| {
                let val = event_target_value(&ev);
                let aid = active.get_untracked();
                configs.update(|v| {
                    if let Some(c) = v.iter_mut().find(|c| c.id == aid) {
                        let f: fn(&mut ProxyConfig, String) = $setter;
                        f(c, val);
                    }
                });
            }
        };
    }

    let config_chips = move || {
        let aid = active.get();
        configs
            .get()
            .into_iter()
            .map(|c| {
                let id = c.id;
                let label = if c.name.trim().is_empty() { "Untitled".to_string() } else { c.name.clone() };
                view! {
                    <button class="cfg-chip" class=("cfg-chip--active", move || id == aid)
                        on:click=move |_| active.set(id)>
                        {label}
                    </button>
                }
            })
            .collect_view()
    };

    let preset_buttons = template::presets()
        .into_iter()
        .map(|p| {
            let label = p.name.clone();
            let ap = apply_preset;
            view! {
                <button class="preset-chip" on:click=move |_| ap(p.clone())>{label}</button>
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
                    "Add one or more endpoints and pick which is active. Keys stay on the server."
                </div>

                // ---- config selector ----
                <div class="cfg-row">
                    {config_chips}
                    <button class="cfg-add" on:click=add_config title="Add configuration">"+"</button>
                </div>

                {move || (!loaded.get()).then(|| view! { <div class="field-hint">"Loading\u{2026}"</div> })}

                // ---- presets ----
                <label class="settings-row">
                    <span>"Preset"<small>" — fills the template below"</small></span>
                    <div class="preset-row">{preset_buttons}</div>
                </label>

                // ---- name ----
                <label class="settings-row">
                    <span>"Config name"</span>
                    <input class="field" type="text" placeholder="My Proxy"
                        prop:value=field_get!(|c| c.name.clone())
                        on:input=field_set!(|c, v| c.name = v) />
                </label>

                // ---- URL ----
                <label class="settings-row">
                    <span>"Endpoint URL"</span>
                    <input class="field" type="text"
                        placeholder="https://your-proxy.example/v1/chat/completions"
                        prop:value=field_get!(|c| c.url.clone())
                        on:input=field_set!(|c, v| c.url = v) />
                </label>

                // ---- model ----
                <label class="settings-row">
                    <span>"Model"</span>
                    <input class="field" type="text" placeholder="gpt-4o-mini / claude-3-opus / your-model"
                        prop:value=field_get!(|c| c.model.clone())
                        on:input=field_set!(|c, v| c.model = v) />
                </label>

                // ---- API Key ----
                <label class="settings-row">
                    <span>"API Key"</span>
                    <input class="field" type="password"
                        placeholder=move || {
                            let aid = active.get();
                            if has_keys.get().contains(&aid) { "(saved — type to replace)".to_string() }
                            else { "sk-...".to_string() }
                        }
                        prop:value=field_get!(|c| c.api_key.clone())
                        on:input=field_set!(|c, v| c.api_key = v) />
                </label>

                // ---- multi-key ----
                <label class="settings-row settings-row--check">
                    <input type="checkbox"
                        prop:checked=field_get!(|c| c.multi_key)
                        on:change=move |ev| {
                            let on = event_target_checked(&ev);
                            let aid = active.get_untracked();
                            configs.update(|v| { if let Some(c)=v.iter_mut().find(|c| c.id==aid){ c.multi_key=on; } });
                        } />
                    <span>"Multiple keys (comma-separated, rotated per request)"</span>
                </label>

                // ---- custom system prompt (JAI "Custom Prompt") ----
                <label class="settings-row">
                    <span>"Custom prompt"<small>" — prepended to every chat's system prompt"</small></span>
                    <textarea class="field field--code" rows="4"
                        placeholder="e.g. [System: respond in vivid third-person, 2-3 paragraphs.]"
                        prop:value=field_get!(|c| c.system_prompt.clone())
                        on:input=field_set!(|c, v| c.system_prompt = v) ></textarea>
                </label>

                // ---- advanced toggle ----
                <button class="advanced-toggle" on:click=move |_| show_advanced.update(|v| *v = !*v)>
                    {move || if show_advanced.get() { "\u{25BE} Advanced" } else { "\u{25B8} Advanced" }}
                </button>

                {move || show_advanced.get().then(|| view! {
                    <>
                    <div class="settings-row settings-row--split">
                        <label>
                            <span>"Temperature"</span>
                            <input class="field" type="number" step="0.1" min="0" max="2"
                                prop:value=field_get!(|c| c.temperature.to_string())
                                on:input=move |ev| {
                                    // Fall back instead of dropping the event, so the
                                    // signal always tracks the box (no desync on blank).
                                    let val = event_target_value(&ev).parse::<f32>().unwrap_or(0.0).clamp(0.0, 2.0);
                                    let aid = active.get_untracked();
                                    configs.update(|v| { if let Some(c)=v.iter_mut().find(|c| c.id==aid){ c.temperature=val; } });
                                } />
                        </label>
                        <label>
                            <span>"Max Tokens"<small>" — 0 = model default"</small></span>
                            <input class="field" type="number" min="0" step="1"
                                prop:value=field_get!(|c| c.max_tokens.to_string())
                                on:input=move |ev| {
                                    // Empty/invalid → 0 ("model default"), keeping signal & box in sync.
                                    let val = event_target_value(&ev).parse::<u32>().unwrap_or(0);
                                    let aid = active.get_untracked();
                                    configs.update(|v| { if let Some(c)=v.iter_mut().find(|c| c.id==aid){ c.max_tokens=val; } });
                                } />
                        </label>
                    </div>

                    <label class="settings-row">
                        <span>"Context Window"<small>" — 0 = unlimited (no truncation)"</small></span>
                        <input class="field" type="number" min="0" step="100" placeholder="0"
                            prop:value=field_get!(|c| c.context_tokens.to_string())
                            on:input=move |ev| {
                                // Empty/invalid → 0 ("unlimited"), keeping signal & box in sync.
                                let val = event_target_value(&ev).parse::<i64>().unwrap_or(0).max(0);
                                let aid = active.get_untracked();
                                configs.update(|v| { if let Some(c)=v.iter_mut().find(|c| c.id==aid){ c.context_tokens=val; } });
                            } />
                    </label>

                    <label class="settings-row">
                        <span>"Headers"<small>" — one per line, Key: Value"</small></span>
                        <textarea class="field field--code" rows="3"
                            prop:value=move || {
                                let aid = active.get();
                                configs.with(|v| v.iter().find(|c| c.id==aid).map(|c| headers_to_text(&c.headers)).unwrap_or_default())
                            }
                            on:input=move |ev| {
                                let h = text_to_headers(&event_target_value(&ev));
                                let aid = active.get_untracked();
                                configs.update(|v| { if let Some(c)=v.iter_mut().find(|c| c.id==aid){ c.headers=h.clone(); } });
                            } ></textarea>
                    </label>

                    <label class="settings-row">
                        <span>"Request Body Template"
                            <small>" — {{model}} {{messages}} {{messages_system}} {{system}} {{prompt}} {{temperature}} {{max_tokens}} {{context_tokens}} {{api_key}}"</small>
                        </span>
                        <textarea class="field field--code" rows="8"
                            prop:value=field_get!(|c| c.body_template.clone())
                            on:input=field_set!(|c, v| c.body_template = v) ></textarea>
                    </label>

                    <label class="settings-row">
                        <span>"Response Path"<small>" — dot/index path to the reply text"</small></span>
                        <input class="field field--code" placeholder="choices.0.message.content"
                            prop:value=field_get!(|c| c.response_path.clone())
                            on:input=field_set!(|c, v| c.response_path = v) />
                    </label>

                    <button class="btn cfg-delete" on:click=delete_active>"\u{1F5D1} Delete this config"</button>
                    </>
                })}
            </div>

            <div class="settings-actions">
                {move || save_error.get().map(|msg| view! {
                    <div class="settings-error" role="alert">{msg}</div>
                })}
                <button class="btn" on:click=move |_| open.set(false)>"Cancel"</button>
                <button class="btn btn--login" prop:disabled=move || saving.get() on:click=save>
                    {move || if saving.get() { "Saving\u{2026}" } else { "Save & activate" }}
                </button>
            </div>
        </aside>
    }
}
