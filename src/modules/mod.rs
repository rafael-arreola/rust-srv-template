pub mod health;
pub mod orders;
pub mod users;

use self::orders::OrdersModule;
use self::users::UsersModule;
use crate::app::AppContext;
use crate::core::error::AppResult;
use axum::Router;

pub async fn build_api_router(ctx: &AppContext) -> AppResult<Router> {
    // 1. Instantiate modules (resolving dependency graph)
    let users_module = UsersModule::new(&ctx.db).await?;
    let orders_module = OrdersModule::new(&ctx.db, users_module.service.clone()).await?;

    // 2. Mount routes into /api/v1
    let api =
        Router::new().nest("/users", users_module.router()).nest("/orders", orders_module.router());

    Ok(api)
}
