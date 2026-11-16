#[macro_use]
pub mod runtime;
pub mod stores;
pub mod types;
#[macro_use]
pub mod web;

pub use axum;
#[cfg(feature = "multipart")]
pub use axum_typed_multipart;
pub use chrono;

#[doc(hidden)]
pub use serde_json as __serde_json;

// Familias accesibles también como submódulos cortos (`engine::db`, `engine::env`, `engine::cast`...).
pub use runtime::{env, registry, telemetry};
pub use stores::{db, net};
pub use types::{cast, context, error, time};
pub use web::{auth, extractors, response};

// Re-exports directos de uso cotidiano en `use engine::{...}`.
pub use axum::http::StatusCode;
pub use runtime::{ConfigError, Envs, FromEnv, Registry, Telemetry, TracerGuard, init_tracing};
pub use stores::{Http, HttpClient, QueryResult};
#[cfg(feature = "mongo")]
pub use stores::{Mongo, MongoDb};
#[cfg(feature = "redis")]
pub use stores::{Redis, RedisProvider};
#[cfg(feature = "uuid")]
pub use types::Uuid;
pub use types::{
    Actor, AppError, AppResult, Cast, Context, DelegatedCredential, TraceExt, UtcDateTime,
};
#[cfg(feature = "mongo")]
pub use types::{FlexibleId, ObjectId};
pub use web::{Auth, ServerLauncher};

/// Preludio ergonómico opcional (`use engine::prelude::*;`) con los tipos y extensiones más usados.
pub mod prelude {
    #[cfg(feature = "uuid")]
    pub use crate::Uuid;
    pub use crate::web::{HeaderContextExt, ReplyExt, RequirePermissionExt, RouterAuthExt};
    pub use crate::{
        Actor, AppError, AppResult, Auth, Cast, Context, DelegatedCredential, Envs, FromEnv, Http,
        QueryResult, ServerLauncher, StatusCode, Telemetry, TraceExt, UtcDateTime,
    };
    #[cfg(feature = "mongo")]
    pub use crate::{FlexibleId, Mongo, MongoDb, ObjectId};
    #[cfg(feature = "redis")]
    pub use crate::{Redis, RedisProvider};
}

/// Instala de forma idempotente el proveedor criptográfico `ring` de `rustls`.
pub fn install_crypto_provider() -> Result<(), String> {
    if rustls::crypto::CryptoProvider::get_default().is_some() {
        return Ok(());
    }
    rustls::crypto::ring::default_provider().install_default().map_err(|e| format!("{e:?}"))
}

/// Ejecuta la rutina principal de un microservicio instalando TLS (`ring`) y gestionando la salida
/// y el volcado estructurado de errores fatales.
pub async fn boot<F>(fut: F) -> std::process::ExitCode
where
    F: std::future::Future<Output = AppResult<()>>,
{
    if let Err(e) = install_crypto_provider() {
        eprintln!("Failed to install rustls crypto provider: {e}");
        return std::process::ExitCode::FAILURE;
    }

    match fut.await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            if tracing::dispatcher::has_been_set() {
                tracing::error!(
                    error.code = %err.code(),
                    error.breadcrumbs = %crate::as_json!(&err.full_trace()),
                    "Service terminated with a fatal error"
                );
            } else {
                eprintln!("Startup error: {}", err.internal_detail());
            }
            std::process::ExitCode::FAILURE
        }
    }
}
