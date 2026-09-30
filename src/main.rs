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
use core::{AppError, AppResult, HttpClient, MongoProvider, RedisProvider, Store};

#[tokio::main]
async fn main() -> ExitCode {
    // Orden de arranque: configuración → proveedor criptográfico → telemetría →
    // recursos vivos. Cualquier fallo de la secuencia termina con código distinto de cero.
    let env = config::get();

    // El proveedor criptográfico debe instalarse antes de construir cualquier cliente TLS.
    if let Err(e) = rustls::crypto::ring::default_provider().install_default() {
        eprintln!("Failed to install rustls crypto provider: {e:?}");
        return ExitCode::FAILURE;
    }

    // Sin subscriber no existe ningún canal de log: el fallo va a stderr, igual que los de
    // configuración y del proveedor criptográfico, porque `log_now()` no tendría a dónde emitir.
    let tracer_guard = match core::tracer::init_tracing().await {
        Ok(guard) => guard,
        Err(e) => {
            eprintln!("Failed to initialize tracing: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Un arranque fallido debe terminar con código distinto de cero para que el orquestador
    // detecte el crash en lugar de interpretarlo como salida limpia.
    let exit_code = match run(env).await {
        Ok(()) => ExitCode::SUCCESS,
        // No se usa `log_now()`: emite "HTTP request error" con `http.status_code`, y cada
        // reinicio de un crash-loop contaría como un 5xx falso en las métricas de SLO.
        Err(e) => {
            tracing::error!(
                error.code = %e.code(),
                error.breadcrumbs = %crate::as_json!(&e.full_trace()),
                "Service terminated with a fatal error"
            );
            ExitCode::FAILURE
        }
    };

    tracer_guard.shutdown();

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
    let ctx = AppContext::new(Store::from_mongo(mongo.database()), redis, http);

    Application::serve(ctx).await
}
