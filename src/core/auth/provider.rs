use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    extract::{Request, State},
    http::{HeaderMap, header::AUTHORIZATION},
    middleware::Next,
    response::Response,
};

use crate::core::auth::principal::AuthPrincipal;
use crate::core::error::{AppError, AppResult};

/// Puerto de autenticación seleccionado una vez al arranque.
///
/// Es el único uso admitido de polimorfismo dinámico en esta capacidad: el proveedor concreto se
/// decide en el composition root y se comparte como `Arc<dyn AuthProvider>`.
#[async_trait]
pub trait AuthProvider: Send + Sync + 'static {
    /// Nombre estable del proveedor, útil para spans y diagnósticos.
    fn name(&self) -> &'static str;

    /// Valida el token y resuelve la identidad autenticada.
    async fn authenticate(&self, token: &str) -> AppResult<AuthPrincipal>;
}

/// Extrae el token de un encabezado `Authorization: Bearer <token>`.
pub fn extract_bearer_token(headers: &HeaderMap) -> AppResult<&str> {
    let auth_header = headers.get(AUTHORIZATION).ok_or_else(|| {
        AppError::unauthorized("Missing Authorization header. A valid Bearer token is required.")
    })?;

    let auth_str = auth_header.to_str().map_err(|_| {
        AppError::unauthorized("The Authorization header must be a valid UTF-8 string.")
    })?;

    let token = auth_str
        .strip_prefix("Bearer ")
        .or_else(|| auth_str.strip_prefix("bearer "))
        .map(str::trim)
        .ok_or_else(|| {
            AppError::unauthorized(
                "The Authorization header must use the Bearer scheme ('Bearer <token>').",
            )
        })?;

    if token.is_empty() {
        return Err(AppError::unauthorized("The provided Bearer token is empty."));
    }

    Ok(token)
}

/// Middleware de autenticación: resuelve el token y adjunta el [`AuthPrincipal`] a la request.
pub async fn auth_middleware(
    State(provider): State<Arc<dyn AuthProvider>>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let token = extract_bearer_token(req.headers())?;
    let principal = provider.authenticate(token).await?;
    req.extensions_mut().insert(principal);
    Ok(next.run(req).await)
}

/// Token compartido para invocaciones internas (por ejemplo, workers de Cloud Tasks).
#[derive(Clone)]
pub struct TaskToken(pub String);

/// Middleware de autorización para tareas internas.
///
/// Acepta el valor crudo o `Authorization: Bearer <token>`. Un token no configurado es un fallo de
/// servidor, no un 401.
pub async fn task_auth_middleware(
    State(token): State<TaskToken>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    if token.0.is_empty() {
        return Err(AppError::internal("task token is not configured"));
    }

    let auth_header =
        req.headers().get(AUTHORIZATION).and_then(|value| value.to_str().ok()).unwrap_or_default();

    let expected_bearer = format!("Bearer {}", token.0);
    if auth_header != token.0.as_str() && auth_header != expected_bearer.as_str() {
        return Err(AppError::unauthorized("Invalid task authorization token"));
    }

    Ok(next.run(req).await)
}
