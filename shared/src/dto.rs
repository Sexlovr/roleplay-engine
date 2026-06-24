//! API request/response DTOs shared between the frontend and backend.
//!
//! The frontend imports these to type its `gloo-net` fetch calls; the backend
//! uses them for JSON serialization/deserialization in axum handlers.

use serde::{Deserialize, Serialize};

use crate::template::ProxyStore;
use crate::types::{Character, Chat, LoreEntry, PersonaStore};

// ---- health ----------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HealthResp {
    pub data_dir: String,
    pub persistent: bool, // best-effort guess
    pub db_exists: bool,
}

// ---- characters ------------------------------------------------------------

/// Create/replace payload for a character. The V2/V3 fields are optional so a
/// minimal V1-style create still works; importers fill in the rest.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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
    // V2/V3 extensions
    #[serde(default)]
    pub spec_version: Option<String>,
    #[serde(default)]
    pub creator_notes: Option<String>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub post_history_instructions: Option<String>,
    #[serde(default)]
    pub mes_example: Option<String>,
    #[serde(default)]
    pub alternate_greetings: Option<Vec<String>>,
    #[serde(default)]
    pub lorebook: Option<Vec<LoreEntry>>,
}

/// Partial update — every field is optional; `None` leaves the column unchanged.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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
    #[serde(default)]
    pub spec_version: Option<String>,
    #[serde(default)]
    pub creator_notes: Option<String>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub post_history_instructions: Option<String>,
    #[serde(default)]
    pub mes_example: Option<String>,
    #[serde(default)]
    pub alternate_greetings: Option<Vec<String>>,
    #[serde(default)]
    pub lorebook: Option<Vec<LoreEntry>>,
}

/// Import a raw character-card JSON (V1/V2/V3 Tavern format). The backend (or a
/// shared parser) normalizes it into a `NewCharacterReq` and inserts it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportCardReq {
    /// The raw card JSON as a string (already extracted from PNG client-side).
    pub json: String,
    /// Optional avatar override (e.g. the PNG itself as a data-URL).
    #[serde(default)]
    pub avatar: Option<String>,
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
/// frontend can edit/delete a specific message via `/api/messages/{id}`, plus
/// alternate generations (swipes) the user can cycle between.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageView {
    pub id: i64,
    pub from_user: bool,
    pub text: String,
    /// All stored variants for this message (index 0 == `text` unless `variant`
    /// points elsewhere). Empty for user messages and legacy rows.
    #[serde(default)]
    pub variants: Vec<String>,
    /// Which variant is currently shown.
    #[serde(default)]
    pub variant: i64,
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

/// Select which stored variant (swipe) of a message is active.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SelectVariantReq {
    pub variant: i64,
}

// ---- settings --------------------------------------------------------------

/// Settings returned to the frontend. API keys are NEVER included; instead
/// `proxy_has_key` lists the config ids that currently have a saved key.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettingsResp {
    pub proxy: ProxyStore, // every config's api_key is blanked
    pub proxy_has_key: Vec<i64>,
    pub personas: PersonaStore,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SettingsReq {
    pub proxy: Option<ProxyStore>, // None = don't change
    pub personas: Option<PersonaStore>,
}
