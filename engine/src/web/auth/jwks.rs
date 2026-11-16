use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk};
use reqwest::header;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};

use super::{AuthPrincipal, AuthProvider};
use crate::error::{AppError, AppResult};
use crate::net::{Http, HttpClient};

const CACHE_TTL: Duration = Duration::from_secs(15 * 60);
const REFRESH_COOLDOWN: Duration = Duration::from_secs(30);
const MAX_JWKS_BYTES: usize = 1024 * 1024;
const MAX_JWKS_KEYS: usize = 64;
const MAX_KID_BYTES: usize = 128;

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
    pub fn get_client_id(&self) -> Option<String> {
        [self.client_id.as_deref(), self.azp.as_deref(), self.sub.as_deref()]
            .into_iter()
            .flatten()
            .map(str::trim)
            .find(|value| !value.is_empty())
            .map(str::to_string)
    }

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

#[derive(Debug, Clone)]
pub struct JwtJwksConfig {
    pub provider_name: &'static str,
    pub jwks_url: String,
    /// Opcional: si viene vacío (ej. Hydra validado solo por JWKS), no exige `iss`.
    pub issuer: String,
    /// Opcional: si viene vacío, desactiva validación de `aud` para soportar tokens OAuth2 M2M sin audiencia fija.
    pub audiences: Vec<String>,
    pub allowed_client_ids: Vec<String>,
}

impl Default for JwtJwksConfig {
    fn default() -> Self {
        Self {
            provider_name: "jwt-jwks",
            jwks_url: String::new(),
            issuer: String::new(),
            audiences: Vec::new(),
            allowed_client_ids: Vec::new(),
        }
    }
}

struct KeyCache {
    keys: HashMap<String, DecodingKey>,
    fetched_at: Instant,
}

impl KeyCache {
    fn is_fresh(&self) -> bool {
        self.fetched_at.elapsed() < CACHE_TTL
    }
}

struct JwksState {
    provider_name: &'static str,
    jwks_url: String,
    http: Arc<HttpClient>,
    validation: Validation,
    cache: RwLock<Arc<KeyCache>>,
    refresh_guard: Mutex<Option<Instant>>,
    allowed_client_ids: Vec<String>,
}

/// Verificador JWT RS256 con caché JWKS y refresco single-flight sobre `HttpClient` + `aws-lc-rs`.
#[derive(Clone)]
pub struct JwtJwksAuthProvider(Arc<JwksState>);

impl JwtJwksAuthProvider {
    pub async fn new(config: JwtJwksConfig) -> AppResult<Self> {
        let http = match Http::try_get(Http::DEFAULT) {
            Some(client) => client,
            None => Arc::new(HttpClient::try_new()?),
        };
        Self::with_http(config, http).await
    }

    pub async fn with_http(config: JwtJwksConfig, http: Arc<HttpClient>) -> AppResult<Self> {
        let jwks_url = config.jwks_url.trim().to_string();
        if jwks_url.is_empty() {
            return Err(AppError::internal("JWT provider requires a non-empty jwks_url"));
        }

        let mut validation = Validation::new(Algorithm::RS256);
        let mut required_claims = vec!["exp"];
        validation.validate_exp = true;
        validation.validate_nbf = true;

        let issuer = config.issuer.trim();
        if !issuer.is_empty() {
            required_claims.push("iss");
            validation.set_issuer(&[issuer]);
        }

        let audiences: Vec<&str> = config
            .audiences
            .iter()
            .map(String::as_str)
            .map(str::trim)
            .filter(|a| !a.is_empty())
            .collect();
        if audiences.is_empty() {
            validation.validate_aud = false;
        } else {
            required_claims.push("aud");
            validation.set_audience(&audiences);
        }
        validation.set_required_spec_claims(&required_claims);

        let keys = fetch_jwks_keys(&http, &jwks_url).await?;
        let provider_name =
            if config.provider_name.trim().is_empty() { "jwt-jwks" } else { config.provider_name };

        Ok(Self(Arc::new(JwksState {
            provider_name,
            jwks_url,
            http,
            validation,
            cache: RwLock::new(Arc::new(KeyCache { keys, fetched_at: Instant::now() })),
            refresh_guard: Mutex::new(None),
            allowed_client_ids: config.allowed_client_ids,
        })))
    }

    async fn resolve_key(&self, kid: &str) -> AppResult<DecodingKey> {
        {
            let cached = self.0.cache.read().await;
            if cached.is_fresh()
                && let Some(key) = cached.keys.get(kid)
            {
                return Ok(key.clone());
            }
        }

        let mut last_refresh = self.0.refresh_guard.lock().await;
        {
            let cached = self.0.cache.read().await;
            if cached.is_fresh()
                && let Some(key) = cached.keys.get(kid)
            {
                return Ok(key.clone());
            }
            if last_refresh.is_some_and(|at| at.elapsed() < REFRESH_COOLDOWN) && cached.is_fresh() {
                return Err(AppError::unauthorized(
                    "The provided token is invalid or has expired.",
                )
                .with_internal(format!("unknown JWT kid '{kid}' during refresh cooldown")));
            }
        }

        let fresh_keys = fetch_jwks_keys(&self.0.http, &self.0.jwks_url).await?;
        let updated = Arc::new(KeyCache { keys: fresh_keys, fetched_at: Instant::now() });
        *self.0.cache.write().await = Arc::clone(&updated);
        *last_refresh = Some(Instant::now());

        updated.keys.get(kid).cloned().ok_or_else(|| {
            AppError::unauthorized("The provided token is invalid or has expired.")
                .with_internal(format!("JWT kid '{kid}' is absent from refreshed JWKS"))
        })
    }
}

