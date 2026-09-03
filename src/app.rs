use axum::Router;
use mongodb::Database;
use reqwest_middleware::ClientWithMiddleware;

use crate::config::Env;
use crate::core::error::AppResult;
use crate::core::http_client::instrumented_client;
use crate::core::server::ServerLauncher;
use crate::modules::{build_api_router, health};

#[derive(Clone)]
pub struct AppContext {
    pub db: Database,
    pub config: &'static Env,
    pub http_client: ClientWithMiddleware,
}

impl AppContext {
    pub fn new(db: Database, config: &'static Env) -> Self {
        Self { db, config, http_client: instrumented_client() }
    }
}

pub struct Application;

impl Application {
    pub async fn create_router(ctx: &AppContext) -> AppResult<Router> {
        let health_router = health::router(ctx.db.clone());
        let api_router = build_api_router(ctx).await?;

        let app = Router::new().merge(health_router).nest("/api/v1", api_router);

        Ok(app)
    }

    pub async fn serve(ctx: AppContext) -> AppResult<()> {
        let config = ctx.config;
        let app = Self::create_router(&ctx).await?;

        ServerLauncher::new(app)
            .with_port(config.port)
            .with_cors_origins(config.cors_origins.clone())
            .with_drain_timeout(config.drain_timeout_secs)
            .with_request_timeout(config.request_timeout_secs)
            .with_msgpack(config.msgpack_enabled)
            .run()
            .await;

        Ok(())
    }
}
