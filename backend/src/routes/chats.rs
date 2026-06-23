use axum::extract::{Path, State};
use axum::Json;

use shared::dto::{
    ChatDetail, ChatListEntry, MessageView, SendMessageReq, SendMessageResp, UpdateMemoryReq,
};
use shared::template::ChatMessage;
use shared::types::{Chat, Character};

use crate::error::AppError;
use crate::llm;
use crate::state::AppState;

/// GET /api/characters/{cid}/chats — list chats for a character.
pub async fn list_for_character(
    State(state): State<AppState>,
    Path(cid): Path<i64>,
) -> Result<Json<Vec<ChatListEntry>>, AppError> {
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || -> Result<Json<Vec<ChatListEntry>>, AppError> {
        let conn = pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT c.id, c.character_id, ch.name, c.title, c.updated_at
             FROM chats c JOIN characters ch ON ch.id = c.character_id
             WHERE c.character_id=?1
             ORDER BY c.updated_at DESC",
        )?;
        let rows = stmt.query_map([cid], |row| {
            Ok(ChatListEntry {
                id: row.get(0)?,
                character_id: row.get(1)?,
                character_name: row.get(2)?,
                title: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map(Json)
            .map_err(Into::into)
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

/// POST /api/characters/{cid}/chats — create a new chat (seeds greeting as first message).
pub async fn create_chat(
    State(state): State<AppState>,
    Path(cid): Path<i64>,
) -> Result<Json<ChatDetail>, AppError> {
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || -> Result<Json<ChatDetail>, AppError> {
        let conn = pool.get()?;
        let character: Character = conn.query_row(
            "SELECT id, name, tagline, description, personality, scenario, first_message,
                    avatar, tags, creator, messages, likes, nsfw, created_at
             FROM characters WHERE id=?1",
            [cid],
            row_to_character,
        ).map_err(|_| AppError::NotFound("Character not found".into()))?;
        let now = unix_now();
        conn.execute(
            "INSERT INTO chats (character_id, title, created_at, updated_at) VALUES (?1,?2,?3,?3)",
            rusqlite::params![cid, character.name.clone(), now],
        )?;
        let chat_id = conn.last_insert_rowid();
        // Seed the greeting as the first message.
        let greeting = if character.first_message.trim().is_empty() {
            format!("*{} appears.*", character.name)
        } else {
            character.first_message.clone()
        };
        conn.execute(
            "INSERT INTO messages (chat_id, from_user, text, created_at) VALUES (?1,0,?2,?3)",
            rusqlite::params![chat_id, greeting, now],
        )?;
        // Also seed first user input column if the thread was already pre-filled? No — it's empty at creation
        // Build the response.
        let detail = build_chat_detail(&conn, chat_id)?;
        Ok(Json(detail))
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

/// GET /api/chats/{id} — full chat detail with character + messages.
pub async fn get_chat(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ChatDetail>, AppError> {
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || -> Result<Json<ChatDetail>, AppError> {
        let conn = pool.get()?;
        Ok(Json(build_chat_detail(&conn, id)?))
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

/// DELETE /api/chats/{id}
pub async fn delete_chat(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || -> Result<Json<serde_json::Value>, AppError> {
        let conn = pool.get()?;
        conn.execute("DELETE FROM chats WHERE id=?1", [id])?;
        Ok(Json(serde_json::json!({"ok": true})))
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

/// PUT /api/chats/{id}/memory
pub async fn update_memory(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateMemoryReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || -> Result<Json<serde_json::Value>, AppError> {
        let conn = pool.get()?;
        conn.execute(
            "UPDATE chats SET memory=?1, updated_at=?2 WHERE id=?3",
            rusqlite::params![body.memory, unix_now(), id],
        )?;
        Ok(Json(serde_json::json!({"ok": true})))
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

/// POST /api/chats/{id}/send — save user message, run LLM, save + return assistant reply.
pub async fn send(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<SendMessageReq>,
) -> Result<Json<SendMessageResp>, AppError> {
    let pool = state.pool.clone();
    let client = state.http.clone();
    let user_text = body.text.clone();

    // Save user message + fetch metadata (blocking), then do the LLM call (async).
    let pool2 = pool.clone();
    let (chat, character, history, user_msg) =
        tokio::task::spawn_blocking(move || -> Result<(Chat, Character, Vec<ChatMessage>, MessageView), AppError> {
            let conn = pool2.get()?;
            let now = unix_now();
            // Insert user message.
            conn.execute(
                "INSERT INTO messages (chat_id, from_user, text, created_at) VALUES (?1,1,?2,?3)",
                rusqlite::params![id, user_text, now],
            )?;
            let user_id = conn.last_insert_rowid();
            let user_msg = MessageView { id: user_id, from_user: true, text: user_text.clone() };
            // Update chat updated_at.
            conn.execute("UPDATE chats SET updated_at=?1 WHERE id=?2", rusqlite::params![now, id])?;
            // Load chat + character + full history.
            let chat = load_chat(&conn, id)?;
            let character = load_chat_character(&conn, id)?;
            let history = to_history(&load_message_views(&conn, id)?);
            Ok((chat, character, history, user_msg))
        })
        .await
        .map_err(|e| AppError::Internal(format!("join: {e}")))??;

    // Build system prompt.
    let persona = crate::routes::settings::load_persona_blocking(&pool)?;
    let system = llm::build_system(
        &character.name,
        &character.tagline,
        &persona.name,
        &persona.description,
        &chat.memory,
    );

    // Call LLM (this is async + potentially slow — DON'T block the runtime).
    let reply_text = match llm::complete(&pool, &client, &history, &system).await {
        Ok(t) => t,
        Err(e) => format!("⚠ {e}"),
    };

    // Save assistant reply.
    let pool3 = pool.clone();
    let reply = tokio::task::spawn_blocking(move || -> Result<MessageView, AppError> {
        let conn = pool3.get()?;
        let now = unix_now();
        conn.execute(
            "INSERT INTO messages (chat_id, from_user, text, created_at) VALUES (?1,0,?2,?3)",
            rusqlite::params![id, reply_text, now],
        )?;
        let reply_id = conn.last_insert_rowid();
        conn.execute("UPDATE chats SET updated_at=?1 WHERE id=?2", rusqlite::params![now, id])?;
        Ok(MessageView { id: reply_id, from_user: false, text: reply_text })
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))??;

    Ok(Json(SendMessageResp { user: user_msg, reply }))
}

/// POST /api/chats/{id}/regenerate — drop last bot message and re-run.
pub async fn regenerate(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<SendMessageResp>, AppError> {
    let pool = state.pool.clone();
    let client = state.http.clone();

    let pool2 = pool.clone();
    let (chat, character, history) =
        tokio::task::spawn_blocking(move || -> Result<(Chat, Character, Vec<ChatMessage>), AppError> {
            let conn = pool2.get()?;
            // Drop the last message if it's from the bot.
            let last_from_user: Option<bool> = conn
                .query_row("SELECT from_user FROM messages WHERE chat_id=?1 ORDER BY id DESC LIMIT 1", [id], |row| row.get(0))
                .optional()?;
            if let Some(false) = last_from_user {
                conn.execute("DELETE FROM messages WHERE id = (SELECT id FROM messages WHERE chat_id=?1 ORDER BY id DESC LIMIT 1)", [id])?;
            }
            let chat = load_chat(&conn, id)?;
            let character = load_chat_character(&conn, id)?;
            let history = to_history(&load_message_views(&conn, id)?);
            Ok((chat, character, history))
        })
        .await
        .map_err(|e| AppError::Internal(format!("join: {e}")))??;

    // Must have at least one user message to regenerate.
    let has_user = history.iter().any(|m| m.from_user);
    if !has_user {
        return Err(AppError::BadRequest("Nothing to regenerate — send a message first.".into()));
    }

    let persona = crate::routes::settings::load_persona_blocking(&pool)?;
    let system = llm::build_system(
        &character.name,
        &character.tagline,
        &persona.name,
        &persona.description,
        &chat.memory,
    );

    let reply_text = match llm::complete(&pool, &client, &history, &system).await {
        Ok(t) => t,
        Err(e) => format!("⚠ {e}"),
    };

    let pool3 = pool.clone();
    let (user_msg, reply) =
        tokio::task::spawn_blocking(move || -> Result<(MessageView, MessageView), AppError> {
            let conn = pool3.get()?;
            // Get the last user message (id + text) for the response.
            let (user_id, user_text): (i64, String) = conn.query_row(
                "SELECT id, text FROM messages WHERE chat_id=?1 AND from_user=1 ORDER BY id DESC LIMIT 1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            let now = unix_now();
            conn.execute(
                "INSERT INTO messages (chat_id, from_user, text, created_at) VALUES (?1,0,?2,?3)",
                rusqlite::params![id, reply_text, now],
            )?;
            let reply_id = conn.last_insert_rowid();
            conn.execute("UPDATE chats SET updated_at=?1 WHERE id=?2", rusqlite::params![now, id])?;
            Ok((
                MessageView { id: user_id, from_user: true, text: user_text },
                MessageView { id: reply_id, from_user: false, text: reply_text },
            ))
        })
        .await
        .map_err(|e| AppError::Internal(format!("join: {e}")))??;

    Ok(Json(SendMessageResp { user: user_msg, reply }))
}

// --- helpers ---

use rusqlite::OptionalExtension;

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn load_chat(conn: &rusqlite::Connection, id: i64) -> Result<Chat, AppError> {
    conn.query_row(
        "SELECT id, character_id, title, memory, created_at, updated_at FROM chats WHERE id=?1",
        [id],
        |row| {
            Ok(Chat {
                id: row.get(0)?,
                character_id: row.get(1)?,
                title: row.get(2)?,
                memory: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        },
    )
    .map_err(|_| AppError::NotFound("Chat not found".into()))
}

fn load_chat_character(conn: &rusqlite::Connection, chat_id: i64) -> Result<Character, AppError> {
    conn.query_row(
        "SELECT c.id, c.name, c.tagline, c.description, c.personality, c.scenario,
                c.first_message, c.avatar, c.tags, c.creator, c.messages, c.likes, c.nsfw, c.created_at
         FROM characters c JOIN chats ch ON ch.character_id = c.id
         WHERE ch.id=?1",
        [chat_id],
        row_to_character,
    )
    .map_err(|_| AppError::NotFound("Chat not found or character deleted".into()))
}

/// Map a `characters` row to a `Character` (columns selected by name).
fn row_to_character(row: &rusqlite::Row) -> rusqlite::Result<Character> {
    let tags_raw: String = row.get("tags")?;
    Ok(Character {
        id: row.get("id")?,
        name: row.get("name")?,
        tagline: row.get("tagline")?,
        description: row.get("description")?,
        personality: row.get("personality")?,
        scenario: row.get("scenario")?,
        first_message: row.get("first_message")?,
        avatar: row.get("avatar")?,
        tags: serde_json::from_str(&tags_raw).unwrap_or_default(),
        creator: row.get("creator")?,
        messages: row.get::<_, i32>("messages")? as u32,
        likes: row.get::<_, i32>("likes")? as u32,
        nsfw: row.get::<_, i32>("nsfw")? != 0,
        created_at: row.get("created_at")?,
    })
}

fn load_message_views(conn: &rusqlite::Connection, chat_id: i64) -> Result<Vec<MessageView>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, from_user, text FROM messages WHERE chat_id=?1 ORDER BY id ASC",
    )?;
    let rows = stmt.query_map([chat_id], |row| {
        Ok(MessageView {
            id: row.get(0)?,
            from_user: row.get::<_, i32>(1)? != 0,
            text: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// The LLM only needs role+text; strip ids for the templating layer.
fn to_history(views: &[MessageView]) -> Vec<ChatMessage> {
    views
        .iter()
        .map(|m| ChatMessage { from_user: m.from_user, text: m.text.clone() })
        .collect()
}

fn build_chat_detail(conn: &rusqlite::Connection, chat_id: i64) -> Result<ChatDetail, AppError> {
    let chat = load_chat(conn, chat_id)?;
    let character = load_chat_character(conn, chat_id)?;
    let messages = load_message_views(conn, chat_id)?;
    Ok(ChatDetail { chat, character, messages })
}