#[async_trait]
impl AuthProvider for JwtJwksAuthProvider {
    fn name(&self) -> &'static str {
        self.0.provider_name
    }

    #[tracing::instrument(skip_all, fields(provider = self.0.provider_name))]
    async fn authenticate(&self, token: &str) -> AppResult<AuthPrincipal> {
        let header = decode_header(token).map_err(|e| {
            AppError::unauthorized("The provided token is invalid or has expired.")
                .with_internal(format!("malformed JWT header: {e}"))
        })?;

        if header.alg != Algorithm::RS256 || header.crit.is_some() {
            return Err(AppError::unauthorized("The provided token is invalid or has expired.")
                .with_internal("unsupported JWT algorithm or critical header"));
        }

        let kid = header.kid.filter(|k| is_valid_kid(k)).ok_or_else(|| {
            AppError::unauthorized("The provided token is invalid or has expired.")
                .with_internal("JWT header carries no valid kid")
        })?;

        let key = self.resolve_key(&kid).await?;
        let token_data = decode::<JwtJwksClaims>(token, &key, &self.0.validation).map_err(|e| {
            AppError::unauthorized("The provided token is invalid or has expired.")
                .with_internal(format!("JWT validation failed: {e}"))
        })?;

        let client_id = token_data.claims.get_client_id().ok_or_else(|| {
            AppError::unauthorized("The provided token is invalid or has expired.")
                .with_internal("JWT carries no client_id, azp or sub claim")
        })?;

        if !self.0.allowed_client_ids.is_empty() && !self.0.allowed_client_ids.contains(&client_id)
        {
            return Err(AppError::forbidden("")
                .with_internal(format!("client id '{client_id}' is not in the allowlist")));
        }

        Ok(AuthPrincipal {
            provider: self.0.provider_name,
            client_id,
            user_id: token_data.claims.sub.clone(),
            email: token_data.claims.email.clone(),
            permissions: token_data.claims.all_permissions(),
            token: token.to_string(),
        })
    }
}

fn is_valid_kid(kid: &str) -> bool {
    !kid.is_empty()
        && kid.len() <= MAX_KID_BYTES
        && kid.trim() == kid
        && !kid.chars().any(char::is_control)
}

#[tracing::instrument(skip_all, fields(dependency = "jwks"))]
async fn fetch_jwks_keys(
    http: &HttpClient,
    jwks_url: &str,
) -> AppResult<HashMap<String, DecodingKey>> {
    let mut response = http
        .get(jwks_url)
        .header(header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| AppError::external_service("JWKS", e.to_string()))?;

    if !response.status().is_success() {
        return Err(AppError::external_service(
            "JWKS",
            format!("HTTP {} from {jwks_url}", response.status()),
        ));
    }

    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| AppError::external_service("JWKS", format!("unreadable body: {e}")))?
    {
        if chunk.len() > MAX_JWKS_BYTES.saturating_sub(body.len()) {
            return Err(AppError::external_service("JWKS", "JWKS payload exceeds 1 MiB limit"));
        }
        body.extend_from_slice(&chunk);
    }

    let jwks: jwk::JwkSet = serde_json::from_slice(&body)
        .map_err(|e| AppError::external_service("JWKS", format!("malformed JwkSet: {e}")))?;

    if jwks.keys.is_empty() || jwks.keys.len() > MAX_JWKS_KEYS {
        return Err(AppError::external_service(
            "JWKS",
            format!("JWKS key count ({}) outside allowed bounds", jwks.keys.len()),
        ));
    }

    let mut keys = HashMap::new();
    for jwk in &jwks.keys {
        let Some(kid) = &jwk.common.key_id else {
            continue;
        };
        if !is_valid_kid(kid) || !matches!(jwk.algorithm, jwk::AlgorithmParameters::RSA(_)) {
            continue;
        }
        if jwk
            .common
            .public_key_use
            .as_ref()
            .is_some_and(|usage| *usage != jwk::PublicKeyUse::Signature)
        {
            continue;
        }
        if let Ok(key) = DecodingKey::from_jwk(jwk) {
            keys.insert(kid.clone(), key);
        }
    }

    if keys.is_empty() {
        return Err(AppError::external_service(
            "JWKS",
            "JWKS contains no usable RS256 verification key",
        ));
    }

    Ok(keys)
}
