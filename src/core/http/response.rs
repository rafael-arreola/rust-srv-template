use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use opentelemetry::trace::TraceContextExt;
use serde::Serialize;
use std::sync::Arc;
use tracing_opentelemetry::OpenTelemetrySpanExt;

pub use super::pagination::Paginated;

/// Type-erased handle to the original response value, stored in the response
/// extensions so MessagePack negotiation can encode it once directly.
#[derive(Clone)]
pub struct NegotiablePayload(pub Arc<dyn erased_serde::Serialize + Send + Sync>);

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
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub trace_id: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<&'static str>,

    #[serde(skip)]
    pub status: StatusCode,
}

impl<T> IntoResponse for ApiResponse<T>
where
    T: Serialize + Send + Sync + 'static,
{
    fn into_response(self) -> Response {
        let status = self.status;
        let shared = Arc::new(self);

        let mut response = (status, Json(&*shared)).into_response();
        response.extensions_mut().insert(NegotiablePayload(shared));
        response
    }
}

impl<T> ApiResponse<T> {
    fn current_trace_id() -> String {
        let context = tracing::Span::current().context();
        let span = context.span();
        let span_context = span.span_context();

        if span_context.is_valid() {
            format!("{:032x}", span_context.trace_id())
        } else {
            "00000000000000000000000000000000".to_string()
        }
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
    pub fn error(code: &'static str, message: String, status: StatusCode) -> Self {
        Self {
            trace_id: Self::current_trace_id(),
            data: Some(ErrorDetail { message, breadcrumbs: Vec::new() }),
            cause: Some(code),
            status,
        }
    }

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

impl<T: Serialize> ApiResponse<Paginated<T>> {
    pub fn paginated(data: Vec<T>, total: u64, page: u64, limit: u64) -> Self {
        Self {
            trace_id: Self::current_trace_id(),
            data: Some(Paginated::new(data, total, page, limit)),
            cause: None,
            status: StatusCode::OK,
        }
    }
}
