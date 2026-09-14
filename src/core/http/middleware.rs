use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderValue, Request, Response, header},
    response::IntoResponse,
};
use opentelemetry::Context;
use opentelemetry::propagation::Extractor;
use opentelemetry::trace::{
    FutureExt as OtelFutureExt, SpanContext, SpanId, TraceContextExt, TraceFlags, TraceId,
    TraceState,
};
use std::time::Duration;
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use super::error::AppError;
use super::extractors::msgpack::{CONTENT_TYPE_MSGPACK, wants_msgpack_response};
use super::response::NegotiablePayload;

/// Per-request trace context middleware.
/// Extracts W3C `traceparent` (or GCP `X-Cloud-Trace-Context`) and attaches it as the parent span.
pub async fn trace_context(req: Request<Body>, next: axum::middleware::Next) -> Response<Body> {
    let span = tracing::info_span!(
        "http.request",
        http.method = %req.method(),
        http.target = %req.uri().path(),
    );

    if let Some(remote_context) = extract_remote_context(req.headers())
        && let Err(e) = span.set_parent(remote_context)
    {
        tracing::debug!("Failed to attach remote trace context: {}", e);
    }

    let otel_context = span.context();
    next.run(req).with_context(otel_context).instrument(span).await
}

/// Enforces per-request timeout. On timeout, returns standard error envelope with `TIMEOUT`.
pub async fn request_timeout(
    State(budget): State<Duration>,
    req: Request<Body>,
    next: axum::middleware::Next,
) -> Response<Body> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    match tokio::time::timeout(budget, next.run(req)).await {
        Ok(response) => response,
        Err(_) => AppError::timeout(format!("{} {} exceeded {:?} timeout", method, path, budget))
            .into_response(),
    }
}

/// Content negotiation middleware for `Accept: application/vnd.msgpack`.
pub async fn msgpack_negotiation(
    req: Request<Body>,
    next: axum::middleware::Next,
) -> Response<Body> {
    let wants_msgpack = wants_msgpack_response(req.headers());

    let mut response = next.run(req).await;

    let payload = response.extensions_mut().remove::<NegotiablePayload>();

    if !wants_msgpack {
        return response;
    }

    let Some(payload) = payload else {
        return response;
    };

    match rmp_serde::encode::to_vec_named(&*payload.0) {
        Ok(msgpack_bytes) => {
            let headers = response.headers_mut();
            headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(CONTENT_TYPE_MSGPACK));
            headers.remove(header::CONTENT_LENGTH);
            *response.body_mut() = Body::from(msgpack_bytes);
            response
        }
        Err(e) => {
            tracing::warn!("Failed to encode MessagePack response: {}", e);
            response
        }
    }
}

struct HeaderExtractor<'a>(&'a HeaderMap);

impl Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|value| value.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|key| key.as_str()).collect()
    }
}

fn extract_remote_context(headers: &HeaderMap) -> Option<Context> {
    let context = opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&HeaderExtractor(headers))
    });

    if context.span().span_context().is_valid() {
        return Some(context);
    }

    parse_cloud_trace_context(headers)
}

fn parse_cloud_trace_context(headers: &HeaderMap) -> Option<Context> {
    let value = headers.get("x-cloud-trace-context")?.to_str().ok()?;

    let (trace_id_hex, rest) = value.split_once('/')?;
    let span_id_decimal = rest.split(';').next()?;

    let trace_id = TraceId::from_hex(trace_id_hex).ok()?;
    let span_id_number: u64 = span_id_decimal.parse().ok()?;
    if trace_id == TraceId::INVALID || span_id_number == 0 {
        return None;
    }

    let span_context = SpanContext::new(
        trace_id,
        SpanId::from_bytes(span_id_number.to_be_bytes()),
        TraceFlags::SAMPLED,
        true,
        TraceState::default(),
    );

    Some(Context::new().with_remote_span_context(span_context))
}
