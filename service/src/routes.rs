use engine::web::Router;

use crate::bootstrap::AppContext;
use crate::http;

/// Mapa único de rutas de negocio (`/healthz` y fallbacks `404`/`405` los monta `ServerLauncher`).
pub fn build(state: AppContext) -> Router {
    let api_v1 = Router::new()
        .nest("/users", http::users::routes())
        .nest("/orders", http::orders::routes())
        .nest("/checkout", http::checkout::routes())
        .with_state(state);

    Router::new().nest("/api/v1", api_v1)
}
