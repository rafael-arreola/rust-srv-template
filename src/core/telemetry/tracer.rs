use super::format::CloudLoggingFormat;
use crate::config;
use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_gcloud_trace::GcpCloudTraceExporterBuilder;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt};

pub struct TracerGuard {
    provider: opentelemetry_sdk::trace::SdkTracerProvider,
}

impl TracerGuard {
    pub fn shutdown(&self) {
        if let Err(e) = self.provider.shutdown() {
            eprintln!("Tracer provider shutdown failed: {}", e);
        }
    }
}

pub async fn init_tracing() -> anyhow::Result<TracerGuard> {
    let cfg = config::get();
    let service_name = &cfg.service_name;
    let base_level = &cfg.debug_level;
    let project_id = &cfg.project_id;
    let service_env = &cfg.service_env;
    let version = env!("CARGO_PKG_VERSION");

    global::set_text_map_propagator(TraceContextPropagator::new());

    let env_filter = EnvFilter::new(format!(
        "h2=warn,hyper=warn,tokio_util=warn,tower_http=warn,axum=warn,{}",
        base_level
    ));

    let guard = match build_gcp_tracer(service_name, project_id, service_env, version).await {
        Ok((tracer, provider)) => {
            let cloud_logging_layer = tracing_subscriber::fmt::layer()
                .event_format(CloudLoggingFormat::new(project_id.clone()))
                .fmt_fields(tracing_subscriber::fmt::format::JsonFields::new());

            let subscriber = tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_opentelemetry::layer().with_tracer(tracer))
                .with(cloud_logging_layer);

            tracing::subscriber::set_global_default(subscriber)
                .map_err(|e| anyhow::anyhow!("Setting default subscriber failed: {}", e))?;

            TracerGuard { provider }
        }
        Err(e) => {
            let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder().build();
            let tracer = provider.tracer(service_name.clone());
            global::set_tracer_provider(provider.clone());

            let subscriber = tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer())
                .with(tracing_opentelemetry::layer().with_tracer(tracer));

            tracing::subscriber::set_global_default(subscriber)
                .map_err(|e| anyhow::anyhow!("Setting default subscriber failed: {}", e))?;

            tracing::warn!(
                "GCP trace exporter unavailable ({}); falling back to local fmt logging",
                e
            );

            TracerGuard { provider }
        }
    };

    Ok(guard)
}

async fn build_gcp_tracer(
    service_name: &str,
    project_id: &str,
    service_env: &str,
    version: &str,
) -> anyhow::Result<(opentelemetry_sdk::trace::Tracer, opentelemetry_sdk::trace::SdkTracerProvider)>
{
    if project_id.is_empty() {
        return Err(anyhow::anyhow!("PROJECT_ID is empty, skipping GCP tracer"));
    }

    let builder = GcpCloudTraceExporterBuilder::for_default_project_id()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialize GCP exporter builder: {}", e))?
        .with_resource(
            opentelemetry_sdk::Resource::builder()
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
        .map_err(|e| anyhow::anyhow!("Failed to create tracer provider: {}", e))?;

    let tracer = builder
        .install(&provider)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create tracer: {}", e))?;

    global::set_tracer_provider(provider.clone());

    Ok((tracer, provider))
}
