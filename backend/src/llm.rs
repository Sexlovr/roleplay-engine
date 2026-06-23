//! Server-side LLM proxy. Loads the saved proxy config from the database,
//! calls `shared::build_request` to render the request, fires it via `reqwest`,
//! and extracts the reply text via `shared::extract`.
//!
//! Supports multi-key rotation (comma-separated keys) and context-window
//! truncation (drops oldest messages until the estimated token count fits).

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use shared::template::{self, ChatMessage, ProxyConfig, RenderedRequest};

use crate::error::AppError;

/// Rough token estimate per word (used when truncating history to fit the
/// context window). ~1.3 tokens per whitespace-delimited word covers most
/// English text with a safety margin.
const TOKENS_PER_WORD: f64 = 1.3;

fn est_tokens(text: &str) -> i64 {
    (text.split_whitespace().count() as f64 * TOKENS_PER_WORD).ceil() as i64
}

/// Truncate history so the total estimated token count is ≤ `limit`.
/// Always keeps at least the last user→assistant pair. This is only called in
/// truncation mode (a positive context window was configured), so a non-positive
/// `limit` — e.g. the system prompt already ate the whole budget — means "keep
/// the bare minimum" rather than "unlimited".
fn truncate_history(history: &[ChatMessage], limit: i64) -> Vec<ChatMessage> {
    if history.is_empty() {
        return Vec::new();
    }
    // Budget exhausted by the system prompt: keep only the most recent turn(s).
    if limit <= 0 {
        return if history.len() >= 2 {
            history[history.len() - 2..].to_vec()
        } else {
            history.to_vec()
        };
    }
    let total: i64 = history.iter().map(|m| est_tokens(&m.text)).sum();
    if total <= limit {
        return history.to_vec();
    }
    // Walk back from the end, keeping the most recent messages.
    let mut kept: Vec<ChatMessage> = Vec::new();
    let mut tokens: i64 = 0;
    for m in history.iter().rev() {
        let t = est_tokens(&m.text);
        if tokens + t > limit && !kept.is_empty() {
            break;
        }
        tokens += t;
        kept.push(m.clone());
    }
    kept.reverse();
    // Ensure we always have at least the last 2 messages (user + reply).
    if kept.len() < 2 && history.len() >= 2 {
        history[history.len() - 2..].to_vec()
    } else {
        kept
    }
}

/// Pick a key from the config. If multi_key is enabled, `api_key` is treated
/// as comma-separated keys and one is selected (cheap time-based rotation).
/// Always trims the selected key so stray whitespace/commas never hit the wire.
fn resolve_key(cfg: &mut ProxyConfig) {
    if cfg.multi_key && cfg.api_key.contains(',') {
        let keys: Vec<&str> = cfg
            .api_key
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        if keys.is_empty() {
            return; // nothing usable — leave as-is
        }
        let idx = if keys.len() > 1 {
            // Use sub-second nanos as a cheap random index (no rand dep needed).
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos() as usize % keys.len())
                .unwrap_or(0)
        } else {
            0
        };
        // Always reassign so a single surviving key (e.g. "sk-abc,") is trimmed.
        cfg.api_key = keys[idx].to_string();
    }
}

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

/// Run completion for a chat: load config, pick key, truncate to context
/// window, build request, send, return reply text. `history` is the full
/// message log (including greeting); `system` is built by `build_system`.
/// Leading non-user turns are trimmed by `build_request` so the API request
/// starts with a user message.
pub async fn complete(
    pool: &Pool<SqliteConnectionManager>,
    client: &reqwest::Client,
    history: &[ChatMessage],
    system: &str,
) -> Result<String, String> {
    let mut cfg = load_config(pool).map_err(|e| format!("load config: {e}"))?;
    if cfg.url.trim().is_empty() {
        return Err("No API endpoint configured. Open Settings to point me at your proxy/API.".into());
    }
    // Multi-key: pick one key from the comma-separated list.
    resolve_key(&mut cfg);
    // Context window: truncate history if a limit is set, reserving room for
    // the system prompt (character + persona + memory) which is always sent.
    let history = if cfg.context_tokens > 0 {
        let hist_limit = cfg.context_tokens - est_tokens(system);
        truncate_history(history, hist_limit)
    } else {
        history.to_vec()
    };
    let req = template::build_request(&cfg, &history, system)?;
    send_request(client, req, &cfg.response_path).await
}

// Pull in rusqlite's .optional() extension.
use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(user: bool, text: &str) -> ChatMessage {
        ChatMessage { from_user: user, text: text.into() }
    }

    #[test]
    fn truncate_keeps_recent_within_limit() {
        let history = vec![
            msg(true, "one two three four five"),  // ~7 tok
            msg(false, "six seven eight nine ten"), // ~7 tok
            msg(true, "eleven twelve"),             // ~3 tok
        ];
        // Big limit keeps everything.
        assert_eq!(truncate_history(&history, 1000).len(), 3);
        // Tiny positive limit keeps at least the last pair.
        let kept = truncate_history(&history, 1);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept.last().unwrap().text, "eleven twelve");
    }

    #[test]
    fn truncate_nonpositive_keeps_last_pair_not_all() {
        let history = vec![
            msg(true, "a b c"),
            msg(false, "d e f"),
            msg(true, "g h i"),
        ];
        // limit <= 0 means the system prompt ate the budget → keep bare minimum.
        let kept = truncate_history(&history, 0);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].text, "d e f");
        assert_eq!(kept[1].text, "g h i");
    }

    #[test]
    fn truncate_empty_is_empty() {
        assert!(truncate_history(&[], 100).is_empty());
        assert!(truncate_history(&[], 0).is_empty());
    }

    #[test]
    fn resolve_key_single_trims_trailing_comma() {
        let mut cfg = ProxyConfig::openai();
        cfg.multi_key = true;
        cfg.api_key = "sk-abc,".into();
        resolve_key(&mut cfg);
        assert_eq!(cfg.api_key, "sk-abc");
    }

    #[test]
    fn resolve_key_picks_one_of_many() {
        let mut cfg = ProxyConfig::openai();
        cfg.multi_key = true;
        cfg.api_key = "k1, k2 , k3".into();
        resolve_key(&mut cfg);
        assert!(["k1", "k2", "k3"].contains(&cfg.api_key.as_str()));
    }

    #[test]
    fn resolve_key_noop_when_disabled() {
        let mut cfg = ProxyConfig::openai();
        cfg.multi_key = false;
        cfg.api_key = "k1,k2".into();
        resolve_key(&mut cfg);
        // Left untouched when multi_key is off (backend route collapses it on save).
        assert_eq!(cfg.api_key, "k1,k2");
    }
}
