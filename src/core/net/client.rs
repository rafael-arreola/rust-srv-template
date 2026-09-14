use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_tracing::{DefaultSpanBackend, TracingMiddleware};
use std::ops::{Deref, DerefMut};
use std::time::Duration;

use crate::core::error::{AppError, AppResult};

/// User-Agent canónico con el que el servicio se identifica ante vendors externos.
pub const APP_USER_AGENT: &str = "rust-srv-template";

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// Construye un cliente HTTP instrumentado con propagación de contexto W3C y timeouts obligatorios.
///
/// Debe invocarse **una sola vez** en el composition root (`AppContext::new`) y el resultado
/// inyectarse en los vendors como `Arc<HttpClient>`. Los vendors **no** deben llamar a esta
/// función: construyen su adaptador con el cliente ya provisto.
///
/// Un fallo de construcción se propaga como `AppError`: jamás se degrada a un cliente sin
/// timeouts, porque una petición saliente sin límite puede congelar indefinidamente una tarea
/// asíncrona.
///
/// El backend de spans es `DefaultSpanBackend`, que registra método, host y status sin la URL
/// completa, evitando exportar query strings con tokens o identificadores sensibles.
pub fn instrumented_client() -> AppResult<ClientWithMiddleware> {
    client_with_timeout(DEFAULT_TIMEOUT)
}

/// Construye un cliente HTTP instrumentado con un timeout global explícito.
///
/// Al igual que [`instrumented_client`], se construye una sola vez en el composition root y se
/// inyecta; no debe usarse como fábrica ad hoc dentro de un vendor.
pub fn client_with_timeout(timeout: Duration) -> AppResult<ClientWithMiddleware> {
    let client = reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .timeout(timeout)
        .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
        .build()
        .map_err(|e| AppError::internal(format!("Failed to build HTTP client: {e}")))?;

    Ok(ClientBuilder::new(client).with(TracingMiddleware::<DefaultSpanBackend>::new()).build())
}

/// Newtype inyectable que encapsula el cliente HTTP instrumentado.
///
/// Se construye en el composition root con [`HttpClient::try_new`] y se comparte clonando el
/// `Arc<HttpClient>`. Implementa `Deref`/`DerefMut` hacia [`ClientWithMiddleware`] para que los
/// vendors invoquen `get`, `post`, etc. directamente sobre el cliente.
#[derive(Clone)]
pub struct HttpClient(ClientWithMiddleware);

impl HttpClient {
    /// Construye el cliente con los timeouts por defecto. Falla si el builder de `reqwest`
    /// no puede inicializarse.
    pub fn try_new() -> AppResult<Self> {
        Ok(Self(client_with_timeout(DEFAULT_TIMEOUT)?))
    }

    /// Acceso explícito al cliente subyacente cuando se requiere por firma.
    pub fn inner(&self) -> &ClientWithMiddleware {
        &self.0
    }
}

impl Deref for HttpClient {
    type Target = ClientWithMiddleware;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for HttpClient {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<ClientWithMiddleware> for HttpClient {
    fn from(client: ClientWithMiddleware) -> Self {
        Self(client)
    }
}
