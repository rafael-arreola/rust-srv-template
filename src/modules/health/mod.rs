pub mod controller;

use axum::{Router, routing::get};
use mongodb::Database;

pub use controller::{is_draining, start_draining};

pub fn router(db: Database) -> Router {
    Router::new()
        .route("/healthz", get(controller::healthz))
        .route("/readyz", get(controller::readyz))
        .with_state(db)
}
