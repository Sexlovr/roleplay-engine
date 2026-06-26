use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::response::Response;
use axum::Json;

use shared::dto::{
    ChatDetail, ChatListEntry, MessageView, RenameChatReq, SendMessageReq, SendMessageResp,
    StreamMsg, UpdateMemoryReq,
};
use shared::template::ChatMessage;
use shared::types::{Chat, Character};

use crate::db::{row_to_character, CHARACTER_COLUMNS};
use crate::error::AppError;
use crate::llm;
use crate::state::AppState;

/// The SELECT columns shared by both chat-list queries. The two correlated
/// subqueries pull the most-recent message text + sender so each list row can
/// show a preview snippet without a second round-trip.
const CHAT_LIST_COLUMNS: &str = "c.id, c.character_id, ch.name, ch.avatar, c.title, c.updated_at, \
     COALESCE((SELECT text FROM messages WHERE chat_id=c.id ORDER BY id DESC LIMIT 1), ''), \
     COALESCE((SELECT from_user FROM messages WHERE chat_id=c.id ORDER BY id DESC LIMIT 1), 0)";

/// Map a row selected with [`CHAT_LIST_COLUMNS`] into a [`ChatListEntry`].
fn row_to_chat_entry(row: &rusqlite::Row) -> rusqlite::Result<ChatListEntry> {
    Ok(ChatListEntry {
        id: row.get(0)?,
        character_id: row.get(1)?,
        character_name: row.get(2)?,
        avatar: row.get(3)?,
        title: row.get(4)?,
        updated_at: row.get(5)?,
        last_message: row.get(6)?,
        last_from_user: row.get::<_, i64>(7)? != 0,
    })
}

/// GET /api/characters/{cid}/chats — list chats for a character.
pub async fn list_for_character(
    State(state): State<AppState>,
    Path(cid): Path<i64>,
) -> Result<Json<Vec<ChatListEntry>>, AppError> {
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || -> Result<Json<Vec<ChatListEntry>>, AppError> {
        let conn = pool.get()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {CHAT_LIST_COLUMNS}
             FROM chats c JOIN characters ch ON ch.id = c.character_id
             WHERE c.character_id=?1
             ORDER BY c.updated_at DESC"
        ))?;
        let rows = stmt.query_map([cid], row_to_chat_entry)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map(Json)
            .map_err(Into::into)
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

/// GET /api/chats — recent chats across every character (newest first), for the
/// global "Chats" tab. Capped so a long history can't balloon the payload.
pub async fn list_recent(
    State(state): State<AppState>,
) -> Result<Json<Vec<ChatListEntry>>, AppError> {
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || -> Result<Json<Vec<ChatListEntry>>, AppError> {
        let conn = pool.get()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {CHAT_LIST_COLUMNS}
             FROM chats c JOIN characters ch ON ch.id = c.character_id
             ORDER BY c.updated_at DESC
             LIMIT 200"
        ))?;
        let rows = stmt.query_map([], row_to_chat_entry)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map(Json)
            .map_err(Into::into)
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

