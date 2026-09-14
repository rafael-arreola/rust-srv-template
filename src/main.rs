pub mod app;
pub mod config;
pub mod core;
pub mod modules;
pub mod shared;
pub mod vendors;

use std::process::ExitCode;
use std::sync::Arc;

use app::{AppContext, Application};
use config::Env;
use core::{AppError, AppResult, HttpClient, MongoProvider, RedisProvider};

#[tokio::main]
async fn main() -> ExitCode {
    // El proveedor criptográfico debe instalarse antes de construir cualquier cliente TLS.
    if let Err(e) = rustls::crypto::ring::default_provider().install_default() {
        eprintln!("Failed to install rustls crypto provider: {e:?}");
        return ExitCode::FAILURE;
    }

    let env = config::get();

    let tracer_guard = match core::tracer::init_tracing().await {
        Ok(guard) => Some(guard),
        Err(e) => {
            eprintln!("Failed to initialize tracing: {e}");
            None
        }
    };

    // Un arranque fallido debe terminar con código distinto de cero para que el orquestador
    // detecte el crash en lugar de interpretarlo como salida limpia.
    let exit_code = match run(env).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let mut fatal = e.attach_context("fatal error during service startup");
            fatal.log_now();
            ExitCode::FAILURE
        }
    };

    if let Some(guard) = tracer_guard {
        guard.shutdown();
    }

    exit_code
}

async fn run(env: &'static Env) -> AppResult<()> {
    tracing::info!("Starting {} (env: {})", env.service_name, env.service_env);

    let mongo = MongoProvider::new(&env.service_name, &env.mongo_url, &env.mongo_db).await?;

    // Redis es opcional en configuración; si se declara, un fallo de conexión es fatal.
    let redis = match &env.redis_url {
        Some(url) => Some(
            RedisProvider::new(url, &env.redis_prefix)
                .await
                .map_err(|e| AppError::database(format!("Failed to connect to Redis: {e}")))?,
        ),
        None => None,
    };

    let http = Arc::new(HttpClient::try_new()?);
    let ctx = AppContext::new(mongo.database(), redis, http);

    Application::serve(ctx).await
}
