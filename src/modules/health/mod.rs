//! # health
//!
//! **Intent:** a single liveness probe (`GET /healthz`) for the orchestrator.
//! **Invariants:** answers 200 while the process serves and 503 once the shutdown signal arrived,
//!   so a balancer that still holds open connections stops routing to them during the drain.
//!   It never pings MongoDB: a dependency outage must not make a liveness probe restart the
//!   process, which would only add a cold start to the incident.
//! **Does not:** use the response envelope — orchestrators read only the status code.

mod controller;

use axum::{Router, routing::get};

pub fn router() -> Router {
    Router::new().route("/healthz", get(controller::healthz))
}
