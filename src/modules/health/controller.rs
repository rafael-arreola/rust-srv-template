use axum::{extract::State, http::StatusCode, response::IntoResponse};
use mongodb::Database;
use std::sync::atomic::{AtomicBool, Ordering};

static DRAINING: AtomicBool = AtomicBool::new(false);

pub fn start_draining() {
    DRAINING.store(true, Ordering::SeqCst);
}

pub fn is_draining() -> bool {
    DRAINING.load(Ordering::SeqCst)
}

#[tracing::instrument(skip_all)]
pub async fn healthz() -> impl IntoResponse {
    StatusCode::OK
}

#[tracing::instrument(skip_all)]
pub async fn readyz(State(db): State<Database>) -> impl IntoResponse {
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
