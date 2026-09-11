use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_tracing::{DefaultSpanBackend, TracingMiddleware};
use std::time::Duration;

use crate::core::error::{AppError, AppResult};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// Builds an instrumented HTTP client with W3C trace context propagation and mandatory timeouts.
///
/// Se construye una sola vez durante el arranque y se comparte vía `AppContext`. Un fallo de
/// construcción se propaga como `AppError`: jamás se degrada a un cliente sin timeouts, porque
/// una petición saliente sin límite puede congelar indefinidamente una tarea asíncrona.
///
/// El backend de spans es `DefaultSpanBackend`, que registra método, host y status sin la URL
/// completa, evitando exportar query strings con tokens o identificadores sensibles.
pub fn instrumented_client() -> AppResult<ClientWithMiddleware> {
    client_with_timeout(DEFAULT_TIMEOUT)
}

pub fn client_with_timeout(timeout: Duration) -> AppResult<ClientWithMiddleware> {
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
        .build()
        .map_err(|e| AppError::internal(format!("Failed to build HTTP client: {e}")))?;

    Ok(ClientBuilder::new(client).with(TracingMiddleware::<DefaultSpanBackend>::new()).build())
}
