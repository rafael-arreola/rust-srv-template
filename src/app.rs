use std::sync::Arc;

use axum::Router;

use crate::config;
use crate::core::db::{RedisProvider, Store};
use crate::core::error::AppResult;
use crate::core::http::fallback;
use crate::core::net::HttpClient;
use crate::core::server::ServerLauncher;
use crate::modules::{Modules, health};

/// Recursos vivos del proceso. La configuración vive en un `OnceLock` estático (`config::get()`),
/// por eso no viaja dentro del contexto.
///
/// `db` es el handle neutral de persistencia: ningún módulo recibe el driver, solo su
/// `repository.rs` baja a él a través de `Store`.
#[derive(Clone)]
pub struct AppContext {
    pub db: Store,
    pub redis: Option<RedisProvider>,
    pub http: Arc<HttpClient>,
}

impl AppContext {
    pub fn new(db: Store, redis: Option<RedisProvider>, http: Arc<HttpClient>) -> Self {
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
        let api_router = Modules::init(ctx).await?.api_v1_router();

        // Los fallbacks se aplican al final: `method_not_allowed_fallback` solo alcanza los
        // `MethodRouter` ya registrados, y `nest` los aplana en el router padre al montarlos.
        let app = Router::new()
            .merge(health::router())
            .nest("/api/v1", api_router)
            .fallback(fallback::not_found)
            .method_not_allowed_fallback(fallback::method_not_allowed);

        Ok(app)
    }
}
