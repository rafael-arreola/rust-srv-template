use axum::{
    body::{Body, Bytes},
    extract::{FromRequest, Request},
    http::{HeaderMap, HeaderValue, header},
    response::{IntoResponse, Response},
};
use serde::{Serialize, de::DeserializeOwned};
use std::ops::{Deref, DerefMut};

use crate::core::http::error::AppError;

/// MIME canónico para el transporte MessagePack del servicio.
pub const CONTENT_TYPE_MSGPACK: &str = "application/vnd.msgpack";

/// `true` si el `Content-Type` corresponde a algún dialecto MessagePack aceptado.
pub fn is_msgpack_content_type(content_type: &str) -> bool {
    let media_type = content_type.split(';').next().unwrap_or_default().trim();

    media_type.eq_ignore_ascii_case(CONTENT_TYPE_MSGPACK)
        || media_type.eq_ignore_ascii_case("application/msgpack")
        || media_type.eq_ignore_ascii_case("application/x-msgpack")
        || media_type
            .split_once('+')
            .is_some_and(|(_, suffix)| suffix.eq_ignore_ascii_case("msgpack"))
}

/// `true` si el header `Accept` solicita una respuesta MessagePack.
pub fn wants_msgpack_response(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|accept| accept.split(',').any(is_msgpack_content_type))
}

/// Extractor de transporte MessagePack directo.
///
/// Exige `Content-Type` MessagePack (no negocia con JSON) y decodifica el cuerpo con `rmp_serde`.
#[derive(Debug, Clone, Copy, Default)]
pub struct MsgPack<T>(pub T);

impl<T> Deref for MsgPack<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for MsgPack<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> From<T> for MsgPack<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

impl<T, S> FromRequest<S> for MsgPack<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let is_msgpack = req
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(is_msgpack_content_type);

        if !is_msgpack {
            return Err(AppError::invalid(
                "content-type",
                format!("expected Content-Type: {CONTENT_TYPE_MSGPACK}"),
            ));
        }

        let bytes = Bytes::from_request(req, state)
            .await
            .map_err(|e| AppError::invalid("body", e.to_string()))?;

        let value = rmp_serde::from_slice(&bytes).map_err(|e| {
            AppError::invalid("body", "malformed MessagePack payload")
                .attach_context(format!("rmp_serde decode error: {e}"))
        })?;

        Ok(MsgPack(value))
    }
}

impl<T> IntoResponse for MsgPack<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        match rmp_serde::encode::to_vec_named(&self.0) {
            Ok(bytes) => {
                let mut response = Response::new(Body::from(bytes));
                response
                    .headers_mut()
                    .insert(header::CONTENT_TYPE, HeaderValue::from_static(CONTENT_TYPE_MSGPACK));
                response
            }
            Err(e) => AppError::internal(format!("failed to encode MessagePack response: {e}"))
                .into_response(),
        }
    }
}
