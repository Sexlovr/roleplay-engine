//! Provider-agnostic LLM request/response templating engine.
//!
//! Pure logic with no I/O — builds for both native (backend) and wasm32 (frontend).
//!
//! A [`ProxyConfig`] describes *any* HTTP+JSON endpoint via:
//!   - a URL, headers, model, and credentials,
//!   - a request **body template** with `{{placeholders}}`, and
//!   - a **response path** (dot/index path) to pull the reply text out of the
//!     returned JSON.
//!
//! Presets fill these in for OpenAI-compatible, Anthropic, and Gemini; a blank
//! "Custom" preset lets the user wire up anything.
//!
//! ## Placeholders (substituted in url, header values, and body)
//!   `{{api_key}}` `{{model}}` `{{temperature}}` `{{max_tokens}}`
//!   `{{prompt}}`  — latest user message, JSON-escaped (no surrounding quotes)
//!   `{{system}}`  — system prompt, JSON-escaped (no surrounding quotes)
//!   `{{messages}}` — OpenAI-shaped array `[{"role","content"}...]` (no system)
//!   `{{messages_system}}` — same, with the system message prepended

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// A chat message as it flows through the templating engine and API layer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub from_user: bool,
    pub text: String,
}

/// Describes an LLM endpoint: URL, credentials, request body shape, and where
/// the reply text lives in the response JSON.
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
        ProxyConfig {
            name: "My Proxy".into(),
            url: String::new(),
            api_key: String::new(),
            model: String::new(),
            headers: vec![(
                "Authorization".into(),
                "Bearer {{api_key}}".into(),
            )],
            body_template: "{\n  \"model\": \"{{model}}\",\n  \"messages\": {{messages_system}},\n  \"temperature\": {{temperature}},\n  \"max_tokens\": {{max_tokens}}\n}".into(),
            response_path: "choices.0.message.content".into(),
            temperature: 0.8,
            max_tokens: 600,
        }
    }
}

impl ProxyConfig {
    pub fn openai() -> Self {
        ProxyConfig {
            name: "OpenAI-compatible".into(),
            url: "https://api.openai.com/v1/chat/completions".into(),
            headers: vec![(
                "Authorization".into(),
                "Bearer {{api_key}}".into(),
            )],
            body_template: "{\n  \"model\": \"{{model}}\",\n  \"messages\": {{messages_system}},\n  \"temperature\": {{temperature}},\n  \"max_tokens\": {{max_tokens}}\n}".into(),
            response_path: "choices.0.message.content".into(),
            ..Default::default()
        }
    }
    pub fn anthropic() -> Self {
        ProxyConfig {
            name: "Anthropic".into(),
            url: "https://api.anthropic.com/v1/messages".into(),
            headers: vec![
                ("x-api-key".into(), "{{api_key}}".into()),
                ("anthropic-version".into(), "2023-06-01".into()),
            ],
            body_template: "{\n  \"model\": \"{{model}}\",\n  \"max_tokens\": {{max_tokens}},\n  \"temperature\": {{temperature}},\n  \"system\": \"{{system}}\",\n  \"messages\": {{messages}}\n}".into(),
            response_path: "content.0.text".into(),
            ..Default::default()
        }
    }
    pub fn gemini() -> Self {
        ProxyConfig {
            name: "Gemini".into(),
            url: "https://generativelanguage.googleapis.com/v1beta/models/{{model}}:generateContent?key={{api_key}}".into(),
            headers: vec![],
            body_template: "{\n  \"systemInstruction\": { \"parts\": [{ \"text\": \"{{system}}\" }] },\n  \"contents\": [{ \"role\": \"user\", \"parts\": [{ \"text\": \"{{prompt}}\" }] }]\n}".into(),
            response_path: "candidates.0.content.parts.0.text".into(),
            ..Default::default()
        }
    }
    pub fn blank() -> Self {
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

// ---- templating ------------------------------------------------------------

/// JSON-escape `s` and strip the surrounding quotes, so it can be dropped
/// inside a `"..."` in a template.
pub fn esc(s: &str) -> String {
    let q = serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into());
    q[1..q.len() - 1].to_string()
}

/// Build an OpenAI-shaped `[{"role","content"}...]` array (no system message).
pub fn messages_array(history: &[ChatMessage]) -> Value {
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

/// Substitute all placeholders in `s`.
pub fn fill(
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
        // Try an object-key lookup first, then fall back to a numeric array
        // index. (Key-first lets objects with numeric-string keys resolve,
        // while arrays still work via the usize fallback.)
        cur = match cur.get(seg) {
            Some(child) => child,
            None => cur.get(seg.parse::<usize>().ok()?)?,
        };
    }
    match cur {
        Value::String(s) => Some(s.clone()),
        Value::Number(_) | Value::Bool(_) => Some(cur.to_string()),
        _ => None,
    }
}

/// Truncate `s` to at most `n` bytes on a UTF-8 char boundary.
pub fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        return s.to_string();
    }
    let mut end = n;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

