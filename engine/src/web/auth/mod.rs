pub mod jwks;
pub mod policy;

use std::collections::HashSet;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    Router,
    extract::{FromRequestParts, Request, State},
    http::{HeaderMap, HeaderName, header::AUTHORIZATION, request::Parts},
    middleware::{self, Next},
    response::Response,
};

pub use self::jwks::{JwtJwksAuthProvider, JwtJwksClaims, JwtJwksConfig};
pub use self::policy::{Policy, RequirePermissionExt};
pub use crate::context::{Actor, Context, DelegatedCredential};
use crate::error::{AppError, AppResult};
use crate::registry::Registry;

const MAX_AUTHORIZATION_HEADER_BYTES: usize = 16 * 1024 + 64;
const X_FORWARDED_AUTHORIZATION: &str = "x-forwarded-authorization";
static AUTH_REGISTRY: Registry<Arc<dyn AuthProvider>> = Registry::new("Auth");

/// Fachada de proveedores de autenticación nombrados para arquitecturas multi-reino
/// (ej. `Auth::register("console", auth0)`, `Auth::register("ingestion", hydra)`).
pub struct Auth;

impl Auth {
    pub const DEFAULT: &'static str = "default";

    pub fn register<P: AuthProvider>(instance: &str, provider: P) -> Arc<dyn AuthProvider> {
        let arc: Arc<dyn AuthProvider> = Arc::new(provider);
        AUTH_REGISTRY.register(instance, arc.clone());
        arc
    }

    #[track_caller]
    pub fn get(instance: &str) -> AppResult<Arc<dyn AuthProvider>> {
        AUTH_REGISTRY.get(instance)
    }

    pub fn try_get(instance: &str) -> Option<Arc<dyn AuthProvider>> {
        AUTH_REGISTRY.try_get(instance)
    }

    pub async fn scope<F: Future>(
        instance: &str,
        provider: Arc<dyn AuthProvider>,
        fut: F,
    ) -> F::Output {
        AUTH_REGISTRY.scope(instance, provider, fut).await
    }
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

/// Identidad autenticada resuelta por un proveedor de autenticación.
#[derive(Clone, PartialEq, Eq)]
pub struct AuthPrincipal {
    pub provider: &'static str,
    pub client_id: String,
    pub user_id: Option<String>,
    pub email: Option<String>,
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
            .field("email", &self.email)
            .field("permissions", &self.permissions)
            .field("token", &"<redacted>")
            .finish()
    }
}

impl AuthPrincipal {
    /// Constructor mínimo para tokens básicos o clientes M2M sin tener que armar el struct completo.
    pub fn new(provider: &'static str, client_id: impl Into<String>) -> Self {
        Self {
            provider,
            client_id: client_id.into(),
            user_id: None,
            email: None,
            permissions: HashSet::new(),
            token: String::new(),
        }
    }

    #[must_use]
    pub fn with_user(mut self, user_id: impl Into<String>, email: Option<String>) -> Self {
        self.user_id = Some(user_id.into());
        self.email = email;
        self
    }

    #[must_use]
    pub fn with_permissions<I, S>(mut self, permissions: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.permissions = permissions.into_iter().map(Into::into).collect();
        self
    }

    pub fn has_permission(&self, permission: &str) -> bool {
        let trimmed = permission.trim();
        if trimmed.is_empty() {
            return false;
        }
        self.permissions.contains(trimmed) || self.permissions.contains("*")
    }

    pub fn has_all_permissions(&self, permissions: &[&str]) -> bool {
        !permissions.is_empty()
            && permissions.iter().all(|permission| self.has_permission(permission))
    }

    pub fn has_any_permission(&self, permissions: &[&str]) -> bool {
        permissions.iter().any(|permission| self.has_permission(permission))
    }
}

