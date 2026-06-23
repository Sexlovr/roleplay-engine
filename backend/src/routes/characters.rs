use axum::extract::{Path, State};
use axum::Json;

use shared::dto::{NewCharacterReq, UpdateCharacterReq};
use shared::types::Character;

use crate::error::AppError;
use crate::state::AppState;

/// Max length for an avatar string (plain URL or base64 `data:` URL). Data-URL
/// avatars are stored inline and echoed in every list payload, so we cap them
/// to keep the home-page JSON and DB rows from ballooning on the small HF Space.
const MAX_AVATAR_LEN: usize = 512 * 1024; // ~512 KB

fn check_avatar(avatar: Option<&str>) -> Result<(), AppError> {
    if let Some(a) = avatar {
        if a.len() > MAX_AVATAR_LEN {
            return Err(AppError::BadRequest(format!(
                "Avatar is too large ({} KB). Please use an image under {} KB, or paste an image URL instead of uploading.",
                a.len() / 1024,
                MAX_AVATAR_LEN / 1024
            )));
        }
    }
    Ok(())
}

/// GET /api/characters — list all characters, newest first.
pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<Character>>, AppError> {
    let pool = state.pool.clone();
    let chars = tokio::task::spawn_blocking(move || -> Result<Vec<Character>, AppError> {
        let conn = pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, tagline, description, personality, scenario, first_message,
                    avatar, tags, creator, messages, likes, nsfw, created_at
             FROM characters ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], row_to_character)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?;
    Ok(Json(chars?))
}

/// GET /api/characters/{id}
pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Character>, AppError> {
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || -> Result<Json<Character>, AppError> {
        let conn = pool.get()?;
        let c = conn.query_row(
            "SELECT id, name, tagline, description, personality, scenario, first_message,
                    avatar, tags, creator, messages, likes, nsfw, created_at
             FROM characters WHERE id=?1",
            [id],
            row_to_character,
        )?;
        Ok(Json(c))
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

/// POST /api/characters
pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<NewCharacterReq>,
) -> Result<Json<Character>, AppError> {
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || -> Result<Json<Character>, AppError> {
        check_avatar(body.avatar.as_deref())?;
        let conn = pool.get()?;
        let tags =
            serde_json::to_string(&body.tags.as_deref().unwrap_or(&vec![])).unwrap_or_default();
        let now = unix_now();
        conn.execute(
            "INSERT INTO characters (name, tagline, description, personality, scenario,
                 first_message, avatar, tags, creator, nsfw, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            rusqlite::params![
                body.name,
                body.tagline.as_deref().unwrap_or(""),
                body.description.as_deref().unwrap_or(""),
                body.personality.as_deref().unwrap_or(""),
                body.scenario.as_deref().unwrap_or(""),
                body.first_message.as_deref().unwrap_or(""),
                body.avatar.as_deref().unwrap_or(""),
                tags,
                body.creator.as_deref().unwrap_or(""),
                body.nsfw.unwrap_or(false) as i32,
                now,
            ],
        )?;
        let id = conn.last_insert_rowid();
        let c = conn.query_row(
            "SELECT id, name, tagline, description, personality, scenario, first_message,
                    avatar, tags, creator, messages, likes, nsfw, created_at
             FROM characters WHERE id=?1",
            [id],
            row_to_character,
        )?;
        Ok(Json(c))
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

/// PUT /api/characters/{id}
pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateCharacterReq>,
) -> Result<Json<Character>, AppError> {
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || -> Result<Json<Character>, AppError> {
        check_avatar(body.avatar.as_deref())?;
        let conn = pool.get()?;
        // Fetch current first.
        let mut c = conn.query_row(
            "SELECT id, name, tagline, description, personality, scenario, first_message,
                    avatar, tags, creator, messages, likes, nsfw, created_at
             FROM characters WHERE id=?1",
            [id],
            row_to_character,
        )?;
        if let Some(v) = body.name { c.name = v; }
        if let Some(v) = body.tagline { c.tagline = v; }
        if let Some(v) = body.description { c.description = v; }
        if let Some(v) = body.personality { c.personality = v; }
        if let Some(v) = body.scenario { c.scenario = v; }
        if let Some(v) = body.first_message { c.first_message = v; }
        if let Some(v) = body.avatar { c.avatar = v; }
        if let Some(v) = body.tags { c.tags = v; }
        if let Some(v) = body.creator { c.creator = v; }
        if let Some(v) = body.nsfw { c.nsfw = v; }
        let tags = serde_json::to_string(&c.tags)?;
        conn.execute(
            "UPDATE characters SET name=?1,tagline=?2,description=?3,personality=?4,
                 scenario=?5,first_message=?6,avatar=?7,tags=?8,creator=?9,nsfw=?10
             WHERE id=?11",
            rusqlite::params![
                c.name, c.tagline, c.description, c.personality, c.scenario,
                c.first_message, c.avatar, tags, c.creator, c.nsfw as i32, id,
            ],
        )?;
        Ok(Json(c))
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

/// DELETE /api/characters/{id}
pub async fn delete_one(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || -> Result<Json<serde_json::Value>, AppError> {
        let conn = pool.get()?;
        conn.execute("DELETE FROM characters WHERE id=?1", [id])?;
        Ok(Json(serde_json::json!({"ok": true})))
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

// --- helpers ---

fn row_to_character(row: &rusqlite::Row) -> rusqlite::Result<Character> {
    let tags_raw: String = row.get("tags")?;
    let tags: Vec<String> =
        serde_json::from_str(&tags_raw).unwrap_or_default();
    Ok(Character {
        id: row.get("id")?,
        name: row.get("name")?,
        tagline: row.get("tagline")?,
        description: row.get("description")?,
        personality: row.get("personality")?,
        scenario: row.get("scenario")?,
        first_message: row.get("first_message")?,
        avatar: row.get("avatar")?,
        tags,
        creator: row.get("creator")?,
        messages: row.get::<_, i32>("messages")? as u32,
        likes: row.get::<_, i32>("likes")? as u32,
        nsfw: row.get::<_, i32>("nsfw")? != 0,
        created_at: row.get("created_at")?,
    })
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
