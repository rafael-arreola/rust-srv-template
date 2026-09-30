use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderValue, Uri};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::signal;
use tower_http::{
    compression::CompressionLayer,
    cors::{AllowHeaders, Any, CorsLayer},
    decompression::RequestDecompressionLayer,
};

use super::error::{AppError, AppResult};
use super::middleware::{msgpack_negotiation, request_timeout, trace_context};
use super::shutdown::start_draining;

const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone)]
pub struct ServerLauncher {
    router: Router,
    http_port: u16,
    cors_origins: String,
    drain_timeout: Duration,
    request_timeout: Duration,
    msgpack_enabled: bool,
}

impl ServerLauncher {
    pub fn new(router: Router) -> Self {
        Self {
            router,
            http_port: 3000,
            cors_origins: "*".to_string(),
            drain_timeout: Duration::from_secs(8),
            request_timeout: Duration::from_secs(30),
            msgpack_enabled: true,
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.http_port = port;
        self
    }

    pub fn with_cors_origins(mut self, cors_origins: String) -> Self {
        self.cors_origins = cors_origins;
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

    pub fn with_msgpack(mut self, enabled: bool) -> Self {
        self.msgpack_enabled = enabled;
        self
    }

    /// Enlaza el puerto y sirve hasta recibir la señal de apagado.
    ///
    /// Un fallo de bind o una configuración CORS inválida se propagan en lugar de registrarse y
    /// continuar: el proceso debe terminar con código distinto de cero para que el orquestador
    /// detecte el arranque fallido.
    pub async fn run(self) -> AppResult<()> {
        // `Authorization` no está cubierto por el comodín `*` de `Access-Control-Allow-Headers`
        // (Fetch §CORS-protocol), así que reflejar los headers solicitados es la única forma de
        // que un navegador con Bearer token pase el preflight sin enumerar cada header a mano.
        let cors = if self.cors_origins == "*" {
            CorsLayer::permissive().allow_headers(AllowHeaders::mirror_request())
        } else {
            CorsLayer::new()
                .allow_methods(Any)
                .allow_headers(AllowHeaders::mirror_request())
                .allow_origin(parse_cors_origins(&self.cors_origins)?)
        };

        // `request_timeout` va dentro de la negociación: el 504 que sintetiza se construye dentro
        // del alcance de `RESPONSE_FORMAT` y respeta `Accept: application/vnd.msgpack`.
        let mut app = self
            .router
            .layer(axum::middleware::from_fn_with_state(self.request_timeout, request_timeout));

        if self.msgpack_enabled {
            app = app.layer(axum::middleware::from_fn(msgpack_negotiation));
        }

        let app = app
            .layer(axum::middleware::from_fn(trace_context))
            .layer(CompressionLayer::new())
            .layer(RequestDecompressionLayer::new())
            .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
            .layer(cors);

        let addr = SocketAddr::from(([0, 0, 0, 0], self.http_port));
        tracing::info!(
            "REST server listening on {} (drain_timeout={:?})",
            addr,
            self.drain_timeout
        );

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| AppError::internal(format!("Failed to bind to {addr}: {e}")))?;

        let (drain_tx, drain_rx) = tokio::sync::oneshot::channel();
        let drain_timeout = self.drain_timeout;

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

/// Interpreta `CORS_ORIGINS` como lista separada por comas de orígenes explícitos.
///
/// Cada entrada debe ser un origen (`esquema://host[:puerto]`, sin ruta ni query). Una entrada
/// inválida o un `*` mezclado con orígenes explícitos es un error de configuración fatal: la
/// alternativa —descartar la entrada en silencio— dejaría a un frontend legítimo bloqueado sin
/// ninguna pista en los logs, y `AllowOrigin::list` entra en pánico ante un comodín.
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

/// Resuelve cuando llega `SIGINT` o `SIGTERM`, y marca el proceso en drenado.
///
/// Si un handler de señal no puede instalarse, ese brazo se queda esperando para siempre en vez
/// de resolver: de lo contrario el `select!` completaría al instante y el servidor comenzaría a
/// apagarse justo después de arrancar.
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = signal::ctrl_c().await {
            tracing::error!("Failed to install Ctrl+C handler: {}", e);
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
                tracing::error!("Failed to install SIGTERM handler: {}", e);
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
