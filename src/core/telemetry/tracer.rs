use super::format::CloudLoggingFormat;
use crate::config;
use crate::core::error::{AppError, AppResult};
use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_gcloud_trace::GcpCloudTraceExporterBuilder;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::{SdkTracerProvider, Tracer};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt};

pub struct TracerGuard {
    provider: SdkTracerProvider,
}

impl TracerGuard {
    pub fn shutdown(&self) {
        if let Err(e) = self.provider.shutdown() {
            eprintln!("Tracer provider shutdown failed: {}", e);
        }
    }
}

/// Instala el subscriber global de `tracing` con dos capas filtradas por separado:
///
/// - La capa OpenTelemetry recibe todos los spans de nivel `INFO` o superior, sin importar
///   `DEBUG_LEVEL`. Es lo que garantiza que el span `http.request` exista en cada petición y que
///   el `trace_id` del envelope sea siempre válido: si el filtro de logs gobernara también esta
///   capa, `DEBUG_LEVEL=warn` apagaría el span raíz y toda la correlación con él.
/// - La capa de logs (Cloud Logging JSON o `fmt` local) recibe `DEBUG_LEVEL`, que así controla
///   únicamente la verbosidad de los eventos, como su nombre indica.
///
/// El exportador a Cloud Trace es best-effort: sin `PROJECT_ID`, o si el exportador no puede
/// construirse, el proceso emite logs locales y no exporta trazas, pero sigue generando
/// identificadores válidos. Que el subscriber no pueda instalarse sí es fatal.
pub async fn init_tracing() -> AppResult<TracerGuard> {
    let cfg = config::get();
    let service_name = &cfg.service_name;
    let base_level = &cfg.debug_level;
    let project_id = &cfg.project_id;
    let service_env = &cfg.service_env;
    let version = env!("CARGO_PKG_VERSION");

    global::set_text_map_propagator(TraceContextPropagator::new());

    let noisy_crates = "h2=warn,hyper=warn,tokio_util=warn,tower_http=warn,axum=warn";
    let log_filter = EnvFilter::new(format!("{noisy_crates},{base_level}"));
    let trace_filter = LevelFilter::INFO;

    let guard = match build_gcp_tracer(service_name, project_id, service_env, version).await {
        Ok((tracer, provider)) => {
            let cloud_logging_layer = tracing_subscriber::fmt::layer()
                .event_format(CloudLoggingFormat::new(project_id.clone()))
                .fmt_fields(tracing_subscriber::fmt::format::JsonFields::new())
                .with_filter(log_filter);

            let subscriber = tracing_subscriber::registry()
                .with(tracing_opentelemetry::layer().with_tracer(tracer).with_filter(trace_filter))
                .with(cloud_logging_layer);

            tracing::subscriber::set_global_default(subscriber).map_err(|e| {
                AppError::internal(format!("Setting default subscriber failed: {e}"))
            })?;

            TracerGuard { provider }
        }
        Err(reason) => {
            let provider = SdkTracerProvider::builder().build();
            let tracer = provider.tracer(service_name.clone());
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

            TracerGuard { provider }
        }
    };

    Ok(guard)
}

/// Construye el exportador a Cloud Trace para el `PROJECT_ID` configurado. Se usa el mismo
/// identificador de proyecto que `CloudLoggingFormat` escribe en cada línea de log, de modo que
/// logs y trazas apunten siempre al mismo proyecto.
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
