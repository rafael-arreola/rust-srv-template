use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

use opentelemetry::global;
use opentelemetry::trace::{TraceContextExt as _, TracerProvider as _};
use opentelemetry_gcloud_trace::GcpCloudTraceExporterBuilder;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::{SdkTracerProvider, Tracer};
use serde_json::{Map, Value, json};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields, FormattedFields};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt};

use crate::error::{AppError, AppResult};

/// Serializa una estructura de forma segura para eventos estructurados de tracing.
#[macro_export]
macro_rules! as_json {
    ($val:expr) => {
        &$crate::__serde_json::to_string($val).unwrap_or_else(|e| {
            let detail = $crate::__serde_json::to_string(&format!("Serialization failed: {e}"))
                .unwrap_or_else(|_| "\"Serialization failed\"".to_string());
            format!("{{\"error\":{detail}}}")
        })
    };
}

pub struct TracerGuard {
    provider: SdkTracerProvider,
    closed: AtomicBool,
}

impl TracerGuard {
    pub fn shutdown(&self) {
        if !self.closed.swap(true, Ordering::SeqCst)
            && let Err(e) = self.provider.shutdown()
        {
            eprintln!("Tracer provider shutdown failed: {e}");
        }
    }
}

impl Drop for TracerGuard {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Fachada fluida para configurar e iniciar telemetría (OpenTelemetry + Cloud Logging / fmt local).
pub struct Telemetry<'a> {
    service_name: &'a str,
    service_env: &'a str,
    project_id: &'a str,
    level: &'a str,
}

impl<'a> Telemetry<'a> {
    pub fn new(service_name: &'a str) -> Self {
        Self { service_name, service_env: "DEV", project_id: "", level: "info" }
    }

    #[must_use]
    pub fn env(mut self, service_env: &'a str) -> Self {
        self.service_env = service_env;
        self
    }

    #[must_use]
    pub fn project_id(mut self, project_id: &'a str) -> Self {
        self.project_id = project_id;
        self
    }

    #[must_use]
    pub fn level(mut self, level: &'a str) -> Self {
        self.level = level;
        self
    }

    pub async fn init(self) -> AppResult<TracerGuard> {
        init_tracing(self.service_name, self.service_env, self.project_id, self.level).await
    }
}

pub async fn init_tracing(
    service_name: &str,
    service_env: &str,
    project_id: &str,
    base_level: &str,
) -> AppResult<TracerGuard> {
    let version = env!("CARGO_PKG_VERSION");

    global::set_text_map_propagator(TraceContextPropagator::new());

    let noisy_crates = "h2=warn,hyper=warn,tokio_util=warn,tower_http=warn,axum=warn";
    let log_filter = EnvFilter::new(format!("{noisy_crates},{base_level}"));
    let trace_filter = LevelFilter::INFO;

    let guard = match build_gcp_tracer(service_name, project_id, service_env, version).await {
        Ok((tracer, provider)) => {
            let cloud_logging_layer = tracing_subscriber::fmt::layer()
                .event_format(CloudLoggingFormat::new(project_id))
                .fmt_fields(tracing_subscriber::fmt::format::JsonFields::new())
                .with_filter(log_filter);

            let subscriber = tracing_subscriber::registry()
                .with(tracing_opentelemetry::layer().with_tracer(tracer).with_filter(trace_filter))
                .with(cloud_logging_layer);

            tracing::subscriber::set_global_default(subscriber).map_err(|e| {
                AppError::internal(format!("Setting default subscriber failed: {e}"))
            })?;

            TracerGuard { provider, closed: AtomicBool::new(false) }
        }
        Err(reason) => {
            let provider = SdkTracerProvider::builder().build();
            let tracer = provider.tracer(service_name.to_string());
            global::set_tracer_provider(provider.clone());

            let subscriber = tracing_subscriber::registry()
                .with(tracing_opentelemetry::layer().with_tracer(tracer).with_filter(trace_filter))
                .with(tracing_subscriber::fmt::layer().with_filter(log_filter));

            tracing::subscriber::set_global_default(subscriber).map_err(|e| {
                AppError::internal(format!("Setting default subscriber failed: {e}"))
            })?;

            tracing::warn!(
                "GCP trace exporter unavailable ({}); falling back to local fmt logging",
                reason
            );

            TracerGuard { provider, closed: AtomicBool::new(false) }
        }
    };

    Ok(guard)
}

