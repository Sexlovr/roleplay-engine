//! Shared data types — compiles for both native (backend) and wasm32 (frontend).

use serde::{Deserialize, Serialize};

/// A character/bot card. `id` is `i64` to match SQLite INTEGER PK.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Character {
    pub id: i64,
    pub name: String,
    /// Short one-line hook shown under the name on the card.
    pub tagline: String,
    /// Longer description / blurb for the character page.
    pub description: String,
    /// System prompt: core personality definition.
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
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Persona {
    pub name: String,
    pub description: String,
}
