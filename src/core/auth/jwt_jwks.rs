//! Proveedor de autenticación basado en JWT validado contra un JWKS remoto.
//!
//! Antes de construir [`JwtJwksAuthProvider`], el consumidor debe instalar el proveedor
//! criptográfico de `jsonwebtoken` una única vez durante el arranque, según la feature de
//! criptografía habilitada. Este módulo no lo instala por sí mismo para no imponer una elección
//! global de criptografía.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use axum_jwt_auth::{JwtDecoder, RemoteJwksDecoder};
use jsonwebtoken::{Algorithm, Validation};
use serde::{Deserialize, Serialize};

use crate::core::auth::principal::AuthPrincipal;
use crate::core::auth::provider::AuthProvider;
use crate::core::error::{AppError, AppResult};

/// Claims genéricos de un JWT validado contra un JWKS remoto.
///
/// Todos los campos son opcionales para tolerar emisores que omiten parte del payload.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct JwtJwksClaims {
    #[serde(default)]
    pub sub: Option<String>,
    #[serde(default)]
    pub exp: u64,
    #[serde(default)]
    pub iss: Option<String>,
    #[serde(default)]
    pub azp: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
}

impl JwtJwksClaims {
    /// Resuelve el identificador de cliente con la precedencia `client_id` → `azp` → `sub`.
    pub fn get_client_id(&self) -> Option<String> {
        [self.client_id.as_deref(), self.azp.as_deref(), self.sub.as_deref()]
            .into_iter()
            .flatten()
            .map(str::trim)
            .find(|value| !value.is_empty())
            .map(str::to_string)
    }

    /// Une los permisos explícitos con los scopes separados por espacios, descartando vacíos.
    pub fn all_permissions(&self) -> HashSet<String> {
        let mut permissions = HashSet::new();
        for permission in &self.permissions {
            let trimmed = permission.trim();
            if !trimmed.is_empty() {
                permissions.insert(trimmed.to_string());
            }
        }
        if let Some(scope) = &self.scope {
            for permission in scope.split_whitespace() {
                let trimmed = permission.trim();
                if !trimmed.is_empty() {
                    permissions.insert(trimmed.to_string());
                }
            }
        }
        permissions
    }
}

/// Proveedor JWT/JWKS con firma `RS256` validada contra el endpoint JWKS configurado.
#[derive(Clone)]
pub struct JwtJwksAuthProvider {
    decoder: Arc<RemoteJwksDecoder>,
    allowed_client_ids: Vec<String>,
}

impl JwtJwksAuthProvider {
    /// Construye e inicializa el decodificador remoto de JWKS.
    ///
    /// Valida `exp` con algoritmo `RS256` y no valida `aud`; la allowlist de clientes se aplica
    /// después de decodificar el token. Un fallo de construcción o inicialización es fatal.
    pub async fn new(
        jwks_url: impl Into<String>,
        allowed_client_ids: Vec<String>,
    ) -> AppResult<Self> {
        let url = jwks_url.into();

        let mut validation = Validation::new(Algorithm::RS256);
        validation.validate_exp = true;
        validation.required_spec_claims.insert("exp".to_string());
        validation.validate_aud = false;

        let decoder = RemoteJwksDecoder::builder()
            .jwks_url(url.clone())
            .validation(validation)
            .build()
            .map_err(|e| {
                AppError::internal(format!("Failed to build JWKS decoder for URL {url}: {e}"))
            })?;

        decoder.initialize().await.map_err(|e| {
            AppError::internal(format!("Failed to initialize JWKS decoder from {url}: {e}"))
        })?;

        Ok(Self { decoder: Arc::new(decoder), allowed_client_ids })
    }
}

#[async_trait]
impl AuthProvider for JwtJwksAuthProvider {
    fn name(&self) -> &'static str {
        "jwt-jwks"
    }

    #[tracing::instrument(skip_all, fields(provider = "jwt-jwks"))]
    async fn authenticate(&self, token: &str) -> AppResult<AuthPrincipal> {
        let clean_token = token
            .strip_prefix("Bearer ")
            .or_else(|| token.strip_prefix("bearer "))
            .unwrap_or(token)
            .trim();

        let token_data = JwtDecoder::<JwtJwksClaims>::decode(self.decoder.as_ref(), clean_token)
            .await
            .map_err(|e| AppError::unauthorized(format!("JWT validation failed: {e}")))?;

        let client_id = token_data.claims.get_client_id();

        if !self.allowed_client_ids.is_empty() {
            let is_allowed = client_id
                .as_deref()
                .is_some_and(|id| self.allowed_client_ids.iter().any(|allowed| allowed == id));
            if !is_allowed {
                return Err(AppError::forbidden("Forbidden: unauthorized client id"));
            }
        }

        Ok(AuthPrincipal {
            provider: "jwt-jwks",
            client_id: client_id.unwrap_or_else(|| "unknown_client".to_string()),
            user_id: token_data.claims.sub.clone(),
            email: token_data.claims.email.clone(),
            permissions: token_data.claims.all_permissions(),
            token: clean_token.to_string(),
        })
    }
}
