use std::sync::Arc;

use super::dto::UserOutput;
use super::service::UserService;
use crate::core::context::Context;
use crate::core::error::AppResult;

/// Proyección de solo lectura de `users` para otros módulos.
///
/// Un orquestador que necesita comprobar que un usuario existe recibe este contrato y no el
/// `UserService` completo: así no puede crear, editar ni borrar usuarios por accidente, y la
/// superficie que `users` promete al resto del sistema queda escrita en un solo lugar.
#[derive(Clone)]
pub struct UserLookup {
    service: Arc<UserService>,
}

impl UserLookup {
    pub(super) fn new(service: Arc<UserService>) -> Self {
        Self { service }
    }

    /// Usuario activo por identificador; `NOT_FOUND` si no existe o fue borrado.
    pub async fn find(&self, ctx: &Context, id: &str) -> AppResult<UserOutput> {
        self.service.get_user(ctx, id).await
    }
}
