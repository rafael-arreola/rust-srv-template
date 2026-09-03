use crate::core::error::{AppError, AppResult};
use mongodb::{Client, Database, options::ClientOptions};

pub mod helpers;

#[derive(Clone)]
pub struct MongoProvider {
    db: Database,
}

impl MongoProvider {
    pub async fn new(app_name: &str, mongo_url: &str, mongo_db: &str) -> AppResult<Self> {
        let mut client_options = ClientOptions::parse(mongo_url)
            .await
            .map_err(|e| AppError::database(format!("Failed to parse MongoDB URI: {}", e)))?;

        client_options.app_name = Some(app_name.to_string());

        // Emit OpenTelemetry spans per command/operation parented to the active trace
        client_options.tracing =
            Some(mongodb::otel::OpentelemetryOptions::builder().enabled(true).build());

        let client = Client::with_options(client_options).map_err(|e| {
            AppError::database(format!("Failed to initialize MongoDB client: {}", e))
        })?;

        let db = client.database(mongo_db);

        db.run_command(bson::doc! {"ping": 1})
            .await
            .map_err(|e| AppError::database(format!("Failed to ping MongoDB: {}", e)))?;

        tracing::info!("Connected to MongoDB database: {}", mongo_db);

        Ok(Self { db })
    }

    pub fn database(&self) -> Database {
        self.db.clone()
    }
}
