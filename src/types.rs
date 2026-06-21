//! Shared data types and global page state for the AI Hub frontend.

/// A character/bot card, as shown in the gallery and chat view.
#[derive(Clone, Debug, PartialEq)]
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
}

/// Which screen is currently shown. Stored as `RwSignal<Page>` in context;
/// any component can navigate by calling `page.set(...)`.
#[derive(Clone, Debug, PartialEq)]
pub enum Page {
    Home,
    Chat(u32), // character id
}
