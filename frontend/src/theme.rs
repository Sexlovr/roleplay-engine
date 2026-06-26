//! Color themes. Each theme is a CSS palette selected via a `data-theme`
//! attribute on the document element (see the `[data-theme="…"]` blocks in
//! `style.css`). The active theme is held in a `ThemeCtx` signal and persisted
//! to `localStorage` so it survives reloads (per-device preference).

use leptos::prelude::*;

/// The active theme id (e.g. "iris", "ocean"). Provided in context at startup.
#[derive(Copy, Clone)]
pub struct ThemeCtx(pub RwSignal<String>);

/// `(id, label, swatch gradient)` — the gradient is shown on the picker swatch
/// and mirrors the theme's `--grad` in `style.css`.
pub const THEMES: &[(&str, &str, &str)] = &[
    ("iris", "Iris", "linear-gradient(135deg,#8b7bff,#ec5cc0)"),
    ("ocean", "Ocean", "linear-gradient(135deg,#4f7fd6,#7aa6ff)"),
    ("azure", "Azure", "linear-gradient(135deg,#2f6bff,#5b93ff)"),
    ("crimson", "Crimson", "linear-gradient(135deg,#8c1a36,#e0556e)"),
    ("ember", "Ember", "linear-gradient(135deg,#e0662a,#ffb14d)"),
    ("emerald", "Emerald", "linear-gradient(135deg,#1f9d6b,#4fd6a0)"),
    ("slate", "Slate", "linear-gradient(135deg,#6f7488,#c4c9d8)"),
];

/// Set `data-theme` on `<html>` so the CSS palette applies immediately.
pub fn apply_theme(id: &str) {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.document_element())
    {
        let _ = el.set_attribute("data-theme", id);
    }
}

/// Read the saved theme from localStorage, falling back to "iris". An unknown
/// id (e.g. removed theme) also falls back, so the attribute is always valid.
pub fn load_theme() -> String {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item("rp_theme").ok().flatten())
        .filter(|t| THEMES.iter().any(|(id, _, _)| id == t))
        .unwrap_or_else(|| "iris".to_string())
}

fn save_theme(id: &str) {
    if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = s.set_item("rp_theme", id);
    }
}

/// Whether streaming (token-by-token) replies are enabled. Held in a `StreamCtx`
/// signal and persisted to localStorage; defaults to on.
#[derive(Copy, Clone)]
pub struct StreamCtx(pub RwSignal<bool>);

/// Read the saved streaming preference (default on). Stored as `"off"`/`"on"`.
pub fn load_stream() -> bool {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item("rp_stream").ok().flatten())
        .map(|v| v != "off")
        .unwrap_or(true)
}

/// Persist the streaming preference.
pub fn save_stream(on: bool) {
    if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = s.set_item("rp_stream", if on { "on" } else { "off" });
    }
}

/// A compact row of color swatches; clicking one applies + persists the theme.
#[component]
pub fn ThemePicker() -> impl IntoView {
    let theme = use_context::<ThemeCtx>().unwrap().0;
    view! {
        <div class="theme-swatches" role="group" aria-label="Theme">
            {THEMES
                .iter()
                .map(|(id, label, grad)| {
                    let id = *id;
                    view! {
                        <button
                            class="theme-swatch"
                            class=("theme-swatch--active", move || theme.get() == id)
                            title=*label
                            aria-label=*label
                            style=format!("background:{grad}")
                            on:click=move |_| {
                                theme.set(id.to_string());
                                apply_theme(id);
                                save_theme(id);
                            }
                        ></button>
                    }
                })
                .collect_view()}
        </div>
    }
}
