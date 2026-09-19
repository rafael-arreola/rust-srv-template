use super::principal::AuthPrincipal;

/// Quién ejecuta la operación, en la forma que puede viajar hasta un servicio.
///
/// Es la proyección del [`AuthPrincipal`] **sin** el token ni el conjunto de permisos: el token no
/// debe salir de la capa HTTP y la autorización se decide en el controller, nunca en un servicio.
/// Esto es lo que el middleware de autenticación deja en el [`Context`](crate::core::context::Context).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Actor {
    /// Proveedor que emitió la credencial (por ejemplo `jwt-jwks`, `auth0`).
    pub provider: &'static str,
    /// Aplicación o cliente autenticado.
    pub client_id: String,
    pub email: Option<String>,
}

impl From<&AuthPrincipal> for Actor {
    fn from(principal: &AuthPrincipal) -> Self {
        Self {
            provider: principal.provider,
            client_id: principal.client_id.clone(),
            email: principal.email.clone(),
        }
    }
}
