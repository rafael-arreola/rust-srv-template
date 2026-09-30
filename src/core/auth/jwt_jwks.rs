//! Proveedor de autenticación basado en JWT validado contra un JWKS remoto.
//!
//! `jsonwebtoken` elige su proveedor criptográfico por feature: con solo `rust_crypto` activa (la
//! que habilita `axum-jwt-auth`) lo resuelve solo. Solo hace falta instalarlo a mano al arranque si
//! se activan a la vez `aws_lc_rs` y `rust_crypto`, o ninguna de las dos.

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

/// Parámetros del proveedor JWT/JWKS. Se agrupan en un struct propio porque `issuer` y
/// `audiences` son obligatorios: un token válido para otra API del mismo emisor comparte el JWKS
/// y solo `aud`/`iss` lo distinguen.
#[derive(Debug, Clone)]
pub struct JwtJwksConfig {
    /// Endpoint JWKS del emisor (`https://.../.well-known/jwks.json`).
    pub jwks_url: String,
    /// Valor exacto que debe traer el claim `iss`.
    pub issuer: String,
    /// Valores admitidos para el claim `aud`; el token debe contener al menos uno.
    pub audiences: Vec<String>,
    /// Allowlist opcional de `client_id`/`azp`/`sub`; vacía significa cualquier cliente del emisor.
    pub allowed_client_ids: Vec<String>,
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
    /// Valida firma `RS256`, `exp`, `nbf`, `iss` y `aud`. `initialize()` descarga las llaves y deja
    /// corriendo la tarea de refresco periódico del propio decodificador, así que una rotación de
    /// llaves en el emisor se absorbe sin reiniciar el proceso. Un fallo de construcción o de la
    /// descarga inicial es fatal.
    pub async fn new(config: JwtJwksConfig) -> AppResult<Self> {
        if config.issuer.trim().is_empty() {
            return Err(AppError::internal("JWT provider requires a non-empty issuer"));
        }
        if config.audiences.iter().all(|audience| audience.trim().is_empty()) {
            return Err(AppError::internal("JWT provider requires at least one audience"));
        }

        // `set_issuer`/`set_audience` solo comparan la claim si viene en el token: sin
        // `required_spec_claims` un token de otra API del mismo IdP que omita `iss` o `aud` pasa.
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_required_spec_claims(&["exp", "iss", "aud"]);
        validation.validate_exp = true;
        validation.validate_nbf = true;
        validation.set_issuer(&[config.issuer.as_str()]);
        validation.set_audience(&config.audiences);

        let url = config.jwks_url;
        let decoder = RemoteJwksDecoder::builder()
            .jwks_url(url.clone())
            .validation(validation)
            .build()
            .map_err(|e| {
                AppError::internal(format!("Failed to build JWKS decoder for URL {url}: {e}"))
            })?;

        // El token de cancelación que devuelve `initialize()` detendría el refresco periódico;
        // se descarta a propósito porque la tarea debe vivir tanto como el proceso.
        decoder.initialize().await.map_err(|e| {
            AppError::internal(format!("Failed to initialize JWKS decoder from {url}: {e}"))
        })?;

        Ok(Self { decoder: Arc::new(decoder), allowed_client_ids: config.allowed_client_ids })
    }
}

#[async_trait]
impl AuthProvider for JwtJwksAuthProvider {
    fn name(&self) -> &'static str {
        "jwt-jwks"
    }

    /// Recibe el token ya extraído del esquema `Bearer` por [`auth_middleware`].
    ///
    /// El motivo concreto del rechazo (firma, expiración, `kid` desconocido, `aud`) se queda en el
    /// detalle interno: al cliente solo le corresponde saber que la credencial no es válida.
    ///
    /// [`auth_middleware`]: crate::core::auth::provider::auth_middleware
    #[tracing::instrument(skip_all, fields(provider = "jwt-jwks"))]
    async fn authenticate(&self, token: &str) -> AppResult<AuthPrincipal> {
        let token_data = JwtDecoder::<JwtJwksClaims>::decode(self.decoder.as_ref(), token)
            .await
            .map_err(|e| {
                AppError::unauthorized("The provided token is invalid or has expired.")
                    .with_internal(format!("JWT validation failed: {e}"))
            })?;

        let client_id = token_data.claims.get_client_id().ok_or_else(|| {
            AppError::unauthorized("The provided token is invalid or has expired.")
                .with_internal("JWT carries no client_id, azp or sub claim")
        })?;

        if !self.allowed_client_ids.is_empty() && !self.allowed_client_ids.contains(&client_id) {
            return Err(AppError::forbidden("")
                .with_internal(format!("client id '{client_id}' is not in the allowlist")));
        }

        Ok(AuthPrincipal {
            provider: "jwt-jwks",
            client_id,
            user_id: token_data.claims.sub.clone(),
            email: token_data.claims.email.clone(),
            permissions: token_data.claims.all_permissions(),
            token: token.to_string(),
        })
    }
}
