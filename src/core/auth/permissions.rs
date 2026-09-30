use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};

use crate::core::auth::principal::AuthPrincipal;
use crate::core::error::AppError;

/// Middleware que exige un permiso concreto sobre el [`AuthPrincipal`] ya resuelto.
///
/// La ausencia de contexto de autenticación es 401; un principal sin el permiso exigido es 403.
/// El nombre del permiso que faltó viaja solo en el detalle interno: enumerar al cliente los
/// permisos que gobiernan cada ruta es cartografiar la autorización para quien la sondea.
pub async fn require_permission(
    State(required_permission): State<&'static str>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let authorized = req
        .extensions()
        .get::<AuthPrincipal>()
        .map(|principal| principal.has_permission(required_permission));

    match authorized {
        Some(true) => Ok(next.run(req).await),
        Some(false) => Err(AppError::forbidden("")
            .with_internal(format!("missing required permission '{required_permission}'"))),
        None => {
            Err(AppError::unauthorized("Missing authentication context. Bearer token required."))
        }
    }
}

/// Middleware que exige al menos uno de los permisos indicados (lógica OR).
pub async fn require_any_permission(
    State(required_permissions): State<&'static [&'static str]>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let authorized = req
        .extensions()
        .get::<AuthPrincipal>()
        .map(|principal| principal.has_any_permission(required_permissions));

    match authorized {
        Some(true) => Ok(next.run(req).await),
        Some(false) => Err(AppError::forbidden("").with_internal(format!(
            "insufficient permissions; requires any of {required_permissions:?}"
        ))),
        None => {
            Err(AppError::unauthorized("Missing authentication context. Bearer token required."))
        }
    }
}
