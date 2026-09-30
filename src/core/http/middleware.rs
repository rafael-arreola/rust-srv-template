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
use super::extractors::msgpack::wants_msgpack_response;
use super::response::{BodyFormat, RESPONSE_FORMAT};

/// Per-request trace context middleware.
/// Extracts W3C `traceparent` (or the GCP load balancer's `X-Cloud-Trace-Context`) and attaches
/// it as the parent span.
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
///
/// Decides the body format before the handler runs and exposes it through
/// [`RESPONSE_FORMAT`], so `ApiResponse` encodes once, straight into the negotiated format,
/// instead of producing JSON that is then thrown away. Every response carries `Vary: Accept`, so
/// an intermediary cache never serves the MessagePack encoding to a JSON client or vice versa.
pub async fn msgpack_negotiation(
    req: Request<Body>,
    next: axum::middleware::Next,
) -> Response<Body> {
    let format =
        if wants_msgpack_response(req.headers()) { BodyFormat::MsgPack } else { BodyFormat::Json };

    let mut response = RESPONSE_FORMAT.scope(format, next.run(req)).await;
    response.headers_mut().append(header::VARY, HeaderValue::from_static("accept"));
    response
}

// Lifetime explícito: el `Extractor` de OpenTelemetry solo lee el
// `HeaderMap` durante la extracción y clonar todos los headers por petición no aporta nada.
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

/// `X-Cloud-Trace-Context: TRACE_ID/SPAN_ID;o=TRACE_TRUE`. La opción `o` es la decisión de
/// muestreo del balanceador: solo `o=1` marca el contexto como muestreado, de modo que el
/// servicio hereda esa decisión en lugar de exportar el 100% del tráfico que entra por el LB.
fn parse_cloud_trace_context(headers: &HeaderMap) -> Option<Context> {
    let value = headers.get("x-cloud-trace-context")?.to_str().ok()?;

    let (trace_id_hex, rest) = value.split_once('/')?;
    let mut options = rest.split(';');
    let span_id_decimal = options.next()?;
    let sampled = options.any(|option| option.trim() == "o=1");

    let trace_id = TraceId::from_hex(trace_id_hex).ok()?;
    let span_id_number: u64 = span_id_decimal.parse().ok()?;
    if trace_id == TraceId::INVALID || span_id_number == 0 {
        return None;
    }

    let flags = if sampled { TraceFlags::SAMPLED } else { TraceFlags::default() };

    let span_context = SpanContext::new(
        trace_id,
        SpanId::from_bytes(span_id_number.to_be_bytes()),
        flags,
        true,
        TraceState::default(),
    );

    Some(Context::new().with_remote_span_context(span_context))
}
