use std::future::Future;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_tracing::{DefaultSpanBackend, TracingMiddleware};

use crate::error::{AppError, AppResult};
use crate::registry::Registry;

pub const APP_USER_AGENT: &str = env!("CARGO_PKG_NAME");

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

static HTTP_REGISTRY: Registry<Arc<HttpClient>> = Registry::new("Http");

fn build_client(user_agent: &str, timeout: Duration) -> AppResult<ClientWithMiddleware> {
    let ua = if user_agent.trim().is_empty() { APP_USER_AGENT } else { user_agent.trim() };
    let client = reqwest::Client::builder()
        .user_agent(ua)
        .timeout(timeout)
        .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
        .build()
        .map_err(|e| AppError::internal(format!("Failed to build HTTP client: {e}")))?;

    Ok(ClientBuilder::new(client).with(TracingMiddleware::<DefaultSpanBackend>::new()).build())
}

#[derive(Clone)]
pub struct HttpClient(ClientWithMiddleware);

impl HttpClient {
    pub fn try_new() -> AppResult<Self> {
        Ok(Self(build_client(APP_USER_AGENT, DEFAULT_TIMEOUT)?))
    }

    pub fn with_user_agent(user_agent: &str) -> AppResult<Self> {
        Ok(Self(build_client(user_agent, DEFAULT_TIMEOUT)?))
    }

    pub fn with_timeout(user_agent: &str, timeout: Duration) -> AppResult<Self> {
        Ok(Self(build_client(user_agent, timeout)?))
    }
}

impl Deref for HttpClient {
    type Target = ClientWithMiddleware;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Facade global de clientes HTTP salientes nombrados (`Http::new("mi-srv", "default")?`, `Http::get("default")?`).
pub struct Http;

impl Http {
    pub const DEFAULT: &'static str = "default";

    #[allow(clippy::new_ret_no_self)]
    pub fn new(user_agent: &str, instance: &str) -> AppResult<Arc<HttpClient>> {
        let client = Arc::new(HttpClient::with_user_agent(user_agent)?);
        HTTP_REGISTRY.register(instance, client.clone());
        Ok(client)
    }

    pub fn with_timeout(
        user_agent: &str,
        timeout: Duration,
        instance: &str,
    ) -> AppResult<Arc<HttpClient>> {
        let client = Arc::new(HttpClient::with_timeout(user_agent, timeout)?);
        HTTP_REGISTRY.register(instance, client.clone());
        Ok(client)
    }

    #[track_caller]
    pub fn get(instance: &str) -> AppResult<Arc<HttpClient>> {
        HTTP_REGISTRY.get(instance)
    }

    pub fn try_get(instance: &str) -> Option<Arc<HttpClient>> {
        HTTP_REGISTRY.try_get(instance)
    }

    pub async fn scope<F: Future>(instance: &str, client: Arc<HttpClient>, fut: F) -> F::Output {
        HTTP_REGISTRY.scope(instance, client, fut).await
    }
}
