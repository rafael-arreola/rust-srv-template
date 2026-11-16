use engine::web::{
    PaginationQuery, PathParam, QueryResult, ReplyExt, Router, State, Status, ValidatedBody,
    ValidatedQuery, get, post,
};
use engine::{AppResult, Context};
use serde::Deserialize;
use validator::Validate;

use crate::bootstrap::AppContext;
use crate::models::{User, UserId};
use crate::services::UserService;

// ── FORM REQUESTS & DIRECT ENTITY RESPONSE ────────────────────────────────

#[derive(Debug, Deserialize, Validate)]
pub struct CreateUserRequest {
    #[validate(length(min = 2, max = 100, message = "Name must be between 2 and 100 characters"))]
    pub name: String,

    #[validate(email(message = "Invalid email format"))]
    pub email: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateUserRequest {
    #[validate(length(min = 2, max = 100, message = "Name must be between 2 and 100 characters"))]
    pub name: String,

    #[validate(email(message = "Invalid email format"))]
    pub email: String,

    pub version: u32,
}

engine::respond!(User);

// ── CONTROLADOR HTTP (Solo invoca UserService inyectado vía State) ────────

pub fn routes() -> Router<AppContext> {
    Router::new()
        .route("/", post(create_user).get(list_users))
        .route("/{id}", get(get_user).put(update_user).delete(delete_user))
}

#[tracing::instrument(skip_all)]
async fn create_user(
    ctx: Context,
    State(users): State<UserService>,
    ValidatedBody(req): ValidatedBody<CreateUserRequest>,
) -> AppResult<Status<User>> {
    users.create(&ctx, req.name, req.email).await.created()
}

#[tracing::instrument(skip_all)]
async fn get_user(
    ctx: Context,
    State(users): State<UserService>,
    PathParam(id): PathParam<UserId>,
) -> AppResult<User> {
    users.find_active(&ctx, &id).await
}

#[tracing::instrument(skip_all)]
async fn list_users(
    ctx: Context,
    State(users): State<UserService>,
    ValidatedQuery(query): ValidatedQuery<PaginationQuery>,
) -> AppResult<QueryResult<User>> {
    users.list(&ctx, None, query.skip(), query.limit()).await
}

#[tracing::instrument(skip_all)]
async fn update_user(
    ctx: Context,
    State(users): State<UserService>,
    PathParam(id): PathParam<UserId>,
    ValidatedBody(req): ValidatedBody<UpdateUserRequest>,
) -> AppResult<User> {
    users.update(&ctx, &id, req.name, req.email, req.version).await
}

#[tracing::instrument(skip_all)]
async fn delete_user(
    ctx: Context,
    State(users): State<UserService>,
    PathParam(id): PathParam<UserId>,
) -> AppResult<Status> {
    users.delete(&ctx, &id).await.no_content()
}
