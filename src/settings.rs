//! API Settings drawer: configure the chat connector (see [`crate::api`]).
//!
//! Edits a working copy of the active [`ProxyConfig`]; on Save it persists to
//! localStorage and updates the global config signal. Preset buttons fill the
//! template fields for common providers without locking the user in.

use leptos::prelude::*;

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
    let open = use_context::<crate::SettingsOpen>().unwrap().0;

    // Single working draft, seeded from the live config when this mounts.
    let draft = RwSignal::new(cfg_sig.get_untracked());

    let save = move |_| {
        let cfg = draft.get();
        api::save(&cfg);
        cfg_sig.set(cfg);
        open.set(false);
    };

    let preset_buttons = api::presets()
        .into_iter()
        .map(|p| {
            let label = p.name.clone();
            view! {
                <button
                    class="preset-chip"
                    on:click=move |_| draft.update(|d| {
                        let key = d.api_key.clone(); // keep the user's secret
                        *d = p.clone();
                        d.api_key = key;
                    })
                >
                    {label}
                </button>
            }
        })
        .collect_view();

    view! {
        <div class="settings-backdrop" on:click=move |_| open.set(false)></div>
        <aside class="settings-panel" on:click=|ev| ev.stop_propagation()>
            <div class="settings-panel__hdr">
                <span>"\u{2699} API Settings"</span>
                <button class="settings-close" on:click=move |_| open.set(false)>"\u{2715}"</button>
            </div>

            <div class="settings-body">
                <div class="field-hint">
                    "Bring your own endpoint — any provider. Pick a preset, then edit. "
                    "The endpoint must allow CORS from this origin."
                </div>

                <label class="settings-row">
                    <span>"Preset"</span>
                    <div class="preset-row">{preset_buttons}</div>
                </label>

                <label class="settings-row">
                    <span>"Name"</span>
                    <input class="field" prop:value=move || draft.with(|d| d.name.clone())
                        on:input=move |ev| draft.update(|d| d.name = event_target_value(&ev)) />
                </label>

                <label class="settings-row">
                    <span>"Endpoint URL"</span>
                    <input class="field" placeholder="https://your-proxy.example/v1/chat/completions"
                        prop:value=move || draft.with(|d| d.url.clone())
                        on:input=move |ev| draft.update(|d| d.url = event_target_value(&ev)) />
                </label>

                <label class="settings-row">
                    <span>"API Key"</span>
                    <input class="field" type="password" placeholder="sk-... (optional)"
                        prop:value=move || draft.with(|d| d.api_key.clone())
                        on:input=move |ev| draft.update(|d| d.api_key = event_target_value(&ev)) />
                </label>

                <label class="settings-row">
                    <span>"Model"</span>
                    <input class="field" placeholder="gpt-4o-mini / claude-... / your-model"
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
                        <small>" — {{model}} {{messages}} {{messages_system}} {{system}} {{prompt}} {{temperature}} {{max_tokens}} {{api_key}}"</small>
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
            </div>

            <div class="settings-actions">
                <button class="btn" on:click=move |_| open.set(false)>"Cancel"</button>
                <button class="btn btn--login" on:click=save>"Save"</button>
            </div>
        </aside>
    }
}
