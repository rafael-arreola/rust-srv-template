use std::sync::atomic::{AtomicBool, Ordering};

static DRAINING: AtomicBool = AtomicBool::new(false);

/// Marca el proceso como en drenado tras recibir la señal de apagado; `/healthz` responde 503.
///
/// Al resolver la señal axum deja de aceptar conexiones, así que el 503 solo lo ve una sonda que
/// reutiliza una conexión keep-alive. En Cloud Run no hace falta más: SIGTERM llega cuando la
/// instancia ya salió del balanceo. En Kubernetes, sacar el pod de los endpoints antes del cierre
/// es trabajo de un hook `preStop` (`sleep`) en el manifiesto, no de una espera aquí, que
/// consumiría el mismo margen de gracia que necesitan el drenado y el flush de trazas.
pub fn start_draining() {
    DRAINING.store(true, Ordering::SeqCst);
}

/// Indica si el proceso está drenando conexiones.
pub fn is_draining() -> bool {
    DRAINING.load(Ordering::SeqCst)
}
