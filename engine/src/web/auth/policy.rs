use axum::{
    Router,
    extract::{Request, State},
    middleware::{self, Next},
    response::Response,
    routing::MethodRouter,
};

use super::AuthPrincipal;
use crate::error::AppError;

/// Política declarativa de permisos RBAC por ruta o sub-router.
#[derive(Clone, Copy, Debug)]
pub enum Policy {
    Single(&'static str),
    Any(&'static [&'static str]),
    All(&'static [&'static str]),
}

impl Policy {
    fn is_valid(self) -> bool {
        match self {
            Self::Single(p) => !p.trim().is_empty(),
            Self::Any(ps) | Self::All(ps) => {
                !ps.is_empty() && ps.iter().all(|p| !p.trim().is_empty())
            }
        }
    }

    fn accepts(self, principal: &AuthPrincipal) -> bool {
        match self {
            Self::Single(p) => principal.has_permission(p),
            Self::Any(ps) => principal.has_any_permission(ps),
            Self::All(ps) => principal.has_all_permissions(ps),
        }
    }
}

async fn enforce_policy(
    State(policy): State<Policy>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    if !policy.is_valid() {
        return Err(AppError::internal("Route authorization policy has empty permissions"));
    }

    let principal = req.extensions().get::<AuthPrincipal>().ok_or_else(|| {
        AppError::unauthorized("Missing authentication context. Bearer token required.")
    })?;

    if !policy.accepts(principal) {
        return Err(AppError::forbidden("")
            .with_internal(format!("insufficient permissions for policy {policy:?}")));
    }

    Ok(next.run(req).await)
}

/// Extensión fluida para exigir permisos directamente sobre rutas (`get(h).require_permission(...)`)
/// o sub-routers completos.
pub trait RequirePermissionExt: Sized {
    fn require_policy(self, policy: Policy) -> Self;

    fn require_permission(self, permission: &'static str) -> Self {
        self.require_policy(Policy::Single(permission))
    }

    fn require_any_permission(self, permissions: &'static [&'static str]) -> Self {
        self.require_policy(Policy::Any(permissions))
    }

    fn require_all_permissions(self, permissions: &'static [&'static str]) -> Self {
        self.require_policy(Policy::All(permissions))
    }
}

impl<S: Clone + Send + Sync + 'static> RequirePermissionExt for MethodRouter<S> {
    fn require_policy(self, policy: Policy) -> Self {
        self.route_layer(middleware::from_fn_with_state(policy, enforce_policy))
    }
}

impl<S: Clone + Send + Sync + 'static> RequirePermissionExt for Router<S> {
    fn require_policy(self, policy: Policy) -> Self {
        self.route_layer(middleware::from_fn_with_state(policy, enforce_policy))
    }
}
