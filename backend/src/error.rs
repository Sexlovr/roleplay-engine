use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

/// Unified error type for all handlers. Each variant maps to an HTTP status.
#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    BadRequest(String),
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, m),
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            AppError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
        };
        (status, Json(serde_json::json!({ "error": msg }))).into_response()
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::NotFound(m) | AppError::BadRequest(m) | AppError::Internal(m) => {
                f.write_str(m)
            }
        }
    }
}

impl std::error::Error for AppError {}

// Convert common error types into AppError.
impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Internal(format!("database: {e}"))
    }
}
impl From<r2d2::Error> for AppError {
    fn from(e: r2d2::Error) -> Self {
        AppError::Internal(format!("pool: {e}"))
    }
}
impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::BadRequest(format!("invalid JSON: {e}"))
    }
}
impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError::Internal(s)
    }
}
