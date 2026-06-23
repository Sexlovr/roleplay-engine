use crate::db::DbPool;

/// Shared application state passed to every handler via axum `State<AppState>`.
#[derive(Clone)]
pub struct AppState {
    pub pool: DbPool,
    pub http: reqwest::Client,
    pub data_dir: String,
    pub persistent: bool,
}
