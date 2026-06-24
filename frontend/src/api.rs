//! Thin REST client over the backend's `/api/*` endpoints.
//!
//! Every call returns the typed DTO from the `shared` crate (the same types the
//! server serializes), or a human-readable `String` error. The browser never
//! talks to an LLM directly anymore — it just calls our own backend, which
//! proxies to the provider server-side. That kills the CORS problem the old
//! direct-from-browser connector fought, and keeps the API key on the server.

use gloo_net::http::Request;
use serde::de::DeserializeOwned;
use serde::Serialize;

use shared::dto::{
    ChatDetail, ChatListEntry, EditMessageReq, HealthResp, ImportCardReq, NewCharacterReq,
    SelectVariantReq, SendMessageReq, SendMessageResp, SettingsReq, SettingsResp,
    UpdateCharacterReq, UpdateMemoryReq,
};
use shared::types::Character;

/// Pull `{"error": "..."}` out of a failed response body, falling back to the
/// raw text (truncated) so the user always sees *something* actionable.
fn err_msg(status: u16, body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(e) = v.get("error").and_then(|e| e.as_str()) {
            return e.to_string();
        }
    }
    let b = body.trim();
    if b.is_empty() {
        format!("HTTP {status}")
    } else {
        let short: String = b.chars().take(200).collect();
        format!("HTTP {status}: {short}")
    }
}

/// Read a JSON response, mapping non-2xx into a friendly error.
async fn read_json<T: DeserializeOwned>(resp: gloo_net::http::Response) -> Result<T, String> {
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("could not read response: {e}"))?;
    if !(200..300).contains(&status) {
        return Err(err_msg(status, &text));
    }
    serde_json::from_str(&text).map_err(|e| format!("unexpected response: {e}"))
}

async fn get<T: DeserializeOwned>(url: &str) -> Result<T, String> {
    let resp = Request::get(url)
        .send()
        .await
        .map_err(|e| format!("network error: {e}"))?;
    read_json(resp).await
}

async fn send_with_body<B: Serialize, T: DeserializeOwned>(
    method: &str,
    url: &str,
    body: &B,
) -> Result<T, String> {
    let builder = match method {
        "POST" => Request::post(url),
        "PUT" => Request::put(url),
        _ => Request::post(url),
    };
    let resp = builder
        .json(body)
        .map_err(|e| format!("could not encode request: {e}"))?
        .send()
        .await
        .map_err(|e| format!("network error: {e}"))?;
    read_json(resp).await
}

async fn post_empty<T: DeserializeOwned>(url: &str) -> Result<T, String> {
    let resp = Request::post(url)
        .send()
        .await
        .map_err(|e| format!("network error: {e}"))?;
    read_json(resp).await
}

async fn delete(url: &str) -> Result<(), String> {
    let resp = Request::delete(url)
        .send()
        .await
        .map_err(|e| format!("network error: {e}"))?;
    let status = resp.status();
    if !(200..300).contains(&status) {
        let text = resp.text().await.unwrap_or_default();
        return Err(err_msg(status, &text));
    }
    Ok(())
}

// ---- health ----------------------------------------------------------------

pub async fn health() -> Result<HealthResp, String> {
    get("/api/health").await
}

// ---- characters -------------------------------------------------------------

pub async fn list_characters() -> Result<Vec<Character>, String> {
    get("/api/characters").await
}

pub async fn get_character(id: i64) -> Result<Character, String> {
    get(&format!("/api/characters/{id}")).await
}

pub async fn create_character(req: &NewCharacterReq) -> Result<Character, String> {
    send_with_body("POST", "/api/characters", req).await
}

pub async fn update_character(id: i64, req: &UpdateCharacterReq) -> Result<Character, String> {
    send_with_body("PUT", &format!("/api/characters/{id}"), req).await
}

/// Import a Tavern V1/V2/V3 card (raw JSON, optionally with an avatar data-URL).
pub async fn import_character(json: String, avatar: Option<String>) -> Result<Character, String> {
    send_with_body("POST", "/api/characters/import", &ImportCardReq { json, avatar }).await
}

pub async fn delete_character(id: i64) -> Result<(), String> {
    delete(&format!("/api/characters/{id}")).await
}

// ---- chats ------------------------------------------------------------------

pub async fn list_chats_for(character_id: i64) -> Result<Vec<ChatListEntry>, String> {
    get(&format!("/api/characters/{character_id}/chats")).await
}

/// Start a fresh chat with a character (seeds the greeting as message #1).
pub async fn create_chat(character_id: i64) -> Result<ChatDetail, String> {
    post_empty(&format!("/api/characters/{character_id}/chats")).await
}

pub async fn get_chat(id: i64) -> Result<ChatDetail, String> {
    get(&format!("/api/chats/{id}")).await
}

pub async fn delete_chat(id: i64) -> Result<(), String> {
    delete(&format!("/api/chats/{id}")).await
}

pub async fn update_memory(id: i64, memory: String) -> Result<(), String> {
    let _: serde_json::Value =
        send_with_body("PUT", &format!("/api/chats/{id}/memory"), &UpdateMemoryReq { memory })
            .await?;
    Ok(())
}

pub async fn send_message(chat_id: i64, text: String) -> Result<SendMessageResp, String> {
    send_with_body("POST", &format!("/api/chats/{chat_id}/send"), &SendMessageReq { text }).await
}

pub async fn regenerate(chat_id: i64) -> Result<SendMessageResp, String> {
    post_empty(&format!("/api/chats/{chat_id}/regenerate")).await
}

// ---- messages ---------------------------------------------------------------

pub async fn edit_message(id: i64, text: String) -> Result<(), String> {
    let _: serde_json::Value =
        send_with_body("PUT", &format!("/api/messages/{id}"), &EditMessageReq { text }).await?;
    Ok(())
}

pub async fn delete_message(id: i64) -> Result<(), String> {
    delete(&format!("/api/messages/{id}")).await
}

/// Switch which stored variant (swipe) of a bot message is shown.
pub async fn select_variant(id: i64, variant: i64) -> Result<(), String> {
    let _: serde_json::Value = send_with_body(
        "PUT",
        &format!("/api/messages/{id}/variant"),
        &SelectVariantReq { variant },
    )
    .await?;
    Ok(())
}

// ---- settings ---------------------------------------------------------------

pub async fn get_settings() -> Result<SettingsResp, String> {
    get("/api/settings").await
}

pub async fn put_settings(req: &SettingsReq) -> Result<(), String> {
    let _: serde_json::Value = send_with_body("PUT", "/api/settings", req).await?;
    Ok(())
}
