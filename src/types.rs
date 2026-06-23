//! Shared data types and global page state.

use serde::{Deserialize, Serialize};

/// A character/bot card, as shown in the gallery, detail page, and chat view.
/// `Serialize`/`Deserialize` so user-created characters persist to localStorage.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Character {
    pub id: u32,
    pub name: String,
    /// Short one-line hook shown under the name on the card.
    pub tagline: String,
    /// Longer intro; used as the character's opening message in chat.
    pub description: String,
    /// Avatar image URL.
    pub avatar: String,
    pub tags: Vec<String>,
    pub creator: String,
    /// Total chat/message count (for the card meta row).
    pub messages: u32,
    pub likes: u32,
    pub nsfw: bool,
}

/// A single chat bubble.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatMessage {
    pub from_user: bool,
    pub text: String,
    /// True while this bubble is the "…" placeholder awaiting an API reply.
    /// The async completion locates the placeholder by this flag rather than a
    /// captured index, so edits/deletes during the request can't misdirect it.
    pub pending: bool,
}

/// The user's roleplay identity, injected into the chat system prompt.
/// Persisted to localStorage.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Persona {
    pub name: String,
    pub description: String,
}

/// Which screen is currently shown. Stored as `RwSignal<Page>` in context;
/// any component can navigate by calling `page.set(...)`.
#[derive(Clone, Debug, PartialEq)]
pub enum Page {
    Home,
    Character(u32), // character id — detail page
    Chat(u32),      // character id
    Create,         // create-a-character form
}
