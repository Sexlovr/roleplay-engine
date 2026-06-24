//! Settings: a store of proxy configs (JAI-style "+ Add Configuration") and a
//! store of personas, each with an `active` selection. Both are persisted as a
//! single JSON `settings` row. API keys are kept server-side and never leave in
//! a GET response; on save, a blank key for a known config id is treated as
//! "keep the existing key".

use axum::extract::State;
use axum::Json;
use rusqlite::OptionalExtension;

use shared::dto::{SettingsReq, SettingsResp};
use shared::template::{ProxyConfig, ProxyStore};
use shared::types::{Persona, PersonaStore};

use crate::error::AppError;
use crate::state::AppState;

const PROXY_STORE_KEY: &str = "proxy_store";
const LEGACY_PROXY_KEY: &str = "proxy_config";
const PERSONA_STORE_KEY: &str = "persona_store";
const LEGACY_PERSONA_KEY: &str = "persona";

type Pool = r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>;

/// GET /api/settings — proxy store (keys blanked) + which ids have a key + personas.
pub async fn get_settings(State(state): State<AppState>) -> Result<Json<SettingsResp>, AppError> {
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || -> Result<Json<SettingsResp>, AppError> {
        let store = load_proxy_store(&pool);
        let personas = load_persona_store(&pool);
        // Which config ids currently have a non-empty key.
        let proxy_has_key: Vec<i64> = store
            .configs
            .iter()
            .filter(|c| !c.api_key.trim().is_empty())
            .map(|c| c.id)
            .collect();
        // Blank every key before returning.
        let mut safe = store;
        for c in safe.configs.iter_mut() {
            c.api_key = String::new();
        }
        Ok(Json(SettingsResp { proxy: safe, proxy_has_key, personas }))
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

/// PUT /api/settings — replace the proxy store and/or persona store.
pub async fn put_settings(
    State(state): State<AppState>,
    Json(body): Json<SettingsReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || -> Result<Json<serde_json::Value>, AppError> {
        if let Some(mut incoming) = body.proxy {
            let existing = load_proxy_store(&pool);
            normalize_proxy_store(&mut incoming, &existing);
            let conn = pool.get()?;
            let json = serde_json::to_string(&incoming)?;
            conn.execute(
                "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
                rusqlite::params![PROXY_STORE_KEY, json],
            )?;
        }
        if let Some(personas) = body.personas {
            let conn = pool.get()?;
            let json = serde_json::to_string(&personas)?;
            conn.execute(
                "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
                rusqlite::params![PERSONA_STORE_KEY, json],
            )?;
        }
        Ok(Json(serde_json::json!({"ok": true})))
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

/// Clean up an incoming proxy store before persisting:
/// - assign ids to new configs (id == 0),
/// - preserve the saved api_key when the incoming one is blank for a known id,
/// - collapse comma lists to a single key when multi_key is off,
/// - default to OpenAI body/headers when a URL is set but the body is blank,
/// - clamp `active` to a real config.
fn normalize_proxy_store(incoming: &mut ProxyStore, existing: &ProxyStore) {
    let mut next_id = incoming.max_id().max(existing.max_id());
    for cfg in incoming.configs.iter_mut() {
        if cfg.id == 0 {
            next_id += 1;
            cfg.id = next_id;
        }
        // Did the client actually supply a key this save? Captured BEFORE the
        // preserve step so a multi-key list resurrected from storage is never
        // treated as freshly-typed (and thus never collapsed below).
        let client_sent_key = !cfg.api_key.trim().is_empty();
        // Preserve the stored key when the client sends a blank one (it never
        // sees the real key, so it can't echo it back).
        if cfg.api_key.trim().is_empty() {
            if let Some(old) = existing.configs.iter().find(|c| c.id == cfg.id) {
                cfg.api_key = old.api_key.clone();
            }
        }
        // Collapse only a comma list the user actually typed while multi_key is
        // off — never truncate a preserved list (that would silently lose keys).
        // At request time, resolve_key already picks one key when multi_key is off.
        if client_sent_key && !cfg.multi_key && cfg.api_key.contains(',') {
            if let Some(first) = cfg.api_key.split(',').map(str::trim).find(|s| !s.is_empty()) {
                cfg.api_key = first.to_string();
            }
        }
        // URL set but no body template → assume OpenAI-compatible.
        if !cfg.url.trim().is_empty() && cfg.body_template.trim().is_empty() {
            let openai = ProxyConfig::openai();
            cfg.body_template = openai.body_template;
            if cfg.response_path.trim().is_empty() {
                cfg.response_path = openai.response_path;
            }
            if cfg.headers.is_empty() {
                cfg.headers = openai.headers;
            }
        }
    }
    // Clamp active to an existing config (fall back to the first id).
    if !incoming.configs.iter().any(|c| c.id == incoming.active) {
        incoming.active = incoming.configs.first().map(|c| c.id).unwrap_or(0);
    }
}

// --- loaders (also used by llm.rs / chats.rs) -------------------------------

/// Load the proxy store, migrating a legacy single `proxy_config` row if needed.
pub fn load_proxy_store(pool: &Pool) -> ProxyStore {
    let Ok(conn) = pool.get() else {
        return ProxyStore::default();
    };
    // Preferred: the new store row.
    if let Ok(Some(raw)) = conn
        .query_row(
            "SELECT value FROM settings WHERE key=?1",
            [PROXY_STORE_KEY],
            |r| r.get::<_, String>(0),
        )
        .optional()
    {
        if let Ok(store) = serde_json::from_str::<ProxyStore>(&raw) {
            return store;
        }
    }
    // Fallback: migrate the old single config into a one-entry store.
    if let Ok(Some(raw)) = conn
        .query_row(
            "SELECT value FROM settings WHERE key=?1",
            [LEGACY_PROXY_KEY],
            |r| r.get::<_, String>(0),
        )
        .optional()
    {
        if let Ok(mut cfg) = serde_json::from_str::<ProxyConfig>(&raw) {
            cfg.id = 1;
            return ProxyStore { configs: vec![cfg], active: 1 };
        }
    }
    ProxyStore::default()
}

/// The active proxy config (or default if none configured).
pub fn load_active_proxy(pool: &Pool) -> ProxyConfig {
    let store = load_proxy_store(pool);
    store.active_config().cloned().unwrap_or_default()
}

/// Load the persona store, migrating a legacy single `persona` row if needed.
pub fn load_persona_store(pool: &Pool) -> PersonaStore {
    let Ok(conn) = pool.get() else {
        return PersonaStore::default();
    };
    if let Ok(Some(raw)) = conn
        .query_row(
            "SELECT value FROM settings WHERE key=?1",
            [PERSONA_STORE_KEY],
            |r| r.get::<_, String>(0),
        )
        .optional()
    {
        if let Ok(store) = serde_json::from_str::<PersonaStore>(&raw) {
            return store;
        }
    }
    // Migrate the legacy single persona.
    if let Ok(Some(raw)) = conn
        .query_row(
            "SELECT value FROM settings WHERE key=?1",
            [LEGACY_PERSONA_KEY],
            |r| r.get::<_, String>(0),
        )
        .optional()
    {
        if let Ok(mut p) = serde_json::from_str::<Persona>(&raw) {
            if !p.name.trim().is_empty() || !p.description.trim().is_empty() {
                p.id = 1;
                return PersonaStore { personas: vec![p], active: 1 };
            }
        }
    }
    PersonaStore::default()
}

/// The active persona (empty default if none).
pub fn load_active_persona(pool: &Pool) -> Persona {
    let store = load_persona_store(pool);
    store
        .personas
        .iter()
        .find(|p| p.id == store.active)
        .or_else(|| store.personas.first())
        .cloned()
        .unwrap_or_default()
}
