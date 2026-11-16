use std::fmt;
use std::future::Future;

use redis::{AsyncCommands, aio::MultiplexedConnection};
use serde::{Serialize, de::DeserializeOwned};

use crate::error::{AppError, AppResult};
use crate::registry::Registry;

static REDIS_REGISTRY: Registry<RedisProvider> = Registry::new("Redis");

/// Facade global de instancias Redis nombradas (`Redis::new(url, prefix, "default").await?`, `Redis::get("default")?`).
pub struct Redis;

impl Redis {
    pub const DEFAULT: &'static str = "default";

    #[allow(clippy::new_ret_no_self)]
    pub async fn new(url: &str, prefix: &str, instance: &str) -> AppResult<RedisProvider> {
        let provider = RedisProvider::new(url, prefix).await.map_err(|e| {
            AppError::database(format!("Failed to connect Redis '{instance}': {e}"))
        })?;
        REDIS_REGISTRY.register(instance, provider.clone());
        Ok(provider)
    }

    #[track_caller]
    pub fn get(instance: &str) -> AppResult<RedisProvider> {
        REDIS_REGISTRY.get(instance)
    }

    pub fn try_get(instance: &str) -> Option<RedisProvider> {
        REDIS_REGISTRY.try_get(instance)
    }

    /// Cache-aside tolerante: si `instance` no fue registrada (ej. local sin Redis), ejecuta `loader` directo.
    pub async fn remember<T, F, Fut>(
        instance: &str,
        parts: &[&str],
        ttl_seconds: u64,
        loader: F,
    ) -> AppResult<T>
    where
        T: Serialize + DeserializeOwned,
        F: FnOnce() -> Fut,
        Fut: Future<Output = AppResult<T>>,
    {
        match Self::try_get(instance) {
            Some(provider) => provider.remember(parts, ttl_seconds, loader).await,
            None => loader().await,
        }
    }

    /// Invalida una llave de caché si `instance` está registrada (no-op seguro en local sin Redis).
    pub async fn invalidate(instance: &str, parts: &[&str]) -> AppResult<()> {
        match Self::try_get(instance) {
            Some(provider) => provider.del_parts(parts).await,
            None => Ok(()),
        }
    }

    pub async fn scope<F: Future>(instance: &str, provider: RedisProvider, fut: F) -> F::Output {
        REDIS_REGISTRY.scope(instance, provider, fut).await
    }
}

/// Conexión Redis multiplexada con prefijo de servicio; expone `.conn()` para comandos nativos
/// de `redis::AsyncCommands` y helpers tipados `get_json` / `set_json` / `remember`.
#[derive(Clone)]
pub struct RedisProvider {
    connection: MultiplexedConnection,
    prefix: String,
}

impl fmt::Debug for RedisProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

    #[inline]
    pub fn conn(&self) -> MultiplexedConnection {
        self.connection.clone()
    }

    #[inline]
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    pub fn key(&self, parts: &[&str]) -> String {
        let joined = parts.join(":");
        if self.prefix.is_empty() { joined } else { format!("{}:{joined}", self.prefix) }
    }

    pub async fn del_parts(&self, parts: &[&str]) -> AppResult<()> {
        let key = self.key(parts);
        let mut conn = self.conn();
        let _: () = conn
            .del(&key)
            .await
            .map_err(|e| AppError::database(format!("redis DEL {key}: {e}")))?;
        Ok(())
    }

    pub async fn get_json<T: DeserializeOwned>(&self, parts: &[&str]) -> AppResult<Option<T>> {
        let key = self.key(parts);
        let mut conn = self.conn();
        let raw: Option<String> = conn
            .get(&key)
            .await
            .map_err(|e| AppError::database(format!("redis GET {key}: {e}")))?;
        raw.map(|s| {
            serde_json::from_str(&s)
                .map_err(|e| AppError::database(format!("redis JSON decode error on {key}: {e}")))
        })
        .transpose()
    }

    pub async fn set_json<T: Serialize>(
        &self,
        parts: &[&str],
        value: &T,
        ttl_seconds: u64,
    ) -> AppResult<()> {
        let key = self.key(parts);
        let raw = serde_json::to_string(value)
            .map_err(|e| AppError::database(format!("redis JSON encode error on {key}: {e}")))?;
        let mut conn = self.conn();
        let _: () = conn
            .set_ex(&key, raw, ttl_seconds)
            .await
            .map_err(|e| AppError::database(format!("redis SETEX {key}: {e}")))?;
        Ok(())
    }

    pub async fn remember<T, F, Fut>(
        &self,
        parts: &[&str],
        ttl_seconds: u64,
        loader: F,
    ) -> AppResult<T>
    where
        T: Serialize + DeserializeOwned,
        F: FnOnce() -> Fut,
        Fut: Future<Output = AppResult<T>>,
    {
        if let Ok(Some(cached)) = self.get_json::<T>(parts).await {
            return Ok(cached);
        }
        let fresh = loader().await?;
        if let Err(err) = self.set_json(parts, &fresh, ttl_seconds).await {
            tracing::warn!(error = %err.internal_detail(), "Failed to populate Redis cache; returning fresh value");
        }
        Ok(fresh)
    }
}
