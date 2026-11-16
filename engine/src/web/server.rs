use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{HeaderName, HeaderValue, Method, StatusCode, Uri, header},
    response::IntoResponse,
    routing::get,
};
use tokio::signal;
use tower_http::{
    compression::CompressionLayer,
    cors::{AllowHeaders, Any, CorsLayer},
    decompression::RequestDecompressionLayer,
};

use super::layers::{msgpack_negotiation, request_timeout, trace_context};
use crate::context::Context;
use crate::error::{AppError, AppResult};

const DEFAULT_MAX_BODY_BYTES: usize = 32 * 1024 * 1024;
static DRAINING: AtomicBool = AtomicBool::new(false);

pub fn start_draining() {
    DRAINING.store(true, Ordering::SeqCst);
}

pub fn is_draining() -> bool {
    DRAINING.load(Ordering::SeqCst)
}

/// Sonda de vida `GET /healthz` (200 en servicio, 503 en drenado).
pub fn health_router() -> Router {
    Router::new().route("/healthz", get(healthz))
}

#[tracing::instrument(skip_all)]
async fn healthz(_ctx: Context) -> impl IntoResponse {
    if is_draining() { StatusCode::SERVICE_UNAVAILABLE } else { StatusCode::OK }
}

pub async fn not_found(method: Method, uri: Uri) -> AppError {
    AppError::censored(
        StatusCode::NOT_FOUND,
        "ROUTE_NOT_FOUND",
        "The requested resource was not found.",
        format!("no route matches {} {}", method, uri.path()),
    )
}

pub async fn method_not_allowed(method: Method, uri: Uri) -> AppError {
    AppError::censored(
        StatusCode::METHOD_NOT_ALLOWED,
        "METHOD_NOT_ALLOWED",
        "The HTTP method is not supported for this resource.",
        format!("method {} not allowed on {}", method, uri.path()),
    )
}

#[derive(Clone)]
pub struct ServerLauncher {
    router: Router,
    http_port: u16,
    cors_origins: String,
    drain_timeout: Duration,
    request_timeout: Duration,
    body_limit_bytes: usize,
    msgpack_enabled: bool,
    health_check_enabled: bool,
    default_fallbacks_enabled: bool,
}

impl ServerLauncher {
    pub fn new(router: Router) -> Self {
        Self {
            router,
            http_port: 3000,
            cors_origins: "*".to_string(),
            drain_timeout: Duration::from_secs(8),
            request_timeout: Duration::from_secs(30),
            body_limit_bytes: DEFAULT_MAX_BODY_BYTES,
            msgpack_enabled: true,
            health_check_enabled: true,
            default_fallbacks_enabled: true,
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.http_port = port;
        self
    }

    pub fn with_cors_origins(mut self, cors_origins: impl Into<String>) -> Self {
        self.cors_origins = cors_origins.into();
        self
    }

    pub fn with_drain_timeout(mut self, secs: u64) -> Self {
        self.drain_timeout = Duration::from_secs(secs);
        self
    }

    pub fn with_request_timeout(mut self, secs: u64) -> Self {
        self.request_timeout = Duration::from_secs(secs);
        self
    }

    pub fn with_body_limit(mut self, max_bytes: usize) -> Self {
        self.body_limit_bytes = max_bytes;
        self
    }

    pub fn with_msgpack(mut self, enabled: bool) -> Self {
        self.msgpack_enabled = enabled;
        self
    }

    pub fn with_health_check(mut self, enabled: bool) -> Self {
        self.health_check_enabled = enabled;
        self
    }

    pub fn with_default_fallbacks(mut self, enabled: bool) -> Self {
        self.default_fallbacks_enabled = enabled;
        self
    }

    /// Ensambla el `Router` final con `/healthz`, fallbacks `404`/`405` y la pila completa de
    /// middlewares (reutilizable tanto en `run` como en pruebas HTTP en memoria con `oneshot`).
    pub fn into_router(self) -> AppResult<Router> {
        let exposed = [header::CONTENT_DISPOSITION, HeaderName::from_static("x-trace-id")];
        let cors = if self.cors_origins == "*" {
            CorsLayer::permissive()
                .allow_headers(AllowHeaders::mirror_request())
                .expose_headers(exposed)
        } else {
            CorsLayer::new()
                .allow_methods(Any)
                .allow_headers(AllowHeaders::mirror_request())
                .allow_origin(parse_cors_origins(&self.cors_origins)?)
                .expose_headers(exposed)
        };

        let mut base = self.router;
        if self.health_check_enabled {
            base = base.merge(health_router());
        }
        if self.default_fallbacks_enabled {
            base = base.fallback(not_found).method_not_allowed_fallback(method_not_allowed);
        }

        let mut app =
            base.layer(axum::middleware::from_fn_with_state(self.request_timeout, request_timeout));

        if self.msgpack_enabled {
            app = app.layer(axum::middleware::from_fn(msgpack_negotiation));
        }

        // `DefaultBodyLimit` va dentro de `RequestDecompressionLayer` para acotar el tamaño real
        // una vez descomprimido y proteger la memoria frente a payloads comprimidos inflados.
        Ok(app
            .layer(axum::middleware::from_fn(trace_context))
            .layer(DefaultBodyLimit::max(self.body_limit_bytes))
            .layer(CompressionLayer::new())
            .layer(RequestDecompressionLayer::new())
            .layer(cors))
    }

    pub async fn run(self) -> AppResult<()> {
        let http_port = self.http_port;
        let drain_timeout = self.drain_timeout;
        let app = self.into_router()?;

        let addr = SocketAddr::from(([0, 0, 0, 0], http_port));
        tracing::info!("REST server listening on {} (drain_timeout={:?})", addr, drain_timeout);

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| AppError::internal(format!("Failed to bind to {addr}: {e}")))?;

        let (drain_tx, drain_rx) = tokio::sync::oneshot::channel();

        let graceful = axum::serve(listener, app).with_graceful_shutdown(async move {
            shutdown_signal().await;
            let _ = drain_tx.send(());
        });

        let drain_deadline = async move {
            if drain_rx.await.is_ok() {
                tokio::time::sleep(drain_timeout).await;
            } else {
                std::future::pending::<()>().await;
            }
        };

        tokio::select! {
            res = graceful => {
                res.map_err(|e| AppError::internal(format!("Server error: {e}")))?;
            }
            _ = drain_deadline => {
                tracing::warn!(
                    "Drain timeout ({:?}) exceeded; terminating in-flight connections",
                    drain_timeout
                );
            }
        }

        Ok(())
    }
}