/// PUT /api/chats/{id}/title — rename a chat session.
pub async fn rename(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<RenameChatReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || -> Result<Json<serde_json::Value>, AppError> {
        let conn = pool.get()?;
        conn.execute(
            "UPDATE chats SET title=?1 WHERE id=?2",
            rusqlite::params![body.title.trim(), id],
        )?;
        Ok(Json(serde_json::json!({"ok": true})))
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

    // Run the LLM. A failure is returned as a transient `error` (NOT persisted):
    // the user message stays, but no error bubble is written, so the log and
    // swipe variants are never polluted. The frontend shows a dismissable banner
    // with a Retry action.
    let (reply, error) = match llm::complete(
        &pool, &client, &history, &system, &character.post_history_instructions,
    )
    .await
    {
        Ok(reply_text) => {
            let pool3 = pool.clone();
            let reply = tokio::task::spawn_blocking(move || -> Result<MessageView, AppError> {
                let conn = pool3.get()?;
                let now = unix_now();
                let variants_json =
                    serde_json::to_string(&vec![reply_text.clone()]).unwrap_or_default();
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
            (Some(reply), None)
        }
        Err(e) => (None, Some(e)),
    };

    Ok(Json(SendMessageResp { user: user_msg, reply, error }))
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

    // An LLM error is returned as a transient `error` (not persisted): the
    // existing reply is left untouched so nothing is lost. The frontend restores
    // it and shows a dismissable banner — regenerate must never bake an error
    // into a message's swipe variants.
    let reply_text = match llm::complete(
        &pool, &client, &history, &system, &character.post_history_instructions,
    )
    .await
    {
        Ok(t) => t,
        Err(e) => {
            return Ok(Json(SendMessageResp {
                user: MessageView {
                    id: 0,
                    from_user: true,
                    text: String::new(),
                    variants: Vec::new(),
                    variant: 0,
                },
                reply: None,
                error: Some(e),
            }));
        }
    };

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

    Ok(Json(SendMessageResp { user: user_msg, reply: Some(reply), error: None }))
}

/// Wrap an [`StreamMsg`] receiver as an NDJSON HTTP response — one JSON object
/// per line. `X-Accel-Buffering: no` defeats reverse-proxy buffering (HF Spaces'
/// nginx) so tokens reach the browser as they're produced.
fn ndjson_stream(rx: tokio::sync::mpsc::UnboundedReceiver<StreamMsg>) -> Response {
    use tokio_stream::StreamExt;
    let stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx).map(|m| {
        let mut line = serde_json::to_string(&m).unwrap_or_default();
        line.push('\n');
        Ok::<_, std::convert::Infallible>(Bytes::from(line))
    });
    Response::builder()
        .header("Content-Type", "application/x-ndjson")
        .header("Cache-Control", "no-cache")
        .header("X-Accel-Buffering", "no")
        .body(Body::from_stream(stream))
        .unwrap()
}

/// POST /api/chats/{id}/send/stream — like [`send`], but streams the reply token
/// by token as NDJSON. The user message is persisted up front (and its id sent
/// as the first frame); the assistant reply is generated by [`llm::stream_complete`]
/// and persisted when the stream finishes, then announced with a `done` frame.
///
/// The generation runs in a detached task, so a client that disconnects (e.g.
/// taps "Stop") doesn't abort the save — the full reply still lands in the DB
/// and shows up on reload.
pub async fn send_stream(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<SendMessageReq>,
) -> Response {
    let pool = state.pool.clone();
    let client = state.http.clone();
    let user_text = body.text.clone();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<StreamMsg>();

    let pool2 = pool.clone();
    let setup = tokio::task::spawn_blocking(
        move || -> Result<(Chat, Character, Vec<ChatMessage>, i64), AppError> {
            let conn = pool2.get()?;
            let now = unix_now();
            conn.execute(
                "INSERT INTO messages (chat_id, from_user, text, created_at) VALUES (?1,1,?2,?3)",
                rusqlite::params![id, user_text, now],
            )?;
            let user_id = conn.last_insert_rowid();
            conn.execute("UPDATE chats SET updated_at=?1 WHERE id=?2", rusqlite::params![now, id])?;
            let chat = load_chat(&conn, id)?;
            let character = load_chat_character(&conn, id)?;
            let history = to_history(&load_message_views(&conn, id)?);
            Ok((chat, character, history, user_id))
        },
    )
    .await;

    match setup {
        Ok(Ok((chat, character, history, user_id))) => {
            let _ = tx.send(StreamMsg::User { id: user_id });
            let persona = crate::routes::settings::load_active_persona(&pool);
            let system = llm::build_system(&character, &persona, &chat.memory, &history);
            let post = character.post_history_instructions.clone();
            let pool3 = pool.clone();
            tokio::spawn(async move {
                match llm::stream_complete(&pool3, &client, &history, &system, &post, &tx).await {
                    Ok(full) => match insert_reply(&pool3, id, full).await {
                        Ok((rid, variants)) => {
                            let _ = tx.send(StreamMsg::Done { id: rid, variants, variant: 0 });
                        }
                        Err(e) => {
                            let _ = tx.send(StreamMsg::Error { v: e.to_string() });
                        }
                    },
                    Err(e) => {
                        let _ = tx.send(StreamMsg::Error { v: e });
                    }
                }
            });
        }
        Ok(Err(e)) => {
            let _ = tx.send(StreamMsg::Error { v: e.to_string() });
        }
        Err(e) => {
            let _ = tx.send(StreamMsg::Error { v: format!("join: {e}") });
        }
    }
    ndjson_stream(rx)
}

/// POST /api/chats/{id}/regenerate/stream — streaming twin of [`regenerate`].
/// Generates a new swipe variant of the trailing bot message (or a fresh reply
/// if none), streaming tokens as it goes; the `done` frame carries the full
/// variant list + active index so the client can wire up swipe controls.
pub async fn regenerate_stream(State(state): State<AppState>, Path(id): Path<i64>) -> Response {
    let pool = state.pool.clone();
    let client = state.http.clone();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<StreamMsg>();

    let pool2 = pool.clone();
    let setup = tokio::task::spawn_blocking(
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
            let mut views = load_message_views(&conn, id)?;
            if let Some(bot_id) = last_bot_id {
                views.retain(|m| m.id != bot_id);
            }
            let history = to_history(&views);
            Ok((chat, character, history, last_bot_id))
        },
    )
    .await;

    match setup {
        Ok(Ok((chat, character, history, last_bot_id))) => {
            if !history.iter().any(|m| m.from_user) {
                let _ = tx.send(StreamMsg::Error {
                    v: "Nothing to regenerate — send a message first.".into(),
                });
                return ndjson_stream(rx);
            }
            let persona = crate::routes::settings::load_active_persona(&pool);
            let system = llm::build_system(&character, &persona, &chat.memory, &history);
            let post = character.post_history_instructions.clone();
            let pool3 = pool.clone();
            tokio::spawn(async move {
                match llm::stream_complete(&pool3, &client, &history, &system, &post, &tx).await {
                    Ok(full) => match append_variant(&pool3, id, last_bot_id, full).await {
                        Ok((rid, variants, variant)) => {
                            let _ = tx.send(StreamMsg::Done { id: rid, variants, variant });
                        }
                        Err(e) => {
                            let _ = tx.send(StreamMsg::Error { v: e.to_string() });
                        }
                    },
                    Err(e) => {
                        let _ = tx.send(StreamMsg::Error { v: e });
                    }
                }
            });
        }
        Ok(Err(e)) => {
            let _ = tx.send(StreamMsg::Error { v: e.to_string() });
        }
        Err(e) => {
            let _ = tx.send(StreamMsg::Error { v: format!("join: {e}") });
        }
    }
    ndjson_stream(rx)
}

/// Persist a freshly-generated reply as a new bot message. Returns its id and
/// single-element variant list. Shared by [`send_stream`].
async fn insert_reply(
    pool: &r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>,
    chat_id: i64,
    text: String,
) -> Result<(i64, Vec<String>), AppError> {
    let pool = pool.clone();
    tokio::task::spawn_blocking(move || -> Result<(i64, Vec<String>), AppError> {
        let conn = pool.get()?;
        let now = unix_now();
        let variants = vec![text.clone()];
        let variants_json = serde_json::to_string(&variants).unwrap_or_default();
        conn.execute(
            "INSERT INTO messages (chat_id, from_user, text, variants, variant, created_at)
             VALUES (?1,0,?2,?3,0,?4)",
            rusqlite::params![chat_id, text, variants_json, now],
        )?;
        let rid = conn.last_insert_rowid();
        conn.execute("UPDATE chats SET updated_at=?1 WHERE id=?2", rusqlite::params![now, chat_id])?;
        Ok((rid, variants))
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

/// Persist a regenerated reply: append it as a swipe variant of `last_bot_id`
/// (seeding the existing text first for legacy rows), or insert a fresh message
/// if there's no trailing bot turn. Returns `(message_id, variants, active_idx)`.
async fn append_variant(
    pool: &r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>,
    chat_id: i64,
    last_bot_id: Option<i64>,
    text: String,
) -> Result<(i64, Vec<String>, i64), AppError> {
    let pool = pool.clone();
    tokio::task::spawn_blocking(move || -> Result<(i64, Vec<String>, i64), AppError> {
        let conn = pool.get()?;
        let now = unix_now();
        let out = match last_bot_id {
            Some(bot_id) => {
                let variants_raw: String = conn
                    .query_row("SELECT variants FROM messages WHERE id=?1", [bot_id], |r| r.get(0))?;
                let mut variants: Vec<String> =
                    serde_json::from_str(&variants_raw).unwrap_or_default();
                if variants.is_empty() {
                    let cur_text: String = conn
                        .query_row("SELECT text FROM messages WHERE id=?1", [bot_id], |r| r.get(0))?;
                    variants.push(cur_text);
                }
                variants.push(text.clone());
                let new_idx = (variants.len() - 1) as i64;
                let variants_json = serde_json::to_string(&variants).unwrap_or_default();
                conn.execute(
                    "UPDATE messages SET text=?1, variants=?2, variant=?3 WHERE id=?4",
                    rusqlite::params![text, variants_json, new_idx, bot_id],
                )?;
                (bot_id, variants, new_idx)
            }
            None => {
                let variants = vec![text.clone()];
                let variants_json = serde_json::to_string(&variants).unwrap_or_default();
                conn.execute(
                    "INSERT INTO messages (chat_id, from_user, text, variants, variant, created_at)
                     VALUES (?1,0,?2,?3,0,?4)",
                    rusqlite::params![chat_id, text, variants_json, now],
                )?;
                (conn.last_insert_rowid(), variants, 0)
            }
        };
        conn.execute("UPDATE chats SET updated_at=?1 WHERE id=?2", rusqlite::params![now, chat_id])?;
        Ok(out)
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
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
