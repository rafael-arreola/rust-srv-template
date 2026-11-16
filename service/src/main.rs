pub mod bootstrap;
pub mod clients;
pub mod envs;
pub mod http;
pub mod models;
pub mod routes;
pub mod services;
pub mod stores;

use std::process::ExitCode;

use engine::{AppResult, Envs, Http, Mongo, Redis, ServerLauncher, Telemetry};
use envs::Env;

#[tokio::main]
async fn main() -> ExitCode {
    engine::boot(run()).await
}

async fn run() -> AppResult<()> {
    let env = Envs::init::<Env>()?;
    let _telemetry = Telemetry::new(&env.service_name)
        .env(&env.service_env)
        .project_id(&env.project_id)
        .level(&env.debug_level)
        .init()
        .await?;

    tracing::info!("Starting {} (env: {})", env.service_name, env.service_env);

    Mongo::new(&env.mongo_url, &env.mongo_db, Mongo::DEFAULT).await?;
    // if let Some(redis_url) = &env.redis_url {
    //     Redis::new(redis_url, &env.redis_prefix, Redis::DEFAULT).await?;
    // }
    Http::new(&env.service_name, Http::DEFAULT)?;

    let state = bootstrap::AppContext::init().await?;

    ServerLauncher::new(routes::build(state))
        .with_port(env.port)
        .with_cors_origins(&env.cors_origins)
        .with_drain_timeout(env.drain_timeout_secs)
        .with_request_timeout(env.request_timeout_secs)
        .with_body_limit(env.body_limit_bytes)
        .with_msgpack(env.msgpack_enabled)
        .run()
        .await
}
