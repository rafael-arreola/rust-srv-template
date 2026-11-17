use std::fmt;

use engine::env;
use engine::{ConfigError, Envs, FromEnv};

/// Catálogo único de variables de entorno del microservicio.
///
/// Se inicializa una vez al arrancar (`Envs::init::<Env>()`) y puede consultarse desde
/// cualquier punto del código con [`get()`] (o `Envs::get::<Env>()`), sin inyectarlo en `Context`.
pub struct Env {
    // Identidad y observabilidad
    pub service_name: String,
    pub service_env: String,
    pub project_id: String,
    pub debug_level: String,

    // Servidor HTTP
    pub port: u16,
    pub cors_origins: String,
    pub drain_timeout_secs: u64,
    pub request_timeout_secs: u64,
    pub body_limit_bytes: usize,
    pub msgpack_enabled: bool,

    // Conexiones primarias
    pub mongo_url: String,
    pub mongo_db: String,

    // Integraciones secundarias (opcionales)
    pub carrier_url: Option<String>,
    /// Secreto: nunca se imprime en `Debug`.
    pub carrier_api_key: Option<String>,
}

impl fmt::Debug for Env {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Env")
            .field("service_name", &self.service_name)
            .field("service_env", &self.service_env)
            .field("port", &self.port)
            .field("drain_timeout_secs", &self.drain_timeout_secs)
            .field("request_timeout_secs", &self.request_timeout_secs)
            .field("body_limit_bytes", &self.body_limit_bytes)
            .finish_non_exhaustive()
    }
}

impl FromEnv for Env {
    fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            service_name: env::required("SERVICE_NAME")?,
            service_env: env::or("SERVICE_ENV", "DEV".to_string())?,
            project_id: env::or("PROJECT_ID", String::new())?,
            debug_level: env::or("DEBUG_LEVEL", "info".to_string())?,

            port: env::or("PORT", 3000)?,
            cors_origins: env::or("CORS_ORIGINS", "*".to_string())?,
            drain_timeout_secs: env::or("DRAIN_TIMEOUT_SECS", 12)?,
            request_timeout_secs: env::or("REQUEST_TIMEOUT_SECS", 300)?,
            body_limit_bytes: env::or("BODY_LIMIT_BYTES", 32 * 1024 * 1024)?,
            msgpack_enabled: env::or("ENABLE_MSGPACK", true)?,

            mongo_url: env::required("MONGO_URL")?,
            mongo_db: env::required("MONGO_DB")?,
            carrier_url: env::optional("CARRIER_URL")?,
            carrier_api_key: env::optional("CARRIER_API_KEY")?,
        })
    }
}

/// Atajo global directo hacia la configuración tipada del servicio (`&'static Env`).
#[inline]
pub fn get() -> &'static Env {
    Envs::get::<Env>()
}
