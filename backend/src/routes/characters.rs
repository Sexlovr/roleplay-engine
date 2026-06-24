use axum::extract::{Path, State};
use axum::Json;

use shared::card;
use shared::dto::{ImportCardReq, NewCharacterReq, UpdateCharacterReq};
use shared::types::Character;

use crate::db::{row_to_character, CHARACTER_COLUMNS};
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

fn select_one(conn: &rusqlite::Connection, id: i64) -> Result<Character, AppError> {
    conn.query_row(
        &format!("SELECT {CHARACTER_COLUMNS} FROM characters WHERE id=?1"),
        [id],
        row_to_character,
    )
    .map_err(|_| AppError::NotFound("Character not found".into()))
}

/// GET /api/characters — list all characters, newest first.
pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<Character>>, AppError> {
    let pool = state.pool.clone();
    let chars = tokio::task::spawn_blocking(move || -> Result<Vec<Character>, AppError> {
        let conn = pool.get()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {CHARACTER_COLUMNS} FROM characters ORDER BY created_at DESC"
        ))?;
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
        Ok(Json(select_one(&conn, id)?))
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

/// Insert a character from a normalized create request, returning the new row.
fn insert_character(
    conn: &rusqlite::Connection,
    body: &NewCharacterReq,
) -> Result<Character, AppError> {
    let tags = serde_json::to_string(body.tags.as_deref().unwrap_or(&[])).unwrap_or_default();
    let greetings =
        serde_json::to_string(body.alternate_greetings.as_deref().unwrap_or(&[])).unwrap_or_default();
    let lore = serde_json::to_string(body.lorebook.as_deref().unwrap_or(&[])).unwrap_or_default();
    let now = unix_now();
    conn.execute(
        "INSERT INTO characters
            (name, tagline, description, personality, scenario, first_message, avatar,
             tags, creator, nsfw, created_at, spec_version, creator_notes, system_prompt,
             post_history_instructions, mes_example, alternate_greetings, lorebook)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
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
            body.nsfw.unwrap_or(false) as i64,
            now,
            body.spec_version.as_deref().unwrap_or(""),
            body.creator_notes.as_deref().unwrap_or(""),
            body.system_prompt.as_deref().unwrap_or(""),
            body.post_history_instructions.as_deref().unwrap_or(""),
            body.mes_example.as_deref().unwrap_or(""),
            greetings,
            lore,
        ],
    )?;
    let id = conn.last_insert_rowid();
    select_one(conn, id)
}

/// POST /api/characters
pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<NewCharacterReq>,
) -> Result<Json<Character>, AppError> {
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || -> Result<Json<Character>, AppError> {
        if body.name.trim().is_empty() {
            return Err(AppError::BadRequest("Character name is required.".into()));
        }
        check_avatar(body.avatar.as_deref())?;
        let conn = pool.get()?;
        Ok(Json(insert_character(&conn, &body)?))
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

/// POST /api/characters/import — import a Tavern V1/V2/V3 card (JSON string,
/// possibly extracted from a PNG client-side).
pub async fn import_card(
    State(state): State<AppState>,
    Json(body): Json<ImportCardReq>,
) -> Result<Json<Character>, AppError> {
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || -> Result<Json<Character>, AppError> {
        let req = card::parse_card(&body.json, body.avatar.clone())
            .map_err(AppError::BadRequest)?;
        check_avatar(req.avatar.as_deref())?;
        let conn = pool.get()?;
        Ok(Json(insert_character(&conn, &req)?))
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
        let mut c = select_one(&conn, id)?;
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
        if let Some(v) = body.spec_version { c.spec_version = v; }
        if let Some(v) = body.creator_notes { c.creator_notes = v; }
        if let Some(v) = body.system_prompt { c.system_prompt = v; }
        if let Some(v) = body.post_history_instructions { c.post_history_instructions = v; }
        if let Some(v) = body.mes_example { c.mes_example = v; }
        if let Some(v) = body.alternate_greetings { c.alternate_greetings = v; }
        if let Some(v) = body.lorebook { c.lorebook = v; }
        let tags = serde_json::to_string(&c.tags)?;
        let greetings = serde_json::to_string(&c.alternate_greetings)?;
        let lore = serde_json::to_string(&c.lorebook)?;
        conn.execute(
            "UPDATE characters SET name=?1,tagline=?2,description=?3,personality=?4,
                 scenario=?5,first_message=?6,avatar=?7,tags=?8,creator=?9,nsfw=?10,
                 spec_version=?11,creator_notes=?12,system_prompt=?13,
                 post_history_instructions=?14,mes_example=?15,alternate_greetings=?16,lorebook=?17
             WHERE id=?18",
            rusqlite::params![
                c.name, c.tagline, c.description, c.personality, c.scenario,
                c.first_message, c.avatar, tags, c.creator, c.nsfw as i64,
                c.spec_version, c.creator_notes, c.system_prompt,
                c.post_history_instructions, c.mes_example, greetings, lore, id,
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

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
