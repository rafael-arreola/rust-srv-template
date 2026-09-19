use axum::{
    Router,
    extract::State,
    routing::{get, post},
};
use std::sync::Arc;

use super::dto::{CreateOrderInput, OrderOutput};
use super::service::OrderService;
use crate::core::context::Context;
use crate::core::error::AppError;
use crate::core::extractors::{PathParam, ValidatedBody};
use crate::core::response::ApiResponse;

pub fn routes(service: Arc<OrderService>) -> Router {
    Router::new().route("/", post(create_order)).route("/{id}", get(get_order)).with_state(service)
}

#[tracing::instrument(skip_all)]
async fn create_order(
    ctx: Context,
    State(service): State<Arc<OrderService>>,
    ValidatedBody(input): ValidatedBody<CreateOrderInput>,
) -> Result<ApiResponse<OrderOutput>, AppError> {
    let output = service.create_order(&ctx, input).await?;
    Ok(ApiResponse::created(output))
}

#[tracing::instrument(skip_all)]
async fn get_order(
    ctx: Context,
    State(service): State<Arc<OrderService>>,
    PathParam(id): PathParam<String>,
) -> Result<ApiResponse<OrderOutput>, AppError> {
    let output = service.get_order(&ctx, &id).await?;
    Ok(ApiResponse::ok(output))
}
