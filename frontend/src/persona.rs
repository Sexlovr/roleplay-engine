//! Persona manager: who *you* are in the roleplay. Supports several saved
//! personas (JAI-style) with one active at a time; the active persona is
//! injected into every chat's system prompt. A slide-in drawer reusing the API
//! Settings drawer's styles.

use leptos::prelude::*;
use shared::dto::SettingsReq;
use shared::types::{Persona, PersonaStore};
use wasm_bindgen_futures::spawn_local;

use crate::api;

#[component]
pub fn PersonaEditor() -> impl IntoView {
    let persona_ctx = use_context::<crate::PersonaCtx>().unwrap().0;
    let open = use_context::<crate::PersonaOpen>().unwrap().0;

    let personas: RwSignal<Vec<Persona>> = RwSignal::new(Vec::new());
    let active = RwSignal::new(0i64);
    let saving = RwSignal::new(false);
    let error: RwSignal<Option<String>> = RwSignal::new(None);

    // Load fresh whenever the drawer opens.
    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        error.set(None);
        spawn_local(async move {
            if let Ok(s) = api::get_settings().await {
                let mut store = s.personas;
                if store.personas.is_empty() {
                    store = PersonaStore {
                        personas: vec![Persona { id: 1, ..Default::default() }],
                        active: 1,
                    };
                }
                active.set(if store.personas.iter().any(|p| p.id == store.active) {
                    store.active
                } else {
                    store.personas.first().map(|p| p.id).unwrap_or(0)
                });
                personas.set(store.personas);
            }
        });
    });

    let next_id = move || personas.with_untracked(|v| v.iter().map(|p| p.id).max().unwrap_or(0)) + 1;

    let add_persona = move |_| {
        let id = next_id();
        personas.update(|v| {
            v.push(Persona { id, name: format!("Persona {}", v.len() + 1), ..Default::default() });
        });
        active.set(id);
    };

    let delete_active = move |_| {
        let aid = active.get_untracked();
        personas.update(|v| v.retain(|p| p.id != aid));
        let first = personas.with_untracked(|v| v.first().map(|p| p.id).unwrap_or(0));
        active.set(first);
    };

    let save_it = move |_| {
        if saving.get_untracked() {
            return;
        }
        let store = PersonaStore { personas: personas.get_untracked(), active: active.get_untracked() };
        saving.set(true);
        // Reflect the active persona into context immediately.
        if let Some(p) = store.personas.iter().find(|p| p.id == store.active) {
            persona_ctx.set(p.clone());
        }
        spawn_local(async move {
            let req = SettingsReq { proxy: None, personas: Some(store) };
            match api::put_settings(&req).await {
                Ok(()) => {
                    saving.set(false);
                    open.set(false);
                }
                Err(e) => {
                    error.set(Some(format!("Couldn't save: {e}")));
                    saving.set(false);
                }
            }
        });
    };

    let persona_chips = move || {
        let aid = active.get();
        personas
            .get()
            .into_iter()
            .map(|p| {
                let id = p.id;
                let label = if p.name.trim().is_empty() { "Unnamed".to_string() } else { p.name.clone() };
                view! {
                    <button class="cfg-chip" class=("cfg-chip--active", move || id == aid)
                        on:click=move |_| active.set(id)>
                        {label}
                    </button>
                }
            })
            .collect_view()
    };

    let name_get = move || {
        let aid = active.get();
        personas.with(|v| v.iter().find(|p| p.id == aid).map(|p| p.name.clone()).unwrap_or_default())
    };
    let desc_get = move || {
        let aid = active.get();
        personas.with(|v| v.iter().find(|p| p.id == aid).map(|p| p.description.clone()).unwrap_or_default())
    };

    view! {
        <div class="settings-backdrop" on:click=move |_| open.set(false)></div>
        <aside class="settings-panel">
            <div class="settings-panel__hdr">
                <span>"\u{1F464} Personas"</span>
                <button class="settings-close" on:click=move |_| open.set(false)>"\u{2715}"</button>
            </div>

            <div class="settings-body">
                <div class="field-hint">
                    "Create one or more personas — who YOU are in the roleplay. The active one "
                    "is injected into every chat so characters know who they're talking to."
                </div>

                <div class="cfg-row">
                    {persona_chips}
                    <button class="cfg-add" on:click=add_persona title="Add persona">"+"</button>
                </div>

                <label class="settings-row">
                    <span>"Name"</span>
                    <input class="field" placeholder="e.g. Alex"
                        prop:value=name_get
                        on:input=move |ev| {
                            let val = event_target_value(&ev);
                            let aid = active.get_untracked();
                            personas.update(|v| { if let Some(p)=v.iter_mut().find(|p| p.id==aid){ p.name=val.clone(); } });
                        } />
                </label>

                <label class="settings-row">
                    <span>"About you"<small>" — appearance, personality, anything characters should know"</small></span>
                    <textarea class="field field--code" rows="6"
                        prop:value=desc_get
                        on:input=move |ev| {
                            let val = event_target_value(&ev);
                            let aid = active.get_untracked();
                            personas.update(|v| { if let Some(p)=v.iter_mut().find(|p| p.id==aid){ p.description=val.clone(); } });
                        } ></textarea>
                </label>

                <button class="btn cfg-delete" on:click=delete_active>"\u{1F5D1} Delete this persona"</button>
            </div>

            <div class="settings-actions">
                {move || error.get().map(|msg| view! { <div class="settings-error" role="alert">{msg}</div> })}
                <button class="btn" on:click=move |_| open.set(false)>"Cancel"</button>
                <button class="btn btn--login" prop:disabled=move || saving.get() on:click=save_it>
                    {move || if saving.get() { "Saving\u{2026}" } else { "Save & activate" }}
                </button>
            </div>
        </aside>
    }
}
