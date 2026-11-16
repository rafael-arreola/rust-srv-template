pub mod envelope;
pub mod file;
#[macro_use]
pub mod reply;

use opentelemetry::trace::TraceContextExt;
use tracing_opentelemetry::OpenTelemetrySpanExt;

pub use self::envelope::{ApiResponse, IntoStatus};
pub use self::file::FileResponse;
pub use self::reply::{ReplyExt, Respond, Status};
pub use super::extractors::PaginationQuery;
pub use super::format::{BodyFormat, RESPONSE_FORMAT};
pub use super::layers::{msgpack_negotiation, request_timeout, trace_context};

pub(super) fn active_trace_id() -> Option<String> {
    let context = tracing::Span::current().context();
    let span = context.span();
    let span_context = span.span_context();
    span_context.is_valid().then(|| format!("{:032x}", span_context.trace_id()))
}
