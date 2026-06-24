use axum::extract::{Path, State};
use axum::Json;

use shared::dto::{
    ChatDetail, ChatListEntry, MessageView, SendMessageReq, SendMessageResp, UpdateMemoryReq,
};
use shared::template::ChatMessage;
use shared::types::{Chat, Character};

use crate::db::{row_to_character, CHARACTER_COLUMNS};
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
        let character: Character = conn
            .query_row(
                &format!("SELECT {CHARACTER_COLUMNS} FROM characters WHERE id=?1"),
                [cid],
                row_to_character,
            )
            .map_err(|_| AppError::NotFound("Character not found".into()))?;
        let now = unix_now();
        conn.execute(
            "INSERT INTO chats (character_id, title, created_at, updated_at) VALUES (?1,?2,?3,?3)",
            rusqlite::params![cid, character.name.clone(), now],
        )?;
        let chat_id = conn.last_insert_rowid();
        // Seed the greeting. The first message plus any alternate greetings
        // become swipe variants the user can cycle through.
        let base = if character.first_message.trim().is_empty() {
            format!("*{} appears.*", character.name)
        } else {
            character.first_message.clone()
        };
        let mut variants = vec![base.clone()];
        for g in &character.alternate_greetings {
            if !g.trim().is_empty() {
                variants.push(g.clone());
            }
        }
        let variants_json = serde_json::to_string(&variants).unwrap_or_else(|_| "[]".into());
        conn.execute(
            "INSERT INTO messages (chat_id, from_user, text, variants, variant, created_at)
             VALUES (?1,0,?2,?3,0,?4)",
            rusqlite::params![chat_id, base, variants_json, now],
        )?;
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

    let pool2 = pool.clone();
    let (chat, character, history, user_msg) = tokio::task::spawn_blocking(
        move || -> Result<(Chat, Character, Vec<ChatMessage>, MessageView), AppError> {
            let conn = pool2.get()?;
            let now = unix_now();
            conn.execute(
                "INSERT INTO messages (chat_id, from_user, text, created_at) VALUES (?1,1,?2,?3)",
                rusqlite::params![id, user_text, now],
            )?;
            let user_id = conn.last_insert_rowid();
            let user_msg = MessageView {
                id: user_id,
                from_user: true,
                text: user_text.clone(),
                variants: Vec::new(),
                variant: 0,
            };
            conn.execute("UPDATE chats SET updated_at=?1 WHERE id=?2", rusqlite::params![now, id])?;
            let chat = load_chat(&conn, id)?;
            let character = load_chat_character(&conn, id)?;
            let history = to_history(&load_message_views(&conn, id)?);
            Ok((chat, character, history, user_msg))
        },
    )
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))??;

    let persona = crate::routes::settings::load_active_persona(&pool);
    let system = llm::build_system(&character, &persona, &chat.memory, &history);

    // A fresh message: masking an LLM error as the bubble text is acceptable
    // here (nothing pre-existing is polluted) and lets the user see what failed.
    let reply_text = match llm::complete(
        &pool, &client, &history, &system, &character.post_history_instructions,
    )
    .await
    {
        Ok(t) => t,
        Err(e) => format!("\u{26A0} {e}"),
    };

    let pool3 = pool.clone();
    let reply = tokio::task::spawn_blocking(move || -> Result<MessageView, AppError> {
        let conn = pool3.get()?;
        let now = unix_now();
        let variants_json = serde_json::to_string(&vec![reply_text.clone()]).unwrap_or_default();
        conn.execute(
            "INSERT INTO messages (chat_id, from_user, text, variants, variant, created_at)
             VALUES (?1,0,?2,?3,0,?4)",
            rusqlite::params![id, reply_text, variants_json, now],
        )?;
        let reply_id = conn.last_insert_rowid();
        conn.execute("UPDATE chats SET updated_at=?1 WHERE id=?2", rusqlite::params![now, id])?;
        Ok(MessageView {
            id: reply_id,
            from_user: false,
            text: reply_text.clone(),
            variants: vec![reply_text],
            variant: 0,
        })
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))??;

    Ok(Json(SendMessageResp { user: user_msg, reply }))
}

