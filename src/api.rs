//! Provider-agnostic chat connector.
//!
//! Instead of hard-coding one provider's request/response shape (the way JAI
//! only speaks OpenAI `/v1/chat/completions`), a [`ProxyConfig`] describes *any*
//! HTTP+JSON endpoint via:
//!   - a URL, headers, model, and credentials,
//!   - a request **body template** with `{{placeholders}}`, and
//!   - a **response path** (dot/index path) to pull the reply text out of the
//!     returned JSON.
//!
//! Presets fill these in for OpenAI-compatible, Anthropic, and Gemini; a blank
//! "Custom" preset lets the user wire up anything — including formats that
//! don't exist yet. Nothing is restricted to one schema.
//!
//! ## Placeholders (substituted in url, header values, and body)
//!   `{{api_key}}` `{{model}}` `{{temperature}}` `{{max_tokens}}`
//!   `{{prompt}}`  — latest user message, JSON-escaped (no surrounding quotes)
//!   `{{system}}`  — system prompt, JSON-escaped (no surrounding quotes)
//!   `{{messages}}` — OpenAI-shaped array `[{"role","content"}...]` (no system)
//!   `{{messages_system}}` — same, with the system message prepended

use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::types::ChatMessage;

pub const STORAGE_KEY: &str = "rp_proxy_config";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub name: String,
    pub url: String,
    pub api_key: String,
    pub model: String,
    /// Extra request headers (values may contain placeholders).
    pub headers: Vec<(String, String)>,
    pub body_template: String,
    pub response_path: String,
    pub temperature: f32,
    pub max_tokens: u32,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        // An empty config — the user must point it at their own endpoint.
        ProxyConfig {
            name: "My Proxy".into(),
            url: String::new(),
            api_key: String::new(),
            model: String::new(),
            headers: vec![("Authorization".into(), "Bearer {{api_key}}".into())],
            body_template:
                "{\n  \"model\": \"{{model}}\",\n  \"messages\": {{messages_system}},\n  \"temperature\": {{temperature}},\n  \"max_tokens\": {{max_tokens}}\n}"
                    .into(),
            response_path: "choices.0.message.content".into(),
            temperature: 0.8,
            max_tokens: 600,
        }
    }
}

impl ProxyConfig {
    fn openai() -> Self {
        ProxyConfig {
            name: "OpenAI-compatible".into(),
            url: "https://api.openai.com/v1/chat/completions".into(),
            headers: vec![("Authorization".into(), "Bearer {{api_key}}".into())],
            body_template:
                "{\n  \"model\": \"{{model}}\",\n  \"messages\": {{messages_system}},\n  \"temperature\": {{temperature}},\n  \"max_tokens\": {{max_tokens}}\n}"
                    .into(),
            response_path: "choices.0.message.content".into(),
            ..Default::default()
        }
    }
    fn anthropic() -> Self {
        ProxyConfig {
            name: "Anthropic".into(),
            url: "https://api.anthropic.com/v1/messages".into(),
            headers: vec![
                ("x-api-key".into(), "{{api_key}}".into()),
                ("anthropic-version".into(), "2023-06-01".into()),
            ],
            body_template:
                "{\n  \"model\": \"{{model}}\",\n  \"max_tokens\": {{max_tokens}},\n  \"temperature\": {{temperature}},\n  \"system\": \"{{system}}\",\n  \"messages\": {{messages}}\n}"
                    .into(),
            response_path: "content.0.text".into(),
            ..Default::default()
        }
    }
    fn gemini() -> Self {
        ProxyConfig {
            name: "Gemini".into(),
            url: "https://generativelanguage.googleapis.com/v1beta/models/{{model}}:generateContent?key={{api_key}}".into(),
            headers: vec![],
            body_template:
                "{\n  \"systemInstruction\": { \"parts\": [{ \"text\": \"{{system}}\" }] },\n  \"contents\": [{ \"role\": \"user\", \"parts\": [{ \"text\": \"{{prompt}}\" }] }]\n}"
                    .into(),
            response_path: "candidates.0.content.parts.0.text".into(),
            ..Default::default()
        }
    }
    fn blank() -> Self {
        ProxyConfig {
            name: "Custom".into(),
            url: String::new(),
            headers: vec![],
            body_template: "{\n  \"input\": \"{{prompt}}\"\n}".into(),
            response_path: "output".into(),
            ..Default::default()
        }
    }
}

/// Named presets shown in the settings dropdown.
pub fn presets() -> Vec<ProxyConfig> {
    vec![
        ProxyConfig::openai(),
        ProxyConfig::anthropic(),
        ProxyConfig::gemini(),
        ProxyConfig::blank(),
    ]
}

