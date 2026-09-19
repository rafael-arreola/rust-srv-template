//! Capacidad genérica de autenticación de `core`.
//!
//! Este módulo no se conecta a ningún router por sí mismo: es infraestructura técnica reutilizable
//! y el consumidor final decide qué proveedor instanciar y dónde montar [`auth_middleware`]. No
//! depende de `modules`, `vendors` ni `shared`, y su vocabulario es agnóstico al negocio.

pub mod actor;
pub mod jwt_jwks;
pub mod permissions;
pub mod principal;
pub mod provider;

pub use actor::Actor;
pub use jwt_jwks::{JwtJwksAuthProvider, JwtJwksClaims};
pub use permissions::{require_any_permission, require_permission};
pub use principal::AuthPrincipal;
pub use provider::{
    AuthProvider, TaskToken, auth_middleware, extract_bearer_token, task_auth_middleware,
};