/// The result of rendering a template: a fully-resolved HTTP request.
#[derive(Clone, Debug)]
pub struct RenderedRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// Render a complete LLM request from config + history + system prompt.
/// Mirrors the old `send_chat`'s pure prep without performing I/O.
///
/// Returns an error if the rendered body is not valid JSON (so callers detect
/// malformed templates before sending HTTP).
pub fn build_request(
    cfg: &ProxyConfig,
    history: &[ChatMessage],
    system: &str,
) -> Result<RenderedRequest, String> {
    // Drop leading non-user turns (e.g. greeting) so the API request starts
    // with a real user message — required by Anthropic Messages and avoids
    // replaying a UI-only greeting as if the model produced it.
    let api_history: Vec<ChatMessage> = history
        .iter()
        .skip_while(|m| !m.from_user)
        .cloned()
        .collect();

    let prompt = api_history
        .iter()
        .rev()
        .find(|m| m.from_user)
        .map(|m| m.text.clone())
        .unwrap_or_default();

    let arr = messages_array(&api_history);
    let msgs = serde_json::to_string(&arr).unwrap();
    let mut with_sys = vec![json!({"role": "system", "content": system})];
    if let Value::Array(a) = arr {
        with_sys.extend(a);
    }
    let msgs_sys = serde_json::to_string(&Value::Array(with_sys)).unwrap();

    let url = fill(&cfg.url, cfg, &msgs, &msgs_sys, system, &prompt);
    let body = fill(&cfg.body_template, cfg, &msgs, &msgs_sys, system, &prompt);

    // Validate the rendered body is legal JSON.
    if serde_json::from_str::<Value>(&body).is_err() {
        return Err("Body template did not render to valid JSON — check your placeholders/quotes.".into());
    }

    let headers: Vec<(String, String)> = cfg
        .headers
        .iter()
        .filter(|(k, _)| !k.trim().is_empty())
        .map(|(k, v)| {
            let val = fill(v, cfg, &msgs, &msgs_sys, system, &prompt);
            (k.clone(), val)
        })
        .collect();

    Ok(RenderedRequest { url, headers, body })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_nested_and_indexed() {
        let v: Value =
            serde_json::from_str(r#"{"choices":[{"message":{"content":"hi there"}}]}"#).unwrap();
        assert_eq!(
            extract(&v, "choices.0.message.content").as_deref(),
            Some("hi there")
        );
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

    #[test]
    fn build_request_starts_with_user_message() {
        let cfg = ProxyConfig::openai();
        let history = vec![
            ChatMessage { from_user: false, text: "Hello! Welcome.".into() },
            ChatMessage { from_user: true, text: "Hi there".into() },
            ChatMessage { from_user: false, text: "How can I help?".into() },
        ];
        let req = build_request(&cfg, &history, "You are a bot.").unwrap();
        let body: Value = serde_json::from_str(&req.body).unwrap();
        let msgs = body["messages"].as_array().unwrap();
        // First message in the API array should be the system message, second
        // should be the first user message (the greeting was dropped).
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[1]["role"], "user");
        assert_eq!(msgs[1]["content"], "Hi there");
    }

    #[test]
    fn truncate_ascii() {
        assert_eq!(truncate("hello", 3), "hel…");
        assert_eq!(truncate("hi", 10), "hi");
    }

    #[test]
    fn truncate_multibyte() {
        let s = "a".repeat(5) + &"日".repeat(5);
        // s = "aaaaa" + 5×"日" (each 日 = 3 UTF-8 bytes) = 20 bytes.
        // Truncate at byte 8 lands on the second "日", which is a char boundary
        // — but if it didn't, the walk-back would find one.
        let t = truncate(&s, 8);
        assert!(t.ends_with('…'), "expected ellipsis, got: {t}");
        // Result: "aaaaa日…" = 5 ASCII + 1 CJK (3 bytes) + ellipsis (3 bytes) = 11.
        assert!(t.len() <= 12, "unexpected len: {}", t.len());
        // Also test truncation right in the middle of a CJK char.
        let t2 = truncate(&s, 6); // byte 6 = middle of the first 日
        assert!(t2.ends_with('…'));
        assert_eq!(t2, "aaaaa…"); // walked back to byte 5
    }
}