fn parse_cors_origins(raw: &str) -> AppResult<Vec<HeaderValue>> {
    let origins: Vec<HeaderValue> = raw
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(parse_cors_origin)
        .collect::<AppResult<_>>()?;

    if origins.is_empty() {
        return Err(AppError::internal("CORS_ORIGINS is set but contains no origins"));
    }

    Ok(origins)
}

fn parse_cors_origin(entry: &str) -> AppResult<HeaderValue> {
    if entry == "*" {
        return Err(AppError::internal(
            "CORS_ORIGINS mixes '*' with explicit origins; use '*' alone or a list of origins",
        ));
    }

    let uri: Uri = entry
        .parse()
        .map_err(|e| AppError::internal(format!("invalid CORS origin '{entry}': {e}")))?;

    let is_origin = uri.scheme().is_some()
        && uri.authority().is_some()
        && uri.path() == "/"
        && uri.query().is_none()
        && !entry.ends_with('/');

    if !is_origin {
        return Err(AppError::internal(format!(
            "invalid CORS origin '{entry}': expected scheme://host[:port] without path or query"
        )));
    }

    HeaderValue::from_str(entry)
        .map_err(|e| AppError::internal(format!("invalid CORS origin '{entry}': {e}")))
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = signal::ctrl_c().await {
            tracing::error!("Failed to install Ctrl+C handler: {e}");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(e) => {
                tracing::error!("Failed to install SIGTERM handler: {e}");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    start_draining();
    tracing::info!("Shutdown signal received, draining in-flight connections...");
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode, header};
    use axum::routing::get;
    use serde::Deserialize;
    use tower::ServiceExt;

    use super::{method_not_allowed, not_found};

    #[derive(Deserialize)]
    struct ErrorEnvelope {
        code: String,
        cause: String,
    }

    #[tokio::test]
    async fn fallbacks_answer_404_and_405_inside_envelope() {
        let app = Router::new()
            .route("/items", get(|| async { "ok" }))
            .fallback(not_found)
            .method_not_allowed_fallback(method_not_allowed);

        let r404 = app
            .clone()
            .oneshot(Request::get("/missing").body(Body::empty()).expect("req"))
            .await
            .expect("ok");
        assert_eq!(r404.status(), StatusCode::NOT_FOUND);
        let b404: ErrorEnvelope =
            serde_json::from_slice(&to_bytes(r404.into_body(), usize::MAX).await.expect("bytes"))
                .expect("json");
        assert_eq!(b404.code, "ROUTE_NOT_FOUND");
        assert_eq!(b404.cause, "The requested resource was not found.");

        let r405 = app
            .oneshot(Request::delete("/items").body(Body::empty()).expect("req"))
            .await
            .expect("ok");
        assert_eq!(r405.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert!(r405.headers().get(header::ALLOW).is_some());
    }
}
