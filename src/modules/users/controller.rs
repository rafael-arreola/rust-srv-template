use axum::{
    Router,
    extract::State,
    routing::{get, post},
};
use std::sync::Arc;

use super::dto::{CreateUserInput, PaginatedUsers, PaginationQuery, UpdateUserInput, UserOutput};
use super::service::UserService;
use crate::core::error::AppError;
use crate::core::extractors::{PathParam, ValidatedBody, ValidatedQuery};
use crate::core::response::ApiResponse;

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
    PathParam(id): PathParam<String>,
) -> Result<ApiResponse<UserOutput>, AppError> {
    let output = service.get_user(&id).await?;
    Ok(ApiResponse::ok(output))
}

#[tracing::instrument(skip_all)]
async fn list_users(
    State(service): State<Arc<UserService>>,
    ValidatedQuery(query): ValidatedQuery<PaginationQuery>,
) -> Result<ApiResponse<PaginatedUsers>, AppError> {
    let paginated = service.list_users(&query).await?;
    Ok(ApiResponse::ok(paginated))
}

#[tracing::instrument(skip_all)]
async fn update_user(
    State(service): State<Arc<UserService>>,
    PathParam(id): PathParam<String>,
    ValidatedBody(input): ValidatedBody<UpdateUserInput>,
) -> Result<ApiResponse<UserOutput>, AppError> {
    let output = service.update_user(&id, input).await?;
    Ok(ApiResponse::ok(output))
}

#[tracing::instrument(skip_all)]
async fn delete_user(
    State(service): State<Arc<UserService>>,
    PathParam(id): PathParam<String>,
) -> Result<ApiResponse<()>, AppError> {
    service.delete_user(&id).await?;
    Ok(ApiResponse::empty())
}
