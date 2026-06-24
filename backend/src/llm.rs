//! Server-side LLM proxy. Loads the saved proxy config from the database,
//! calls `shared::build_request` to render the request, fires it via `reqwest`,
//! and extracts the reply text via `shared::extract`.
//!
//! Supports multi-key rotation (comma-separated keys) and context-window
//! truncation (drops oldest messages until the estimated token count fits).

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use shared::template::{self, ChatMessage, ProxyConfig, RenderedRequest};
use shared::types::{Character, Persona};

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
    if !cfg.api_key.contains(',') {
        return; // plain single key — leave as-is
    }
    let keys: Vec<&str> = cfg
        .api_key
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if keys.is_empty() {
        return; // nothing usable — leave as-is
    }
    // `multi_key` rotates across the list. With multi_key off the key may still
    // be a comma list (a preserved list whose owner toggled rotation off) — pick
    // the first so a comma-joined string never hits the wire as a single key.
    let idx = if cfg.multi_key && keys.len() > 1 {
        // Use sub-second nanos as a cheap random index (no rand dep needed).
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as usize % keys.len())
            .unwrap_or(0)
    } else {
        0
    };
    cfg.api_key = keys[idx].to_string();
}


/// Append a labelled, non-empty section to the system prompt.
fn section(buf: &mut String, label: &str, body: &str) {
    let body = body.trim();
    if body.is_empty() {
        return;
    }
    buf.push_str("\n\n");
    buf.push_str(label);
    buf.push('\n');
    buf.push_str(body);
}

/// Select lorebook entries relevant to the recent conversation. Keyless entries
/// are always included ("constant"); keyed entries fire when any key appears in
/// the recent text (case-insensitive). Disabled entries are skipped.
fn active_lore(character: &Character, history: &[ChatMessage]) -> Vec<String> {
    if character.lorebook.is_empty() {
        return Vec::new();
    }
    // Scan a window of the most recent turns for key matches.
    let recent: String = history
        .iter()
        .rev()
        .take(8)
        .map(|m| m.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    character
        .lorebook
        .iter()
        .filter(|e| e.enabled && !e.content.trim().is_empty())
        .filter(|e| {
            e.keys.is_empty()
                || e.keys
                    .iter()
                    .any(|k| !k.trim().is_empty() && recent.contains(&k.to_lowercase()))
        })
        .map(|e| e.content.trim().to_string())
        .collect()
}

/// Build the full system prompt from the character card, the active persona,
/// the chat memory, and any triggered lorebook entries. This is what makes
/// characters actually *work* — the whole card (personality, scenario,
/// description, example dialogue) is sent, not just the name. The proxy-level
/// custom prompt is prepended later, inside [`complete`].
pub fn build_system(
    character: &Character,
    persona: &Persona,
    memory: &str,
    history: &[ChatMessage],
) -> String {
    let name = &character.name;
    let mut s = String::new();

    // Character-level system prompt overrides the generic framing when present.
    if !character.system_prompt.trim().is_empty() {
        s.push_str(character.system_prompt.trim());
    } else {
        s.push_str(&format!(
            "You are {name}, a fictional roleplay character. Stay fully in character \
             as {name}: write vivid, immersive, in-character replies that move the \
             scene forward, and never break character or mention being an AI."
        ));
    }

    if !character.tagline.trim().is_empty() {
        section(&mut s, &format!("# {name} — summary"), &character.tagline);
    }
    section(&mut s, &format!("# {name}'s personality"), &character.personality);
    section(&mut s, &format!("# About {name}"), &character.description);
    section(&mut s, "# Scenario", &character.scenario);
    section(&mut s, "# Example dialogue", &character.mes_example);

    // Persona — who the user is.
    if !persona.name.trim().is_empty() || !persona.description.trim().is_empty() {
        let who = if persona.name.trim().is_empty() {
            "the user".to_string()
        } else {
            persona.name.clone()
        };
        let mut body = format!("The user is roleplaying as {who}.");
        if !persona.description.trim().is_empty() {
            body.push(' ');
            body.push_str(persona.description.trim());
        }
        section(&mut s, "# Your scene partner", &body);
    }

    // Triggered lorebook / world-info.
    let lore = active_lore(character, history);
    if !lore.is_empty() {
        section(&mut s, "# World info", &lore.join("\n\n"));
    }

    // Chat memory — user-curated facts.
    section(&mut s, "# Important context to remember", memory);

    // NOTE: post_history_instructions (jailbreak / UJB) are deliberately NOT
    // appended here — they are passed separately to `complete` so the templating
    // layer can place them *after* the chat history (strongest recency).
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
    post_history: &str,
) -> Result<String, String> {
    let mut cfg = crate::routes::settings::load_active_proxy(pool);
    if cfg.url.trim().is_empty() {
        return Err("No API endpoint configured. Open Settings to point me at your proxy/API.".into());
    }
    // Prepend the proxy-level custom prompt (JAI "Custom Prompt") if set.
    let system: String = if cfg.system_prompt.trim().is_empty() {
        system.to_string()
    } else {
        format!("{}\n\n{}", cfg.system_prompt.trim(), system)
    };
    // Multi-key: pick one key from the comma-separated list.
    resolve_key(&mut cfg);
    // Context window: truncate history if a limit is set, reserving room for
    // the system prompt (character + persona + memory) which is always sent.
    // Clamp the reservation to the window so an oversize system prompt can't
    // drive the history budget arbitrarily negative.
    let history = if cfg.context_tokens > 0 {
        let sys_tokens = est_tokens(&system).min(cfg.context_tokens);
        let hist_limit = cfg.context_tokens - sys_tokens;
        truncate_history(history, hist_limit)
    } else {
        history.to_vec()
    };
    let req = template::build_request(&cfg, &history, &system, post_history)?;
    send_request(client, req, &cfg.response_path).await
}

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
    fn resolve_key_single_mode_takes_first() {
        let mut cfg = ProxyConfig::openai();
        cfg.multi_key = false;
        cfg.api_key = "k1,k2".into();
        resolve_key(&mut cfg);
        // With multi_key off, a preserved comma list collapses to the first key
        // for the request so a comma-joined string never hits the wire as one key.
        assert_eq!(cfg.api_key, "k1");
    }

    #[test]
    fn resolve_key_plain_key_untouched() {
        let mut cfg = ProxyConfig::openai();
        cfg.multi_key = false;
        cfg.api_key = "sk-single".into();
        resolve_key(&mut cfg);
        assert_eq!(cfg.api_key, "sk-single");
    }
}
