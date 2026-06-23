//! Persona editor: who *you* are in the roleplay. Saved to the server and
//! injected into every chat's system prompt. A slide-in drawer reusing the API
//! Settings drawer's styles.

use leptos::prelude::*;
use shared::dto::SettingsReq;
use wasm_bindgen_futures::spawn_local;

use crate::api;

#[component]
pub fn PersonaEditor() -> impl IntoView {
    let persona = use_context::<crate::PersonaCtx>().unwrap().0;
    let open = use_context::<crate::PersonaOpen>().unwrap().0;

    let draft = RwSignal::new(persona.get_untracked());
    let saving = RwSignal::new(false);

    let save_it = move |_| {
        if saving.get_untracked() {
            return;
        }
        let p = draft.get();
        saving.set(true);
        persona.set(p.clone());
        spawn_local(async move {
            let req = SettingsReq { proxy: None, persona: Some(p) };
            let _ = api::put_settings(&req).await;
            saving.set(false);
            open.set(false);
        });
    };

    view! {
        <div class="settings-backdrop" on:click=move |_| open.set(false)></div>
        <aside class="settings-panel">
            <div class="settings-panel__hdr">
                <span>"\u{1F464} Persona"</span>
                <button class="settings-close" on:click=move |_| open.set(false)>"\u{2715}"</button>
            </div>

            <div class="settings-body">
                <div class="field-hint">
                    "This is who YOU are in the roleplay. It's injected into every chat so characters know who they're talking to."
                </div>

                <label class="settings-row">
                    <span>"Name"</span>
                    <input class="field" placeholder="e.g. Alex"
                        prop:value=move || draft.with(|d| d.name.clone())
                        on:input=move |ev| draft.update(|d| d.name = event_target_value(&ev)) />
                </label>

                <label class="settings-row">
                    <span>"About you"<small>" — appearance, personality, anything characters should know"</small></span>
                    <textarea class="field field--code" rows="5"
                        prop:value=move || draft.with(|d| d.description.clone())
                        on:input=move |ev| draft.update(|d| d.description = event_target_value(&ev)) ></textarea>
                </label>
            </div>

            <div class="settings-actions">
                <button class="btn" on:click=move |_| open.set(false)>"Cancel"</button>
                <button class="btn btn--login" prop:disabled=move || saving.get() on:click=save_it>
                    {move || if saving.get() { "Saving\u{2026}" } else { "Save" }}
                </button>
            </div>
        </aside>
    }
}