async fn build_gcp_tracer(
    service_name: &str,
    project_id: &str,
    service_env: &str,
    version: &str,
) -> Result<(Tracer, SdkTracerProvider), String> {
    if project_id.is_empty() {
        return Err("PROJECT_ID is empty, skipping GCP tracer".to_string());
    }

    let builder = GcpCloudTraceExporterBuilder::new(project_id.to_string()).with_resource(
        Resource::builder()
            .with_attributes(vec![
                opentelemetry::KeyValue::new("service.name", service_name.to_string()),
                opentelemetry::KeyValue::new("service.version", version.to_string()),
                opentelemetry::KeyValue::new("deployment.environment", service_env.to_string()),
                opentelemetry::KeyValue::new("project.id", project_id.to_string()),
            ])
            .build(),
    );

    let provider = builder
        .create_provider()
        .await
        .map_err(|e| format!("Failed to create tracer provider: {e}"))?;

    let tracer =
        builder.install(&provider).await.map_err(|e| format!("Failed to create tracer: {e}"))?;

    global::set_tracer_provider(provider.clone());
    Ok((tracer, provider))
}

pub struct CloudLoggingFormat {
    project_id: String,
}

impl CloudLoggingFormat {
    pub fn new(project_id: impl Into<String>) -> Self {
        Self { project_id: project_id.into() }
    }
}

impl<S, N> FormatEvent<S, N> for CloudLoggingFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        context: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let metadata = event.metadata();
        let mut entry = Map::new();

        if let Some(scope) = context.event_scope() {
            for span in scope.from_root() {
                if let Some(formatted) = span.extensions().get::<FormattedFields<N>>()
                    && let Ok(Value::Object(fields)) = serde_json::from_str(formatted)
                {
                    entry.extend(fields);
                }
            }
        }

        event.record(&mut JsonVisitor(&mut entry));

        entry.insert(
            "time".to_string(),
            Value::String(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)),
        );
        entry.insert("severity".to_string(), Value::String(severity(metadata.level()).into()));
        entry.insert("target".to_string(), Value::String(metadata.target().to_string()));

        if let (Some(file), Some(line)) = (metadata.file(), metadata.line()) {
            entry.insert(
                "logging.googleapis.com/sourceLocation".to_string(),
                json!({ "file": file, "line": line.to_string() }),
            );
        }

        let otel_context = opentelemetry::Context::current();
        let otel_span = otel_context.span();
        let span_context = otel_span.span_context();

        if span_context.is_valid() {
            entry.insert(
                "logging.googleapis.com/trace".to_string(),
                Value::String(format!(
                    "projects/{}/traces/{}",
                    self.project_id,
                    span_context.trace_id()
                )),
            );
            entry.insert(
                "logging.googleapis.com/trace_sampled".to_string(),
                span_context.is_sampled().into(),
            );
            entry.insert(
                "logging.googleapis.com/spanId".to_string(),
                Value::String(span_context.span_id().to_string()),
            );
        }

        let line = serde_json::to_string(&Value::Object(entry)).map_err(|_| fmt::Error)?;
        writeln!(writer, "{line}")
    }
}

fn severity(level: &Level) -> &'static str {
    match *level {
        Level::TRACE | Level::DEBUG => "DEBUG",
        Level::INFO => "INFO",
        Level::WARN => "WARNING",
        Level::ERROR => "ERROR",
    }
}

struct JsonVisitor<'a>(&'a mut Map<String, Value>);

impl tracing::field::Visit for JsonVisitor<'_> {
    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.0.insert(field.name().to_string(), value.into());
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.0.insert(field.name().to_string(), value.into());
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.0.insert(field.name().to_string(), value.into());
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.0.insert(field.name().to_string(), value.into());
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        let parsed = if (value.starts_with('{') && value.ends_with('}'))
            || (value.starts_with('[') && value.ends_with(']'))
        {
            serde_json::from_str::<Value>(value).ok()
        } else {
            None
        };

        if let Some(json_value) = parsed {
            self.0.insert(field.name().to_string(), json_value);
            return;
        }

        self.0.insert(field.name().to_string(), value.into());
    }

    fn record_error(
        &mut self,
        field: &tracing::field::Field,
        value: &(dyn std::error::Error + 'static),
    ) {
        self.0.insert(field.name().to_string(), value.to_string().into());
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        self.0.insert(field.name().to_string(), format!("{value:?}").into());
    }
}

#[cfg(test)]
mod tests {
    use serde::ser::Error as _;
    use serde::{Deserialize, Serialize, Serializer};

    struct Unserializable;

    impl Serialize for Unserializable {
        fn serialize<S: Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
            Err(S::Error::custom("bad \"quoted\" value"))
        }
    }

    #[derive(Deserialize)]
    struct Fallback {
        error: String,
    }

    #[test]
    fn serializes_a_valid_value_and_escapes_fallback_errors() {
        assert_eq!(crate::as_json!(&vec![1, 2]), "[1,2]");
        let out = crate::as_json!(&Unserializable);
        let parsed: Fallback = serde_json::from_str(out).expect("fallback must be valid JSON");
        assert!(parsed.error.contains("bad \"quoted\" value"));
    }
}
