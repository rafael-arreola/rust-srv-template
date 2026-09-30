use axum::{Router, extract::State, routing::get};
use std::sync::Arc;

use super::dto::OrderOutput;
use super::service::OrderService;
use crate::core::context::Context;
use crate::core::error::AppResult;
use crate::core::extractors::PathParam;
use crate::core::response::ApiResponse;

/// Solo lectura: la creación de órdenes no tiene ruta propia y ocurre a través de `checkout`.
pub fn routes(service: Arc<OrderService>) -> Router {
    Router::new().route("/{id}", get(get_order)).with_state(service)
}

#[tracing::instrument(skip_all)]
async fn get_order(
    ctx: Context,
    State(service): State<Arc<OrderService>>,
    PathParam(id): PathParam<String>,
) -> AppResult<ApiResponse<OrderOutput>> {
    let output = service.get_order(&ctx, &id).await?;
    Ok(ApiResponse::ok(output))
}
