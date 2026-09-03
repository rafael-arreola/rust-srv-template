use axum::{
    Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use std::sync::Arc;

use super::dto::{CreateUserInput, PaginationQuery, UpdateUserInput, UserOutput};
use super::service::UserService;
use crate::core::error::AppError;
use crate::core::extractors::ValidatedBody;
use crate::core::response::{ApiResponse, PaginatedResponse};

pub fn routes(service: Arc<UserService>) -> Router {
    Router::new()
        .route("/", post(create_user).get(list_users))
        .route("/{id}", get(get_user).put(update_user).delete(delete_user))
        .with_state(service)
}

#[tracing::instrument(skip_all)]
async fn create_user(
    State(service): State<Arc<UserService>>,
    ValidatedBody(input): ValidatedBody<CreateUserInput>,
) -> Result<ApiResponse<UserOutput>, AppError> {
    let output = service.create_user(input).await?;
    Ok(ApiResponse::created(output))
}

#[tracing::instrument(skip_all)]
async fn get_user(
    State(service): State<Arc<UserService>>,
    Path(id): Path<String>,
) -> Result<ApiResponse<UserOutput>, AppError> {
    let output = service.get_user(&id).await?;
    Ok(ApiResponse::ok(output))
}

#[tracing::instrument(skip_all)]
async fn list_users(
    State(service): State<Arc<UserService>>,
    Query(query): Query<PaginationQuery>,
) -> Result<ApiResponse<PaginatedResponse<UserOutput>>, AppError> {
    let page = query.page();
    let limit = query.limit();

    let (users, total) = service.list_users(page, limit).await?;
    Ok(ApiResponse::paginated(users, total, page, limit))
}

#[tracing::instrument(skip_all)]
async fn update_user(
    State(service): State<Arc<UserService>>,
    Path(id): Path<String>,
    ValidatedBody(input): ValidatedBody<UpdateUserInput>,
) -> Result<ApiResponse<UserOutput>, AppError> {
    let output = service.update_user(&id, input).await?;
    Ok(ApiResponse::ok(output))
}

#[tracing::instrument(skip_all)]
async fn delete_user(
    State(service): State<Arc<UserService>>,
    Path(id): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    service.delete_user(&id).await?;
    Ok(ApiResponse::empty())
}