impl<S: Send + Sync> FromRequestParts<S> for AuthPrincipal {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<AuthPrincipal>().cloned().ok_or_else(|| {
            AppError::unauthorized("Missing authentication context. Bearer token required.")
        })
    }
}

#[async_trait]
pub trait AuthProvider: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    async fn authenticate(&self, token: &str) -> AppResult<AuthPrincipal>;
}

type TokenEvaluator = Arc<
    dyn Fn(String) -> Pin<Box<dyn Future<Output = AppResult<AuthPrincipal>> + Send>> + Send + Sync,
>;

/// Proveedor de autenticación por token básico / API Key donde el servicio define su propia
/// fórmula asíncrona de evaluación (ej. consulta en Redis/Mongo, HMAC, o secreto estático).
#[derive(Clone)]
pub struct TokenAuthProvider {
    provider_name: &'static str,
    evaluator: TokenEvaluator,
}

impl TokenAuthProvider {
    pub fn new<F, Fut>(provider_name: &'static str, evaluator: F) -> Self
    where
        F: Fn(String) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = AppResult<AuthPrincipal>> + Send + 'static,
    {
        Self { provider_name, evaluator: Arc::new(move |token| Box::pin(evaluator(token))) }
    }

    /// Variante directa para tokens estáticos compartidos comparados en tiempo constante.
    pub fn static_secret(provider_name: &'static str, secret: impl Into<String>) -> Self {
        let expected = Arc::new(secret.into());
        Self::new(provider_name, move |presented| {
            let expected = Arc::clone(&expected);
            async move {
                if expected.is_empty() {
                    return Err(AppError::internal(format!(
                        "auth provider '{provider_name}' secret is not configured"
                    )));
                }
                if !constant_time_eq(presented.as_bytes(), expected.as_bytes()) {
                    return Err(AppError::unauthorized("Invalid authentication token."));
                }
                Ok(AuthPrincipal::new(provider_name, provider_name))
            }
        })
    }
}

#[async_trait]
impl AuthProvider for TokenAuthProvider {
    fn name(&self) -> &'static str {
        self.provider_name
    }

    #[tracing::instrument(skip_all, fields(provider = self.provider_name))]
    async fn authenticate(&self, token: &str) -> AppResult<AuthPrincipal> {
        let mut principal = (self.evaluator)(token.to_string()).await?;
        if principal.token.is_empty() {
            principal.token = token.to_string();
        }
        Ok(principal)
    }
}

/// Extrae `Bearer <token>` desde `X-Forwarded-Authorization` (prioritario tras API Gateway) o
/// `Authorization`.
pub fn extract_bearer_token(headers: &HeaderMap) -> AppResult<&str> {
    let header_name = if headers.contains_key(X_FORWARDED_AUTHORIZATION) {
        HeaderName::from_static(X_FORWARDED_AUTHORIZATION)
    } else {
        AUTHORIZATION
    };

    let mut values = headers.get_all(&header_name).iter();
    let auth_header = values.next().ok_or_else(|| {
        AppError::unauthorized("Missing Authorization header. A valid Bearer token is required.")
    })?;

    if values.next().is_some() || auth_header.as_bytes().len() > MAX_AUTHORIZATION_HEADER_BYTES {
        return Err(AppError::unauthorized("Unsupported Authorization header."));
    }

    let auth_str = auth_header
        .to_str()
        .map_err(|_| {
            AppError::unauthorized("The Authorization header must be a valid UTF-8 string.")
        })?
        .trim();

    let (scheme, credential) =
        auth_str.split_once(|ch: char| ch.is_ascii_whitespace()).ok_or_else(|| {
            AppError::unauthorized(
                "The Authorization header must use the Bearer scheme ('Bearer <token>').",
            )
        })?;

    if !scheme.eq_ignore_ascii_case("Bearer") {
        return Err(AppError::unauthorized(
            "The Authorization header must use the Bearer scheme ('Bearer <token>').",
        ));
    }

    let token = credential.trim();
    if token.is_empty() {
        return Err(AppError::unauthorized("The provided Bearer token is empty."));
    }

    Ok(token)
}

