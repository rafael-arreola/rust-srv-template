use axum::{http::StatusCode, response::IntoResponse};

use crate::core::context::Context;
use crate::core::http::shutdown::is_draining;

#[tracing::instrument(skip_all)]
pub async fn healthz(_ctx: Context) -> impl IntoResponse {
    if is_draining() { StatusCode::SERVICE_UNAVAILABLE } else { StatusCode::OK }
}
