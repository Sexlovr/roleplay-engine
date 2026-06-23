use axum::extract::{Path, State};
use axum::Json;

use shared::dto::EditMessageReq;

use crate::error::AppError;
use crate::state::AppState;

/// PUT /api/messages/{id} — edit the text of a message.
pub async fn edit(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<EditMessageReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    let pool = state.pool.clone();
    tokio::task::spawn_blocking(move || -> Result<Json<serde_json::Value>, AppError> {
        let conn = pool.get()?;
        let affected = conn.execute("UPDATE messages SET text=?1 WHERE id=?2", rusqlite::params![body.text, id])?;
        if affected == 0 {
            return Err(AppError::NotFound("Message not found".into()));
        }
        Ok(Json(serde_json::json!({"ok": true})))
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
