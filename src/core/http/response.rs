use axum::{
    Json,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use opentelemetry::trace::TraceContextExt;
use serde::Serialize;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use super::extractors::msgpack::CONTENT_TYPE_MSGPACK;
pub use super::pagination::Paginated;

/// Codificación del cuerpo negociada para la petición en curso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyFormat {
    Json,
    MsgPack,
}

tokio::task_local! {
    /// Lo fija `middleware::msgpack_negotiation` alrededor del handler, de modo que
    /// [`ApiResponse`] serializa una sola vez y directamente en el formato pedido. Fuera de ese
    /// alcance (negociación deshabilitada) la respuesta es JSON.
    pub static RESPONSE_FORMAT: BodyFormat;
}

#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    pub message: String,

    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub breadcrumbs: Vec<String>,
}

/// Acuse de recibo de un worker o tarea asíncrona con un mensaje descriptivo.
#[derive(Debug, Serialize, serde::Deserialize, Clone, PartialEq, Eq)]
pub struct MessageOutput {
    pub message: String,
}

impl MessageOutput {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

/// Acuse de recibo booleano para tareas de worker que solo reportan éxito.
#[derive(Debug, Serialize, serde::Deserialize, Clone, PartialEq, Eq)]
pub struct TaskSuccessOutput {
    pub success: bool,
}

impl TaskSuccessOutput {
    pub fn ok() -> Self {
        Self { success: true }
    }
}

/// Standard HTTP envelope:
/// ```json
/// { "trace_id": "...", "data": { ... } }
/// { "trace_id": "...", "data": { "message": "..." }, "cause": "NOT_FOUND" }
/// ```
///
/// `trace_id` es el identificador W3C del span de la petición. Es `null` únicamente cuando la
/// respuesta se construye fuera de un span, lo cual es un defecto de cableado que se reporta con
/// un evento `error` en el log: jamás se fabrica un identificador para rellenar el campo.
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub trace_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<&'static str>,

    #[serde(skip)]
    pub status: StatusCode,
}

impl<T> IntoResponse for ApiResponse<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        let status = self.status;

        // 204 no lleva cuerpo: se responde el status desnudo en lugar de serializar un envelope
        // que hyper descartaría igualmente, dejando un `content-type` sin contenido.
        if status == StatusCode::NO_CONTENT {
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
        let context = tracing::Span::current().context();
        let span = context.span();
        let span_context = span.span_context();

        if span_context.is_valid() {
            return Some(format!("{:032x}", span_context.trace_id()));
        }

        tracing::error!(
            "ApiResponse built outside of a request span: trace_id is null (wiring defect)"
        );
        None
    }

    pub fn ok(data: T) -> Self {
        Self {
            trace_id: Self::current_trace_id(),
            data: Some(data),
            cause: None,
            status: StatusCode::OK,
        }
    }

    pub fn created(data: T) -> Self {
        Self {
            trace_id: Self::current_trace_id(),
            data: Some(data),
            cause: None,
            status: StatusCode::CREATED,
        }
    }
}

impl ApiResponse<()> {
    pub fn empty() -> Self {
        Self {
            trace_id: Self::current_trace_id(),
            data: None,
            cause: None,
            status: StatusCode::NO_CONTENT,
        }
    }
}

impl ApiResponse<ErrorDetail> {
    pub fn error_with_breadcrumbs(
        code: &'static str,
        message: String,
        breadcrumbs: Vec<String>,
        status: StatusCode,
    ) -> Self {
        Self {
            trace_id: Self::current_trace_id(),
            data: Some(ErrorDetail { message, breadcrumbs }),
            cause: Some(code),
            status,
        }
    }
}
