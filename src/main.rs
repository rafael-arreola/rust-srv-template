pub mod app;
pub mod config;
pub mod core;
pub mod modules;

use app::{AppContext, Application};
use core::MongoProvider;

#[tokio::main]
async fn main() {
    if let Err(e) = rustls::crypto::ring::default_provider().install_default() {
        eprintln!("Failed to install rustls crypto provider: {:?}", e);
        return;
    }

    let env = config::get();

    let tracer_guard = match core::tracer::init_tracing().await {
        Ok(guard) => Some(guard),
        Err(e) => {
            eprintln!("Failed to initialize tracing: {}", e);
            None
        }
    };

    tracing::info!("Starting {} (env: {})", env.service_name, env.service_env);

    let mongo = match MongoProvider::new(&env.service_name, &env.mongo_url, &env.mongo_db).await {
        Ok(mongo) => mongo,
        Err(e) => {
            tracing::error!("Failed to connect to MongoDB: {}", e);
            return;
        }
    };

    let ctx = AppContext::new(mongo.database(), env);

    if let Err(e) = Application::serve(ctx).await {
        tracing::error!("Server error: {}", e);
    }

    if let Some(guard) = tracer_guard {
        guard.shutdown();
    }
}
