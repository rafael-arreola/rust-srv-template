use axum::{extract::State, http::StatusCode, response::IntoResponse};
use mongodb::{Database, bson};

use crate::core::context::Context;
use crate::core::http::shutdown::is_draining;

#[tracing::instrument(skip_all)]
pub async fn healthz(_ctx: Context) -> impl IntoResponse {
    StatusCode::OK
}

#[tracing::instrument(skip_all)]
pub async fn readyz(_ctx: Context, State(db): State<Database>) -> impl IntoResponse {
    if is_draining() {
        return StatusCode::SERVICE_UNAVAILABLE;
    }

    match db.run_command(bson::doc! { "ping": 1 }).await {
        Ok(_) => StatusCode::OK,
        Err(e) => {
            tracing::warn!("Readiness probe failed: {}", e);
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}
