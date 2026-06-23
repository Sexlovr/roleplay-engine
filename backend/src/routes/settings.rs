use axum::extract::State;
use axum::Json;

use shared::dto::{SettingsReq, SettingsResp};
use shared::template::ProxyConfig;
use shared::types::Persona;

use crate::error::AppError;
use crate::state::AppState;

/// GET /api/settings — returns proxy config (WITHOUT the api_key) + persona.
pub async fn get_settings(
    State(state): State<AppState>,
) -> Result<Json<SettingsResp>, AppError> {
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || -> Result<Json<SettingsResp>, AppError> {
        let persona = load_persona_blocking(&pool)?;
        let proxy = load_proxy_blocking(&pool).unwrap_or_default();
        let has_api_key = !proxy.api_key.is_empty();
        // Strip the key before returning.
        let mut safe = proxy;
        safe.api_key = String::new();
        Ok(Json(SettingsResp {
            has_api_key,
            proxy: safe,
            persona,
        }))
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

/// PUT /api/settings — save proxy config and/or persona.
pub async fn put_settings(
    State(state): State<AppState>,
    Json(body): Json<SettingsReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || -> Result<Json<serde_json::Value>, AppError> {
        let conn = pool.get()?;
        if let Some(mut proxy) = body.proxy {
                    // Preserve existing api_key if the caller sent a blank one
                    // (the frontend never sees it so it can't resend it).
                    if proxy.api_key.is_empty() {
                        if let Some(old) = load_proxy_blocking(&pool) {
                            proxy.api_key = old.api_key;
                        }
                    }
                    let json = serde_json::to_string(&proxy)?;
                    conn.execute(
                        "INSERT OR REPLACE INTO settings (key, value) VALUES ('proxy_config', ?1)",
                        [&json],
                    )?;
                }
        if let Some(persona) = &body.persona {
            let json = serde_json::to_string(persona)?;
            conn.execute(
                "INSERT OR REPLACE INTO settings (key, value) VALUES ('persona', ?1)",
                [&json],
            )?;
        }
        Ok(Json(serde_json::json!({"ok": true})))
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

// --- shared helpers (also used by chats.rs) ---

pub(crate) fn load_persona_blocking(
    pool: &r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>,
) -> Result<Persona, AppError> {
    let conn = pool.get()?;
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key='persona'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    Ok(match raw {
        Some(json) => serde_json::from_str(&json).unwrap_or_default(),
        None => Persona::default(),
    })
}

fn load_proxy_blocking(
    pool: &r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>,
) -> Option<ProxyConfig> {
    let conn = pool.get().ok()?;
    let raw: String = conn
        .query_row(
            "SELECT value FROM settings WHERE key='proxy_config'",
            [],
            |row| row.get(0),
        )
        .ok()?;
    serde_json::from_str(&raw).ok()
}

use rusqlite::OptionalExtension;
