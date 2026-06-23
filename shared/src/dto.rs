//! API request/response DTOs shared between the frontend and backend.
//!
//! The frontend imports these to type its `gloo-net` fetch calls; the backend
//! uses them for JSON serialization/deserialization in axum handlers.

use serde::{Deserialize, Serialize};

use crate::template::ProxyConfig;
use crate::types::{Character, Chat, Persona};

// ---- health ----------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HealthResp {
    pub data_dir: String,
    pub persistent: bool, // best-effort guess
    pub db_exists: bool,
}

// ---- characters ------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewCharacterReq {
    pub name: String,
    pub tagline: Option<String>,
    pub description: Option<String>,
    pub personality: Option<String>,
    pub scenario: Option<String>,
    pub first_message: Option<String>,
    pub avatar: Option<String>,
    pub tags: Option<Vec<String>>,
    pub creator: Option<String>,
    pub nsfw: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateCharacterReq {
    pub name: Option<String>,
    pub tagline: Option<String>,
    pub description: Option<String>,
    pub personality: Option<String>,
    pub scenario: Option<String>,
    pub first_message: Option<String>,
    pub avatar: Option<String>,
    pub tags: Option<Vec<String>>,
    pub creator: Option<String>,
    pub nsfw: Option<bool>,
}

// ---- chats -----------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatListEntry {
    pub id: i64,
    pub character_id: i64,
    pub character_name: String,
    pub title: String,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatDetail {
    pub chat: Chat,
    pub character: Character,
    pub messages: Vec<MessageView>,
}

/// A persisted message as shown in the chat log. Unlike the bare
/// [`ChatMessage`] used for LLM templating, this carries the row `id` so the
/// frontend can edit/delete a specific message via `/api/messages/{id}`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageView {
    pub id: i64,
    pub from_user: bool,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateMemoryReq {
    pub memory: String,
}

// ---- messages --------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendMessageReq {
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendMessageResp {
    pub user: MessageView,
    pub reply: MessageView,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditMessageReq {
    pub text: String,
}

// ---- settings --------------------------------------------------------------

/// Settings returned to the frontend — the API key is NEVER included.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettingsResp {
    pub has_api_key: bool,
    pub proxy: ProxyConfig, // api_key field is empty in the response
    pub persona: Persona,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettingsReq {
    pub proxy: Option<ProxyConfig>, // None = don't change
    pub persona: Option<Persona>,
}
