use axum::{
    Json,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;

use super::active_trace_id;
use crate::web::format::{BodyFormat, CONTENT_TYPE_MSGPACK, RESPONSE_FORMAT};

/// Convierte una primitiva tipada (`StatusCode`) o un código numérico HTTP (`u16`, ej. `201`, `202`, `204`)
/// en un [`StatusCode`] válido sin riesgo de `panic!`.
pub trait IntoStatus {
    fn into_status(self) -> StatusCode;
}

impl IntoStatus for StatusCode {
    #[inline]
    fn into_status(self) -> StatusCode {
        self
    }
}

impl IntoStatus for u16 {
    #[inline]
    fn into_status(self) -> StatusCode {
        StatusCode::from_u16(self).unwrap_or_else(|_| {
            tracing::warn!(raw_status = self, "Invalid HTTP status code; defaulting to 500");
            StatusCode::INTERNAL_SERVER_ERROR
        })
    }
}

/// Sobre uniforme gobernado por `engine`:
/// - Éxito: `{ "trace_id": "...", "data": ... }`
/// - Error simple: `{ "trace_id": "...", "code": "NOT_FOUND", "cause": "User not found..." }`
/// - Error con detalles: `{ "trace_id": "...", "code": "...", "cause": "...", "details": { ... } }`
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    #[serde(skip)]
    pub status: StatusCode,
}

impl<T: Serialize> IntoResponse for ApiResponse<T> {
    fn into_response(self) -> Response {
        let status = self.status;
        if matches!(status, StatusCode::NO_CONTENT | StatusCode::NOT_MODIFIED) {
            return status.into_response();
        }

        let format = RESPONSE_FORMAT.try_with(|format| *format).unwrap_or(BodyFormat::Json);
        if format == BodyFormat::MsgPack {
            match rmp_serde::encode::to_vec_named(&self) {
                Ok(bytes) => {
                    let content_type = HeaderValue::from_static(CONTENT_TYPE_MSGPACK);
                    return (status, [(header::CONTENT_TYPE, content_type)], bytes).into_response();
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to encode MessagePack response; answering JSON");
                }
            }
        }

        (status, Json(self)).into_response()
    }
}

impl<T> ApiResponse<T> {
    fn current_trace_id() -> Option<String> {
        if let Some(id) = active_trace_id() {
            return Some(id);
        }

        tracing::error!(
            "ApiResponse built outside of a request span: trace_id is null (wiring defect)"
        );
        None
    }

    pub fn with_status(status: impl IntoStatus, data: T) -> Self {
        Self {
            trace_id: Self::current_trace_id(),
            data: Some(data),
            code: None,
            cause: None,
            details: None,
            status: status.into_status(),
        }
    }

    #[inline]
    pub fn ok(data: T) -> Self {
        Self::with_status(StatusCode::OK, data)
    }
}

impl ApiResponse<()> {
    pub fn empty(status: impl IntoStatus) -> Self {
        Self {
            trace_id: Self::current_trace_id(),
            data: None,
            code: None,
            cause: None,
            details: None,
            status: status.into_status(),
        }
    }

    pub fn error(
        code: &'static str,
        cause: String,
        details: Option<serde_json::Value>,
        status: impl IntoStatus,
    ) -> Self {
        Self {
            trace_id: Self::current_trace_id(),
            data: None,
            code: Some(code),
            cause: Some(cause),
            details,
            status: status.into_status(),
        }
    }
}