/// POST /api/chats/{id}/regenerate — generate a NEW variant (swipe) of the last
/// bot message, preserving the previous one so the user can swipe between them.
pub async fn regenerate(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<SendMessageResp>, AppError> {
    let pool = state.pool.clone();
    let client = state.http.clone();

    // Identify the trailing bot message (if any) to attach the new swipe to,
    // and build the generation context from the history *excluding* it.
    let pool2 = pool.clone();
    let (chat, character, history, last_bot_id) = tokio::task::spawn_blocking(
        move || -> Result<(Chat, Character, Vec<ChatMessage>, Option<i64>), AppError> {
            let conn = pool2.get()?;
            let last: Option<(i64, bool)> = conn
                .query_row(
                    "SELECT id, from_user FROM messages WHERE chat_id=?1 ORDER BY id DESC LIMIT 1",
                    [id],
                    |row| Ok((row.get(0)?, row.get::<_, i64>(1)? != 0)),
                )
                .optional()?;
            let last_bot_id = match last {
                Some((mid, false)) => Some(mid),
                _ => None,
            };
            let chat = load_chat(&conn, id)?;
            let character = load_chat_character(&conn, id)?;
            // History for generation excludes the trailing bot message.
            let mut views = load_message_views(&conn, id)?;
            if let Some(bot_id) = last_bot_id {
                views.retain(|m| m.id != bot_id);
            }
            let history = to_history(&views);
            Ok((chat, character, history, last_bot_id))
        },
    )
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))??;

    if !history.iter().any(|m| m.from_user) {
        return Err(AppError::BadRequest("Nothing to regenerate — send a message first.".into()));
    }

    let persona = crate::routes::settings::load_active_persona(&pool);
    let system = llm::build_system(&character, &persona, &chat.memory, &history);

    // Propagate an LLM error instead of masking it: regenerate mutates an
    // EXISTING message's variants, so persisting an error banner would bake it
    // in as a permanent swipe. Returning Err lets the frontend restore the prior
    // reply and show a transient, un-persisted error instead.
    let reply_text = llm::complete(
        &pool, &client, &history, &system, &character.post_history_instructions,
    )
    .await
    .map_err(AppError::Internal)?;

    let pool3 = pool.clone();
    let (user_msg, reply) = tokio::task::spawn_blocking(
        move || -> Result<(MessageView, MessageView), AppError> {
            let conn = pool3.get()?;
            let now = unix_now();
            let reply = match last_bot_id {
                // Append the new generation as a swipe variant of the existing message.
                Some(bot_id) => {
                    let (variants_raw, _cur): (String, i64) = conn.query_row(
                        "SELECT variants, variant FROM messages WHERE id=?1",
                        [bot_id],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )?;
                    let mut variants: Vec<String> =
                        serde_json::from_str(&variants_raw).unwrap_or_default();
                    if variants.is_empty() {
                        // Legacy row: seed with the current text first.
                        let cur_text: String = conn
                            .query_row("SELECT text FROM messages WHERE id=?1", [bot_id], |r| r.get(0))?;
                        variants.push(cur_text);
                    }
                    variants.push(reply_text.clone());
                    let new_idx = (variants.len() - 1) as i64;
                    let variants_json = serde_json::to_string(&variants).unwrap_or_default();
                    conn.execute(
                        "UPDATE messages SET text=?1, variants=?2, variant=?3 WHERE id=?4",
                        rusqlite::params![reply_text, variants_json, new_idx, bot_id],
                    )?;
                    MessageView {
                        id: bot_id,
                        from_user: false,
                        text: reply_text,
                        variants,
                        variant: new_idx,
                    }
                }
                // No trailing bot message — insert a fresh reply.
                None => {
                    let variants_json =
                        serde_json::to_string(&vec![reply_text.clone()]).unwrap_or_default();
                    conn.execute(
                        "INSERT INTO messages (chat_id, from_user, text, variants, variant, created_at)
                         VALUES (?1,0,?2,?3,0,?4)",
                        rusqlite::params![id, reply_text, variants_json, now],
                    )?;
                    let rid = conn.last_insert_rowid();
                    MessageView {
                        id: rid,
                        from_user: false,
                        text: reply_text.clone(),
                        variants: vec![reply_text],
                        variant: 0,
                    }
                }
            };
            conn.execute("UPDATE chats SET updated_at=?1 WHERE id=?2", rusqlite::params![now, id])?;
            // Echo the last user message for parity with `send`.
            let user = conn
                .query_row(
                    "SELECT id, text FROM messages WHERE chat_id=?1 AND from_user=1 ORDER BY id DESC LIMIT 1",
                    [id],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            let user_msg = match user {
                Some((uid, utext)) => MessageView {
                    id: uid,
                    from_user: true,
                    text: utext,
                    variants: Vec::new(),
                    variant: 0,
                },
                None => MessageView { id: 0, from_user: true, text: String::new(), variants: Vec::new(), variant: 0 },
            };
            Ok((user_msg, reply))
        },
    )
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
        &format!(
            "SELECT {} FROM characters c JOIN chats ch ON ch.character_id = c.id WHERE ch.id=?1",
            CHARACTER_COLUMNS
                .split(',')
                .map(|s| format!("c.{}", s.trim()))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        [chat_id],
        row_to_character,
    )
    .map_err(|_| AppError::NotFound("Chat not found or character deleted".into()))
}

fn load_message_views(conn: &rusqlite::Connection, chat_id: i64) -> Result<Vec<MessageView>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, from_user, text, variants, variant FROM messages WHERE chat_id=?1 ORDER BY id ASC",
    )?;
    let rows = stmt.query_map([chat_id], |row| {
        let variants_raw: String = row.get(3)?;
        let variants: Vec<String> = serde_json::from_str(&variants_raw).unwrap_or_default();
        Ok(MessageView {
            id: row.get(0)?,
            from_user: row.get::<_, i64>(1)? != 0,
            text: row.get(2)?,
            variants,
            variant: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// The LLM only needs role+text; strip ids/variants for the templating layer.
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