fn extract_raw_header_token<'a>(headers: &'a HeaderMap, header_name: &str) -> AppResult<&'a str> {
    let mut values = headers.get_all(header_name).iter();
    let value = values.next().ok_or_else(|| {
        AppError::unauthorized(format!("Missing required '{header_name}' credential header."))
    })?;

    if values.next().is_some() || value.as_bytes().len() > MAX_AUTHORIZATION_HEADER_BYTES {
        return Err(AppError::unauthorized(format!("Unsupported '{header_name}' header.")));
    }

    let token = value
        .to_str()
        .map_err(|_| {
            AppError::unauthorized(format!("The '{header_name}' header must be valid UTF-8."))
        })?
        .trim();

    if token.is_empty() {
        return Err(AppError::unauthorized(format!("The '{header_name}' token is empty.")));
    }

    Ok(token)
}

fn strip_auth_headers(headers: &mut HeaderMap) {
    headers.remove(AUTHORIZATION);
    headers.remove(X_FORWARDED_AUTHORIZATION);
}

#[tracing::instrument(skip_all, fields(provider = provider.name()))]
pub async fn auth_middleware(
    State(provider): State<Arc<dyn AuthProvider>>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let token = extract_bearer_token(req.headers())?;
    let principal = provider.authenticate(token).await?;
    strip_auth_headers(req.headers_mut());
    Context::from_extensions(req.extensions_mut()).insert(Actor::from(&principal));
    req.extensions_mut().insert(principal);
    Ok(next.run(req).await)
}

#[tracing::instrument(skip_all, fields(provider = provider.name()))]
pub async fn relay_auth_middleware(
    State(provider): State<Arc<dyn AuthProvider>>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let credential = extract_bearer_token(req.headers())?.to_owned();
    let principal = provider.authenticate(&credential).await?;
    strip_auth_headers(req.headers_mut());
    let ctx = Context::from_extensions(req.extensions_mut());
    ctx.insert(Actor::from(&principal));
    ctx.insert(DelegatedCredential::new(credential));
    req.extensions_mut().insert(principal);
    Ok(next.run(req).await)
}

