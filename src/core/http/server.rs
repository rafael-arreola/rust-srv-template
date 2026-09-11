use axum::Router;
use axum::extract::DefaultBodyLimit;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::signal;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
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
            drain_timeout: Duration::from_secs(10),
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
    /// Un fallo de bind se propaga en lugar de registrarse y continuar: el proceso debe terminar
    /// con código distinto de cero para que el orquestador detecte el arranque fallido.
    pub async fn run(self) -> AppResult<()> {
        let cors = if self.cors_origins == "*" {
            CorsLayer::permissive().allow_methods(Any).allow_headers(Any)
        } else {
            let origins: Vec<_> =
                self.cors_origins.split(',').filter_map(|s| s.trim().parse().ok()).collect();
            CorsLayer::new().allow_methods(Any).allow_headers(Any).allow_origin(origins)
        };

        let mut app = self.router;

        if self.msgpack_enabled {
            app = app.layer(axum::middleware::from_fn(msgpack_negotiation));
        }

        let app = app
            .layer(axum::middleware::from_fn_with_state(self.request_timeout, request_timeout))
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

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = signal::ctrl_c().await {
            tracing::error!("Failed to install Ctrl+C handler: {}", e);
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
