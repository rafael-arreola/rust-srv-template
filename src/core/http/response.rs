use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use opentelemetry::trace::TraceContextExt;
use serde::Serialize;
use std::sync::Arc;
use tracing_opentelemetry::OpenTelemetrySpanExt;

pub use super::pagination::{GenericPagination, Paginated, PaginatedResponse};

/// Type-erased handle to the original response value, stored in the response
/// extensions so MessagePack negotiation can encode it once directly.
#[derive(Clone)]
pub struct NegotiablePayload(pub Arc<dyn erased_serde::Serialize + Send + Sync>);

#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    pub message: String,
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
            data: Some(ErrorDetail { message }),
            cause: Some(code),
            status,
        }
    }
}

impl<T: Serialize> ApiResponse<PaginatedResponse<T>> {
    pub fn paginated(data: Vec<T>, total: u64, page: u32, limit: u32) -> Self {
        Self {
            trace_id: Self::current_trace_id(),
            data: Some(PaginatedResponse::new(data, total, page, limit)),
            cause: None,
            status: StatusCode::OK,
        }
    }
}

/// Alias para soporte de código preexistente
pub type GenericApiResponse<T> = ApiResponse<T>;