// ---- persistence ----------------------------------------------------------
pub fn load() -> Option<ProxyConfig> {
    LocalStorage::get(STORAGE_KEY).ok()
}
pub fn save(cfg: &ProxyConfig) {
    let _ = LocalStorage::set(STORAGE_KEY, cfg);
}

// ---- templating ------------------------------------------------------------
/// JSON-escape `s` and strip the surrounding quotes, so it can be dropped
/// inside a `"..."` in a template.
fn esc(s: &str) -> String {
    let q = serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into());
    q[1..q.len() - 1].to_string()
}

fn messages_array(history: &[ChatMessage]) -> Value {
    Value::Array(
        history
            .iter()
            .map(|m| {
                json!({
                    "role": if m.from_user { "user" } else { "assistant" },
                    "content": m.text,
                })
            })
            .collect(),
    )
}

fn fill(
    s: &str,
    cfg: &ProxyConfig,
    messages: &str,
    messages_system: &str,
    system: &str,
    prompt: &str,
) -> String {
    s.replace("{{api_key}}", &cfg.api_key)
        .replace("{{model}}", &cfg.model)
        .replace("{{temperature}}", &cfg.temperature.to_string())
        .replace("{{max_tokens}}", &cfg.max_tokens.to_string())
        .replace("{{messages_system}}", messages_system)
        .replace("{{messages}}", messages)
        .replace("{{system}}", &esc(system))
        .replace("{{prompt}}", &esc(prompt))
}

/// Walk a dot/index path (e.g. `choices.0.message.content`) into a JSON value.
pub fn extract(v: &Value, path: &str) -> Option<String> {
    let mut cur = v;
    for seg in path.split('.').filter(|s| !s.is_empty()) {
        cur = match seg.parse::<usize>() {
            Ok(i) => cur.get(i)?,
            Err(_) => cur.get(seg)?,
        };
    }
    match cur {
        Value::String(s) => Some(s.clone()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n])
    }
}

/// Send the conversation to the configured endpoint and return the reply text.
/// `history` is the full visible log (greeting + turns); `system` is the
/// character's system prompt.
pub async fn send_chat(
    cfg: ProxyConfig,
    history: Vec<ChatMessage>,
    system: String,
) -> Result<String, String> {
    let prompt = history
        .iter()
        .rev()
        .find(|m| m.from_user)
        .map(|m| m.text.clone())
        .unwrap_or_default();

    let msgs = serde_json::to_string(&messages_array(&history)).unwrap();
    let mut with_sys = vec![json!({"role": "system", "content": system})];
    if let Value::Array(a) = messages_array(&history) {
        with_sys.extend(a);
    }
    let msgs_sys = serde_json::to_string(&Value::Array(with_sys)).unwrap();

    let url = fill(&cfg.url, &cfg, &msgs, &msgs_sys, &system, &prompt);
    let body = fill(&cfg.body_template, &cfg, &msgs, &msgs_sys, &system, &prompt);

    // Validate the template produced legal JSON before sending (nicer errors).
    if serde_json::from_str::<Value>(&body).is_err() {
        return Err("Body template did not render to valid JSON — check your placeholders/quotes.".into());
    }

    let mut req = gloo_net::http::Request::post(&url).header("Content-Type", "application/json");
    for (k, v) in &cfg.headers {
        if k.trim().is_empty() {
            continue;
        }
        let val = fill(v, &cfg, &msgs, &msgs_sys, &system, &prompt);
        req = req.header(k, &val);
    }

    let resp = req
        .body(body)
        .map_err(|e| format!("request build failed: {e}"))?
        .send()
        .await
        .map_err(|e| format!("network error (CORS or unreachable?): {e}"))?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("could not read response: {e}"))?;

    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}: {}", truncate(&text, 300)));
    }
    let val: Value = serde_json::from_str(&text)
        .map_err(|e| format!("response was not JSON: {e} — {}", truncate(&text, 200)))?;
    extract(&val, &cfg.response_path).ok_or_else(|| {
        format!(
            "response_path '{}' not found in: {}",
            cfg.response_path,
            truncate(&text, 300)
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extracts_nested_and_indexed() {
        let v: Value = serde_json::from_str(
            r#"{"choices":[{"message":{"content":"hi there"}}]}"#,
        )
        .unwrap();
        assert_eq!(extract(&v, "choices.0.message.content").as_deref(), Some("hi there"));
        assert_eq!(extract(&v, "choices.5.message.content"), None);
    }
    #[test]
    fn esc_escapes_quotes_and_newlines() {
        assert_eq!(esc("a\"b\nc"), "a\\\"b\\nc");
    }
    #[test]
    fn fill_injects_message_array_as_raw_json() {
        let cfg = ProxyConfig::openai();
        let out = fill(r#"{"m":{{messages}}}"#, &cfg, "[1,2]", "[3]", "sys", "hi");
        assert_eq!(out, r#"{"m":[1,2]}"#);
    }
}
