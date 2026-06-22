//! Persona editor: who *you* are in the roleplay. Persisted to localStorage and
//! injected into every chat's system prompt. A slide-in drawer reusing the API
//! Settings drawer's styles.

use gloo_storage::{LocalStorage, Storage};
use leptos::prelude::*;

use crate::types::Persona;

const KEY: &str = "rp_persona";

pub fn load() -> Persona {
    LocalStorage::get(KEY).unwrap_or_default()
}
pub fn save(p: &Persona) {
    let _ = LocalStorage::set(KEY, p);
}

#[component]
pub fn PersonaEditor() -> impl IntoView {
    let persona = use_context::<crate::PersonaCtx>().unwrap().0;
    let open = use_context::<crate::PersonaOpen>().unwrap().0;

    let draft = RwSignal::new(persona.get_untracked());

    let save_it = move |_| {
        let p = draft.get();
        save(&p);
        persona.set(p);
        open.set(false);
    };

    view! {
        <div class="settings-backdrop" on:click=move |_| open.set(false)></div>
        <aside class="settings-panel" on:click=|ev| ev.stop_propagation()>
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
                <button class="btn btn--login" on:click=save_it>"Save"</button>
            </div>
        </aside>
    }
}
