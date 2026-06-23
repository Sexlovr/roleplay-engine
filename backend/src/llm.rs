//! Server-side LLM proxy. Loads the saved proxy config from the database,
//! calls `shared::build_request` to render the request, fires it via `reqwest`,
//! and extracts the reply text via `shared::extract`.

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use shared::template::{self, ChatMessage, ProxyConfig, RenderedRequest};

use crate::error::AppError;

/// Load the full proxy config from the settings table. Returns a default if
/// none has been saved yet (empty url — callers should check).
fn load_config(pool: &Pool<SqliteConnectionManager>) -> Result<ProxyConfig, AppError> {
    let conn = pool.get()?;
    let raw: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key='proxy_config'", [], |row| {
            row.get(0)
        })
        .optional()?;
    match raw {
        Some(json) => Ok(serde_json::from_str(&json)?),
        None => Ok(ProxyConfig::default()),
    }
}

/// Build the system prompt for a chat (character + persona + memory).
pub fn build_system(
    char_name: &str,
    char_tagline: &str,
    persona_name: &str,
    persona_desc: &str,
    memory: &str,
) -> String {
    let mut s = format!(
        "You are {char_name}, a roleplay character. {char_tagline}\n\nStay fully in \
         character as {char_name}: write vivid, immersive, in-character replies \
         and never mention being an AI."
    );
    if !persona_name.trim().is_empty() || !persona_desc.trim().is_empty() {
        let who = if persona_name.trim().is_empty() {
            "the user".to_string()
        } else {
            persona_name.to_string()
        };
        s.push_str(&format!("\n\nThe user is roleplaying as {who}."));
        if !persona_desc.trim().is_empty() {
            s.push_str(&format!(" {}", persona_desc));
        }
    }
    if !memory.trim().is_empty() {
        s.push_str(&format!("\n\nImportant context to remember:\n{memory}"));
    }
    s
}

/// Send a rendered request and return the extracted response text.
async fn send_request(
    client: &reqwest::Client,
    req: RenderedRequest,
    response_path: &str,
) -> Result<String, String> {
    let mut builder = client.post(&req.url).header("Content-Type", "application/json");
    for (k, v) in &req.headers {
        builder = builder.header(k, v);
    }
    let resp = builder
        .body(req.body)
        .send()
        .await
        .map_err(|e| format!("network error: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("read error: {e}"))?;
    if !status.is_success() {
        return Err(format!(
            "HTTP {}: {}",
            status,
            template::truncate(&text, 300)
        ));
    }
    let val: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("response was not JSON: {e} — {}", template::truncate(&text, 200)))?;
    template::extract(&val, response_path).ok_or_else(|| {
        format!(
            "response_path '{}' not found in: {}",
            response_path,
            template::truncate(&text, 300)
        )
    })
}

/// Run completion for a chat: load config, build request, send, return reply text.
/// `history` is the full message log (including greeting); `system` is built by
/// `build_system`. Trims leading non-user turns so the API request starts with a
/// user message.
pub async fn complete(
    pool: &Pool<SqliteConnectionManager>,
    client: &reqwest::Client,
    history: &[ChatMessage],
    system: &str,
) -> Result<String, String> {
    let cfg = load_config(pool).map_err(|e| format!("load config: {e}"))?;
    if cfg.url.trim().is_empty() {
        return Err("No API endpoint configured. Open Settings to point me at your proxy/API.".into());
    }
    let req = template::build_request(&cfg, history, system)?;
    send_request(client, req, &cfg.response_path).await
}

// Pull in rusqlite's .optional() extension.
use rusqlite::OptionalExtension;
