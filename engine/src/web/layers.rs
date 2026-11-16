use std::time::Duration;

use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::MethodRouter,
};
use opentelemetry::Context as OtelContext;
use opentelemetry::propagation::Extractor;
use opentelemetry::trace::{
    FutureExt as OtelFutureExt, SpanContext, SpanId, TraceContextExt, TraceFlags, TraceId,
    TraceState,
};
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use super::format::{RESPONSE_FORMAT, negotiate_format};
use crate::context::Context;
use crate::error::AppError;

pub async fn trace_context(req: Request<Body>, next: Next) -> axum::http::Response<Body> {
    let span = tracing::info_span!(
        "http.request",
        http.method = %req.method(),
        http.target = %req.uri().path(),
    );

    if let Some(remote_context) = extract_remote_context(req.headers())
        && let Err(e) = span.set_parent(remote_context)
    {
        tracing::debug!("Failed to attach remote trace context: {e}");
    }

    let otel_context = span.context();
    next.run(req).with_context(otel_context).instrument(span).await
}

pub async fn request_timeout(
    State(budget): State<Duration>,
    req: Request<Body>,
    next: Next,
) -> axum::http::Response<Body> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    match tokio::time::timeout(budget, next.run(req)).await {
        Ok(response) => response,
        Err(_) => AppError::timeout(format!("{method} {path} exceeded {budget:?} timeout"))
            .into_response(),
    }
}

pub async fn msgpack_negotiation(req: Request<Body>, next: Next) -> axum::http::Response<Body> {
    let format = negotiate_format(req.headers());

    let mut response = RESPONSE_FORMAT.scope(format, next.run(req)).await;
    response.headers_mut().append(header::VARY, HeaderValue::from_static("accept"));
    response
}

/// Middleware genérico que extrae una cabecera HTTP obligatoria, la parsea en `T` e inyecta el
/// valor tipado dentro del [`Context`](crate::context::Context) de la operación.
pub async fn require_header_context<T>(
    State(header_name): State<&'static str>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError>
where
    T: Clone + Send + Sync + 'static + for<'a> TryFrom<&'a str, Error = AppError>,
{
    let raw = req
        .headers()
        .get(header_name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppError::invalid(header_name, format!("missing required '{header_name}' header"))
        })?;

    let parsed = T::try_from(raw)?;
    Context::from_extensions(req.extensions_mut()).insert(parsed);
    Ok(next.run(req).await)
}

/// Extensión fluida para exigir e inyectar una cabecera tipada en `Context` sobre un router o ruta.
pub trait HeaderContextExt: Sized {
    fn require_header<T>(self, header_name: &'static str) -> Self
    where
        T: Clone + Send + Sync + 'static + for<'a> TryFrom<&'a str, Error = AppError>;
}

impl<S: Clone + Send + Sync + 'static> HeaderContextExt for Router<S> {
    fn require_header<T>(self, header_name: &'static str) -> Self
    where
        T: Clone + Send + Sync + 'static + for<'a> TryFrom<&'a str, Error = AppError>,
    {
        self.route_layer(middleware::from_fn_with_state(header_name, require_header_context::<T>))
    }
}

impl<S: Clone + Send + Sync + 'static> HeaderContextExt for MethodRouter<S> {
    fn require_header<T>(self, header_name: &'static str) -> Self
    where
        T: Clone + Send + Sync + 'static + for<'a> TryFrom<&'a str, Error = AppError>,
    {
        self.route_layer(middleware::from_fn_with_state(header_name, require_header_context::<T>))
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

fn extract_remote_context(headers: &HeaderMap) -> Option<OtelContext> {
    let context = opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&HeaderExtractor(headers))
    });

    if context.span().span_context().is_valid() {
        return Some(context);
    }

    parse_cloud_trace_context(headers)
}

fn parse_cloud_trace_context(headers: &HeaderMap) -> Option<OtelContext> {
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

    Some(OtelContext::new().with_remote_span_context(span_context))
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use tower::ServiceExt;

    use super::HeaderContextExt;
    use crate::context::Context;
    use crate::error::AppError;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct TestCountry(String);

    impl TryFrom<&str> for TestCountry {
        type Error = AppError;
        fn try_from(value: &str) -> Result<Self, Self::Error> {
            match value {
                "MX" | "CL" => Ok(Self(value.to_string())),
                _ => Err(AppError::invalid("x-country", "unsupported country")),
            }
        }
    }

    #[tokio::test]
    async fn require_header_injects_typed_value_into_context_and_rejects_missing() {
        let app = Router::new()
            .route(
                "/scoped",
                get(|ctx: Context| async move { ctx.require::<TestCountry>().expect("country").0 }),
            )
            .require_header::<TestCountry>("x-country");

        let ok = app
            .clone()
            .oneshot(
                Request::get("/scoped").header("x-country", "MX").body(Body::empty()).expect("req"),
            )
            .await
            .expect("res");
        assert_eq!(ok.status(), StatusCode::OK);
        assert_eq!(to_bytes(ok.into_body(), usize::MAX).await.expect("b").as_ref(), b"MX");

        let missing = app
            .oneshot(Request::get("/scoped").body(Body::empty()).expect("req"))
            .await
            .expect("res");
        assert_eq!(missing.status(), StatusCode::BAD_REQUEST);
    }
}
