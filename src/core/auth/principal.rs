use std::collections::HashSet;
use std::fmt;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use crate::core::error::AppError;

/// Identidad autenticada resuelta por un proveedor de autenticación.
///
/// Contiene vocabulario agnóstico al negocio: proveedor, cliente, usuario opcional, permisos y el
/// token que originó la autenticación. El token se redacta en `Debug` para no filtrarlo en logs.
#[derive(Clone, PartialEq, Eq)]
pub struct AuthPrincipal {
    pub provider: &'static str,
    pub client_id: String,
    pub user_id: Option<String>,
    pub permissions: HashSet<String>,
    pub token: String,
}

impl fmt::Debug for AuthPrincipal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthPrincipal")
            .field("provider", &self.provider)
            .field("client_id", &self.client_id)
            .field("user_id", &self.user_id)
            .field("permissions", &self.permissions)
            .field("token", &"<redacted>")
            .finish()
    }
}

impl AuthPrincipal {
    /// Indica si el principal posee el permiso solicitado.
    ///
    /// Un permiso vacío no exige nada y siempre se concede. El comodín `"*"` entre los permisos
    /// concedidos autoriza cualquier solicitud.
    pub fn has_permission(&self, permission: &str) -> bool {
        let trimmed = permission.trim();
        if trimmed.is_empty() {
            return true;
        }
        self.permissions.contains(trimmed) || self.permissions.contains("*")
    }

    /// Exige que el principal posea todos los permisos indicados.
    pub fn has_all_permissions(&self, permissions: &[&str]) -> bool {
        permissions.iter().all(|permission| self.has_permission(permission))
    }

    /// Exige que el principal posea al menos uno de los permisos indicados.
    pub fn has_any_permission(&self, permissions: &[&str]) -> bool {
        permissions.iter().any(|permission| self.has_permission(permission))
    }
}

impl<S> FromRequestParts<S> for AuthPrincipal
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<AuthPrincipal>().cloned().ok_or_else(|| {
            AppError::unauthorized("Missing authentication context. Bearer token required.")
        })
    }
}
