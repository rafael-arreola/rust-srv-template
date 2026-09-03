use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_tracing::{SpanBackendWithUrl, TracingMiddleware};
use std::time::Duration;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// Builds an instrumented HTTP client with W3C trace context propagation and mandatory timeouts.
pub fn instrumented_client() -> ClientWithMiddleware {
    client_with_timeout(DEFAULT_TIMEOUT)
}

pub fn client_with_timeout(timeout: Duration) -> ClientWithMiddleware {
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
        .build()
        .unwrap_or_else(|e| {
            tracing::error!("Failed to build HTTP client with timeouts: {e}");
            reqwest::Client::new()
        });

    ClientBuilder::new(client).with(TracingMiddleware::<SpanBackendWithUrl>::new()).build()
}
