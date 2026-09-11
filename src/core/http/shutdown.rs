use std::sync::atomic::{AtomicBool, Ordering};

static DRAINING: AtomicBool = AtomicBool::new(false);

/// Marca el proceso como en drenado tras recibir la señal de apagado.
/// Las sondas de readiness deben responder 503 para que el balanceador deje de enrutar
/// tráfico nuevo mientras se cierran las conexiones en vuelo.
pub fn start_draining() {
    DRAINING.store(true, Ordering::SeqCst);
}

/// Indica si el proceso está drenando conexiones.
pub fn is_draining() -> bool {
    DRAINING.load(Ordering::SeqCst)
}
