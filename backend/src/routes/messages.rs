use axum::extract::{Path, State};
use axum::Json;
use rusqlite::OptionalExtension;

use shared::dto::{EditMessageReq, SelectVariantReq};

use crate::error::AppError;
use crate::state::AppState;

/// PUT /api/messages/{id} — edit the text of a message. The edit also replaces
/// the currently-selected swipe variant so the two stay consistent.
pub async fn edit(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<EditMessageReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || -> Result<Json<serde_json::Value>, AppError> {
        let conn = pool.get()?;
        // Keep the active variant in sync with the edited text.
        let row: Option<(String, i64)> = conn
            .query_row(
                "SELECT variants, variant FROM messages WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((variants_raw, cur)) = row else {
            return Err(AppError::NotFound("Message not found".into()));
        };
        let mut variants: Vec<String> = serde_json::from_str(&variants_raw).unwrap_or_default();
        if let Some(slot) = variants.get_mut(cur.max(0) as usize) {
            *slot = body.text.clone();
        }
        let variants_json = serde_json::to_string(&variants).unwrap_or(variants_raw);
        conn.execute(
            "UPDATE messages SET text=?1, variants=?2 WHERE id=?3",
            rusqlite::params![body.text, variants_json, id],
        )?;
        Ok(Json(serde_json::json!({"ok": true})))
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

/// PUT /api/messages/{id}/variant — switch which stored swipe is shown.
pub async fn select_variant(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<SelectVariantReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || -> Result<Json<serde_json::Value>, AppError> {
        let conn = pool.get()?;
        let variants_raw: Option<String> = conn
            .query_row("SELECT variants FROM messages WHERE id=?1", [id], |r| r.get(0))
            .optional()?;
        let Some(variants_raw) = variants_raw else {
            return Err(AppError::NotFound("Message not found".into()));
        };
        let variants: Vec<String> = serde_json::from_str(&variants_raw).unwrap_or_default();
        if variants.is_empty() {
            return Err(AppError::BadRequest("Message has no variants to switch.".into()));
        }
        let idx = body.variant.clamp(0, variants.len() as i64 - 1);
        let text = variants[idx as usize].clone();
        conn.execute(
            "UPDATE messages SET text=?1, variant=?2 WHERE id=?3",
            rusqlite::params![text, idx, id],
        )?;
        Ok(Json(serde_json::json!({"ok": true, "text": text, "variant": idx})))
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}

/// DELETE /api/messages/{id} — delete a message. Refuses if it would empty the chat.
pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || -> Result<Json<serde_json::Value>, AppError> {
        let conn = pool.get()?;
        // Get the chat so we can check message count.
        let chat_id: i64 = conn
            .query_row("SELECT chat_id FROM messages WHERE id=?1", [id], |row| row.get(0))
            .map_err(|_| AppError::NotFound("Message not found".into()))?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE chat_id=?1",
            [chat_id],
            |row| row.get(0),
        )?;
        if count <= 1 {
            return Err(AppError::BadRequest("Cannot delete the last message in a chat.".into()));
        }
        conn.execute("DELETE FROM messages WHERE id=?1", [id])?;
        Ok(Json(serde_json::json!({"ok": true})))
    })
    .await
    .map_err(|e| AppError::Internal(format!("join: {e}")))?
}
