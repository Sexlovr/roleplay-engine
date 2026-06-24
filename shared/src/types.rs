//! Shared data types — compiles for both native (backend) and wasm32 (frontend).

use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

/// A single lorebook / world-info entry (a simplified V2/V3 `character_book`
/// entry). When any of `keys` appears in the recent conversation the `content`
/// is injected into the system prompt; an entry with no keys is always on
/// (a "constant" entry).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LoreEntry {
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default)]
    pub content: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// A character/bot card. `id` is `i64` to match SQLite INTEGER PK.
///
/// The first block mirrors a V1 Tavern card; the fields after `created_at` are
/// the V2/V3 extensions (all `#[serde(default)]` so older saved rows keep
/// loading after an upgrade).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Character {
    pub id: i64,
    pub name: String,
    /// Short one-line hook shown under the name on the card.
    pub tagline: String,
    /// Longer description / blurb for the character page.
    pub description: String,
    /// Core personality definition.
    pub personality: String,
    /// Scenario / setting context.
    pub scenario: String,
    /// The character's opening line in a new chat (seeded as the first message).
    pub first_message: String,
    pub avatar: String,
    pub tags: Vec<String>,
    pub creator: String,
    pub messages: u32,
    pub likes: u32,
    pub nsfw: bool,
    pub created_at: i64,

    // ---- V2 / V3 card extensions ----
    /// Card spec this character was authored/imported as: "" (v1), "2.0", "3.0".
    #[serde(default)]
    pub spec_version: String,
    /// Out-of-character notes from the creator (not sent to the model).
    #[serde(default)]
    pub creator_notes: String,
    /// Character-level system prompt / instructions.
    #[serde(default)]
    pub system_prompt: String,
    /// Instructions injected after the chat history (a.k.a. jailbreak / UJB).
    #[serde(default)]
    pub post_history_instructions: String,
    /// Example dialogue demonstrating the character's voice.
    #[serde(default)]
    pub mes_example: String,
    /// Additional opening lines the user can swipe between.
    #[serde(default)]
    pub alternate_greetings: Vec<String>,
    /// Embedded world-info / lorebook entries.
    #[serde(default)]
    pub lorebook: Vec<LoreEntry>,
}

/// A saved chat session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chat {
    pub id: i64,
    pub character_id: i64,
    pub title: String,
    pub memory: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// The user's roleplay identity, injected into the chat system prompt.
/// Multiple personas can be saved; one is active at a time.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Persona {
    #[serde(default)]
    pub id: i64,
    pub name: String,
    pub description: String,
    /// Optional avatar (data-URL or image URL).
    #[serde(default)]
    pub avatar: String,
}

/// Persisted collection of personas plus which one is active.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PersonaStore {
    #[serde(default)]
    pub personas: Vec<Persona>,
    #[serde(default)]
    pub active: i64,
}