#[tracing::instrument(skip_all, fields(provider = provider.name(), header = header_name))]
pub async fn header_auth_middleware(
    State((header_name, provider)): State<(&'static str, Arc<dyn AuthProvider>)>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let token = extract_raw_header_token(req.headers(), header_name)?;
    let principal = provider.authenticate(token).await?;
    req.headers_mut().remove(header_name);
    Context::from_extensions(req.extensions_mut()).insert(Actor::from(&principal));
    req.extensions_mut().insert(principal);
    Ok(next.run(req).await)
}

/// Extensión fluida para proteger reinos de rutas (`ui`, `xb`, `worker`, `b2b`) en `routes.rs`.
pub trait RouterAuthExt: Sized {
    fn with_jwt_auth(self, provider: Arc<dyn AuthProvider>) -> Self;
    fn with_relay_auth(self, provider: Arc<dyn AuthProvider>) -> Self;
    fn with_header_auth(self, header_name: &'static str, provider: Arc<dyn AuthProvider>) -> Self;
    fn with_task_auth(self, token: TaskToken) -> Self;
}

impl<S: Clone + Send + Sync + 'static> RouterAuthExt for Router<S> {
    fn with_jwt_auth(self, provider: Arc<dyn AuthProvider>) -> Self {
        self.route_layer(middleware::from_fn_with_state(provider, auth_middleware))
    }

    fn with_relay_auth(self, provider: Arc<dyn AuthProvider>) -> Self {
        self.route_layer(middleware::from_fn_with_state(provider, relay_auth_middleware))
    }

    fn with_header_auth(self, header_name: &'static str, provider: Arc<dyn AuthProvider>) -> Self {
        self.route_layer(middleware::from_fn_with_state(
            (header_name, provider),
            header_auth_middleware,
        ))
    }

    fn with_task_auth(self, token: TaskToken) -> Self {
        self.route_layer(middleware::from_fn_with_state(token, task_auth_middleware))
    }
}

#[derive(Clone)]
pub struct TaskToken(pub String);

pub async fn task_auth_middleware(
    State(token): State<TaskToken>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    if token.0.is_empty() {
        return Err(AppError::internal("task token is not configured"));
    }

    let presented = extract_bearer_token(req.headers())?;
    if !constant_time_eq(presented.as_bytes(), token.0.as_bytes()) {
        return Err(AppError::unauthorized("Invalid task authorization token"));
    }

    strip_auth_headers(req.headers_mut());
    Ok(next.run(req).await)
}

/// Comparación en tiempo constante para validar secretos o API Keys sin filtración temporal.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let difference = a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y));
    std::hint::black_box(difference) == 0
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        routing::get,
    };
    use tower::ServiceExt;

    use super::*;

    struct StubAuth;

    #[async_trait]
    impl AuthProvider for StubAuth {
        fn name(&self) -> &'static str {
            "stub"
        }

        async fn authenticate(&self, token: &str) -> AppResult<AuthPrincipal> {
            Ok(AuthPrincipal::new("stub", "ui-app")
                .with_user("u1", None)
                .with_permissions(["orders:read"]))
            .map(|mut p| {
                p.token = token.into();
                p
            })
        }
    }

    #[tokio::test]
    async fn fluent_permissions_and_relay_auth_enforce_policies_and_strip_header() {
        let provider: Arc<dyn AuthProvider> = Arc::new(StubAuth);
        let app = Router::new()
            .route(
                "/orders",
                get(|ctx: crate::Context, req: Request<Body>| async move {
                    assert!(req.headers().get(AUTHORIZATION).is_none());
                    assert!(req.headers().get(X_FORWARDED_AUTHORIZATION).is_none());
                    assert_eq!(
                        ctx.require::<DelegatedCredential>().expect("cred").as_str(),
                        "tok123"
                    );
                    "ok"
                })
                .require_permission("orders:read"),
            )
            .route("/admin", get(|| async { "secret" }).require_permission("orders:admin"))
            .with_relay_auth(provider);

        let ok = app
            .clone()
            .oneshot(
                Request::get("/orders")
                    .header(X_FORWARDED_AUTHORIZATION, "Bearer tok123")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("res");
        assert_eq!(ok.status(), StatusCode::OK);

        let forbidden = app
            .oneshot(
                Request::get("/admin")
                    .header(AUTHORIZATION, "Bearer tok123")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("res");
        assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn custom_token_evaluator_and_header_auth_work_seamlessly() {
        let provider = Auth::register(
            "api-key",
            TokenAuthProvider::new("api-key", |token| async move {
                if token == "k_live_123" {
                    Ok(AuthPrincipal::new("api-key", "partner_99")
                        .with_permissions(["shipments:write"]))
                } else {
                    Err(AppError::unauthorized("invalid key"))
                }
            }),
        );

        let app = Router::new()
            .route(
                "/hook",
                get(|ctx: Context, req: Request<Body>| async move {
                    assert!(req.headers().get("x-api-key").is_none());
                    ctx.require::<Actor>().expect("actor").client_id
                })
                .require_permission("shipments:write"),
            )
            .with_header_auth("x-api-key", provider);

        let ok = app
            .clone()
            .oneshot(
                Request::get("/hook")
                    .header("x-api-key", "k_live_123")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("res");
        assert_eq!(ok.status(), StatusCode::OK);

        let bad = app
            .oneshot(
                Request::get("/hook")
                    .header("x-api-key", "wrong")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("res");
        assert_eq!(bad.status(), StatusCode::UNAUTHORIZED);
    }
}
