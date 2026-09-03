use axum::{
    Router,
    extract::{Path, State},
    routing::{get, post},
};
use std::sync::Arc;

use super::dto::{CreateOrderInput, OrderOutput};
use super::service::OrderService;
use crate::core::error::AppError;
use crate::core::extractors::ValidatedBody;
use crate::core::response::ApiResponse;

pub fn routes(service: Arc<OrderService>) -> Router {
    Router::new().route("/", post(create_order)).route("/{id}", get(get_order)).with_state(service)
}

#[tracing::instrument(skip_all)]
async fn create_order(
    State(service): State<Arc<OrderService>>,
    ValidatedBody(input): ValidatedBody<CreateOrderInput>,
) -> Result<ApiResponse<OrderOutput>, AppError> {
    let output = service.create_order(input).await?;
    Ok(ApiResponse::created(output))
}

#[tracing::instrument(skip_all)]
async fn get_order(
    State(service): State<Arc<OrderService>>,
    Path(id): Path<String>,
) -> Result<ApiResponse<OrderOutput>, AppError> {
    let output = service.get_order(&id).await?;
    Ok(ApiResponse::ok(output))
}
