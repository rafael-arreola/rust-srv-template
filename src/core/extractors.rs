use axum::{
    Json,
    body::Bytes,
    extract::{FromRequest, Request},
    http::header,
};
use serde::de::DeserializeOwned;
use validator::Validate;

use crate::core::error::AppError;

/// Content-negotiating, validating body extractor.
/// Deserializes JSON or MessagePack according to `Content-Type` and runs `validator::Validate`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ValidatedBody<T>(pub T);

impl<T, S> FromRequest<S> for ValidatedBody<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let is_msgpack = req
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|ct| ct.contains("msgpack"));

        let value: T = if is_msgpack {
            let bytes = Bytes::from_request(req, state)
                .await
                .map_err(|e| AppError::bad_request(e.to_string()))?;

            rmp_serde::from_slice(&bytes)
                .map_err(|e| AppError::bad_request(format!("Invalid MessagePack body: {}", e)))?
        } else {
            let Json(value) = Json::<T>::from_request(req, state)
                .await
                .map_err(|e| AppError::bad_request(e.to_string()))?;
            value
        };

        value.validate().map_err(|e| AppError::bad_request(e.to_string()))?;

        Ok(ValidatedBody(value))
    }
}
