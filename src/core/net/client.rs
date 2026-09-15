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
/// Visibilidad `pub(crate)` deliberada: la fábrica pertenece al composition root y no es alcanzable
/// desde `vendors` ni desde `modules`, de modo que el compilador —y no la directiva— impide crear
/// clientes ad hoc. Los vendors reciben el `Arc<HttpClient>` ya construido.
///
/// Un fallo de construcción se propaga como `AppError`: jamás se degrada a un cliente sin
/// timeouts, porque una petición saliente sin límite puede congelar indefinidamente una tarea
/// asíncrona.
///
/// El backend de spans es `DefaultSpanBackend`, que registra método, host y status sin la URL
/// completa, evitando exportar query strings con tokens o identificadores sensibles.
pub(crate) fn client_with_timeout(timeout: Duration) -> AppResult<ClientWithMiddleware> {
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
///
/// No existe conversión desde un `ClientWithMiddleware` arbitrario: envolver un cliente externo
/// permitiría inyectar uno sin timeouts y anular la garantía de esta frontera.
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
