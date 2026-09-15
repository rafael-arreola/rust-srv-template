//! # health
//!
//! **Intent:** liveness (`/healthz`) and readiness (`/readyz`) probes for the orchestrator.
//! **Invariants:** `/readyz` answers 503 while draining or when MongoDB does not ping, so the
//!   balancer stops routing before in-flight connections close.
//! **Does not:** use the response envelope — probes answer bare status codes by design.

mod controller;

use axum::{Router, routing::get};
use mongodb::Database;

pub fn router(db: Database) -> Router {
    Router::new()
        .route("/healthz", get(controller::healthz))
        .route("/readyz", get(controller::readyz))
        .with_state(db)
}
