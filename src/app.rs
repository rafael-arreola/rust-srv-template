use std::sync::Arc;

use axum::Router;
use mongodb::Database;

use crate::config;
use crate::core::db::RedisProvider;
use crate::core::error::AppResult;
use crate::core::http::fallback;
use crate::core::net::HttpClient;
use crate::core::server::ServerLauncher;
use crate::modules::{build_api_router, health};

/// Recursos vivos del proceso. La configuración se lee con `config::get()`; nunca se inyecta `&Env`.
#[derive(Clone)]
pub struct AppContext {
    pub db: Database,
    pub redis: Option<RedisProvider>,
    pub http: Arc<HttpClient>,
}

impl AppContext {
    pub fn new(db: Database, redis: Option<RedisProvider>, http: Arc<HttpClient>) -> Self {
        Self { db, redis, http }
    }
}

pub struct Application;

impl Application {
    pub async fn serve(ctx: AppContext) -> AppResult<()> {
        let config = config::get();
        let app = Self::create_router(&ctx).await?;

        ServerLauncher::new(app)
            .with_port(config.port)
            .with_cors_origins(config.cors_origins.clone())
            .with_drain_timeout(config.drain_timeout_secs)
            .with_request_timeout(config.request_timeout_secs)
            .with_msgpack(config.msgpack_enabled)
            .run()
            .await
    }

    pub async fn create_router(ctx: &AppContext) -> AppResult<Router> {
        let health_router = health::router(ctx.db.clone());
        let api_router = build_api_router(ctx).await?;

        // Los fallbacks se aplican al final: `method_not_allowed_fallback` solo alcanza los
        // `MethodRouter` ya registrados, y `nest` los aplana en el router padre al montarlos.
        let app = Router::new()
            .merge(health_router)
            .nest("/api/v1", api_router)
            .fallback(fallback::not_found)
            .method_not_allowed_fallback(fallback::method_not_allowed);

        Ok(app)
    }
}
