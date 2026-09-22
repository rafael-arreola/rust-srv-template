use axum::{
    Router,
    extract::State,
    routing::{get, post},
};
use std::sync::Arc;

use super::dto::{CreateUserInput, PaginatedUsers, PaginationQuery, UpdateUserInput, UserOutput};
use super::service::UserService;
use crate::core::context::Context;
use crate::core::error::AppResult;
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
    ctx: Context,
    State(service): State<Arc<UserService>>,
    ValidatedBody(input): ValidatedBody<CreateUserInput>,
) -> AppResult<ApiResponse<UserOutput>> {
    let output = service.create_user(&ctx, input).await?;
    Ok(ApiResponse::created(output))
}

#[tracing::instrument(skip_all)]
async fn get_user(
    ctx: Context,
    State(service): State<Arc<UserService>>,
    PathParam(id): PathParam<String>,
) -> AppResult<ApiResponse<UserOutput>> {
    let output = service.get_user(&ctx, &id).await?;
    Ok(ApiResponse::ok(output))
}

#[tracing::instrument(skip_all)]
async fn list_users(
    ctx: Context,
    State(service): State<Arc<UserService>>,
    ValidatedQuery(query): ValidatedQuery<PaginationQuery>,
) -> AppResult<ApiResponse<PaginatedUsers>> {
    let paginated = service.list_users(&ctx, &query).await?;
    Ok(ApiResponse::ok(paginated))
}

#[tracing::instrument(skip_all)]
async fn update_user(
    ctx: Context,
    State(service): State<Arc<UserService>>,
    PathParam(id): PathParam<String>,
    ValidatedBody(input): ValidatedBody<UpdateUserInput>,
) -> AppResult<ApiResponse<UserOutput>> {
    let output = service.update_user(&ctx, &id, input).await?;
    Ok(ApiResponse::ok(output))
}

#[tracing::instrument(skip_all)]
async fn delete_user(
    ctx: Context,
    State(service): State<Arc<UserService>>,
    PathParam(id): PathParam<String>,
) -> AppResult<ApiResponse<()>> {
    service.delete_user(&ctx, &id).await?;
    Ok(ApiResponse::empty())
}
