use redis::AsyncCommands;
use redis::aio::MultiplexedConnection;

use crate::core::error::{AppError, AppResult};

#[derive(Clone)]
pub struct RedisProvider {
    connection: MultiplexedConnection,
    prefix: String,
}

impl std::fmt::Debug for RedisProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisProvider").field("prefix", &self.prefix).finish()
    }
}

impl RedisProvider {
    pub async fn new(url: &str, prefix: &str) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(url)?;
        let mut conn = client.get_multiplexed_async_connection().await?;

        redis::cmd("PING").query_async::<()>(&mut conn).await?;

        tracing::info!("Connected to Redis: {}", prefix);

        Ok(RedisProvider { connection: conn, prefix: prefix.to_string() })
    }

    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    pub fn key(&self, parts: &[&str]) -> String {
        let joined = parts.join(":");
        if self.prefix.is_empty() { joined } else { format!("{}:{}", self.prefix, joined) }
    }

    /// Obtiene el valor asociado a una clave. Retorna `None` si la clave no existe.
    ///
    /// Un fallo de Redis es un error de infraestructura de datos (500 con mensaje público
    /// genérico); el nombre del servicio y la clave viajan solo en el detalle interno.
    pub async fn get(&self, key: &str) -> AppResult<Option<String>> {
        let mut conn = self.connection.clone();
        let value: Option<String> =
            conn.get(key).await.map_err(|e| AppError::database(format!("redis GET {key}: {e}")))?;
        Ok(value)
    }

    /// Establece una clave con expiración en segundos.
    pub async fn set_ex(&self, key: &str, value: &str, ttl_seconds: u64) -> AppResult<()> {
        let mut conn = self.connection.clone();
        let _: () = conn
            .set_ex(key, value, ttl_seconds)
            .await
            .map_err(|e| AppError::database(format!("redis SETEX {key}: {e}")))?;
        Ok(())
    }

    pub async fn del(&self, key: &str) -> AppResult<()> {
        let mut conn = self.connection.clone();
        let _: () =
            conn.del(key).await.map_err(|e| AppError::database(format!("redis DEL {key}: {e}")))?;
        Ok(())
    }
}
