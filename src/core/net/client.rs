use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_tracing::{DefaultSpanBackend, TracingMiddleware};
use std::ops::Deref;
use std::time::Duration;

use crate::core::error::{AppError, AppResult};

/// User-Agent canónico con el que el servicio se identifica ante vendors externos.
/// Se deriva de `CARGO_PKG_NAME`: cada servicio se identifica con su propio nombre de paquete.
pub const APP_USER_AGENT: &str = env!("CARGO_PKG_NAME");

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// Construye un cliente HTTP instrumentado con propagación de contexto W3C y timeouts obligatorios.
///
/// Es privada a este archivo y `core::net` declara `client` como módulo privado: la única forma
/// de obtener un cliente es [`HttpClient::try_new`], así que ningún código fuera de `core::net`
/// puede construir un cliente sin estos timeouts.
///
/// Un fallo de construcción se propaga como `AppError`: jamás se degrada a un cliente sin
/// timeouts, porque una petición saliente sin límite puede congelar indefinidamente una tarea
/// asíncrona.
///
/// El backend de spans es `DefaultSpanBackend`, que registra método, host y status sin la URL
/// completa, evitando exportar query strings con tokens o identificadores sensibles.
fn client_with_timeout(timeout: Duration) -> AppResult<ClientWithMiddleware> {
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
/// `Arc<HttpClient>`. Implementa `Deref` hacia [`ClientWithMiddleware`] para que los vendors
/// invoquen `get`, `post`, etc. directamente sobre el cliente.
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
}

impl Deref for HttpClient {
    type Target = ClientWithMiddleware;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
