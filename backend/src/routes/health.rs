use axum::{extract::State, Json};
use shared::dto::HealthResp;

use crate::error::AppError;
use crate::state::AppState;

pub async fn health(State(state): State<AppState>) -> Result<Json<HealthResp>, AppError> {
    let db_exists = std::path::Path::new(&state.data_dir).join("roleplay.db").exists();
    Ok(Json(HealthResp {
        data_dir: state.data_dir.clone(),
        persistent: state.persistent,
        db_exists,
    }))
}
