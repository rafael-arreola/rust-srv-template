use std::fmt;
use std::panic::Location;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

use crate::response::ApiResponse;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
struct AppErrorInner {
    status: StatusCode,
    code: &'static str,
    public_message: String,
    internal_detail: Option<String>,
    origin: String,
    details: Option<serde_json::Value>,
    breadcrumbs: Vec<String>,
    severity: ErrorSeverity,
    logged: bool,
}

/// Contenedor unificado de error gobernado por `StatusCode` con rastreo automático de saltos (`#[track_caller]`):
/// - `origin`: archivo y línea exactos donde nació el error.
/// - `breadcrumbs`: ruta completa de saltos (`archivo:línea -> contexto`) para consola/logs.
/// - `details`: datos estructurados opcionales enviados al cliente en `error.details`.
/// - `log_now()` / `trace_report()`: inspección e impresión inmediata en consola sin doble log.
#[derive(Debug, Clone)]
pub struct AppError(Box<AppErrorInner>);

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {} (at {})", self.0.code, self.0.public_message, self.0.origin)?;
        if let Some(ref detail) = self.0.internal_detail {
            write!(f, " | internal: {detail}")?;
        }
        if !self.0.breadcrumbs.is_empty() {
            write!(f, " | trace: [{}]", self.0.breadcrumbs.join(" -> "))?;
        }
        Ok(())
    }
}

impl std::error::Error for AppError {}

fn status_from_u16(raw: u16) -> StatusCode {
    StatusCode::from_u16(raw).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
}

fn default_code(status: StatusCode) -> &'static str {
    match status {
        StatusCode::BAD_REQUEST => "INVALID_INPUT",
        StatusCode::UNAUTHORIZED => "UNAUTHORIZED",
        StatusCode::FORBIDDEN => "FORBIDDEN",
        StatusCode::NOT_FOUND => "NOT_FOUND",
        StatusCode::METHOD_NOT_ALLOWED => "METHOD_NOT_ALLOWED",
        StatusCode::CONFLICT => "CONFLICT",
        StatusCode::PAYLOAD_TOO_LARGE => "PAYLOAD_TOO_LARGE",
        StatusCode::UNSUPPORTED_MEDIA_TYPE => "UNSUPPORTED_MEDIA_TYPE",
        StatusCode::UNPROCESSABLE_ENTITY => "BUSINESS_RULE_VIOLATION",
        StatusCode::TOO_MANY_REQUESTS => "TOO_MANY_REQUESTS",
        StatusCode::BAD_GATEWAY => "EXTERNAL_SERVICE_UNAVAILABLE",
        StatusCode::SERVICE_UNAVAILABLE => "SERVICE_UNAVAILABLE",
        StatusCode::GATEWAY_TIMEOUT => "TIMEOUT",
        _ if status.is_server_error() => "INTERNAL_ERROR",
        _ => "REQUEST_ERROR",
    }
}

fn default_severity(status: StatusCode) -> ErrorSeverity {
    if status.is_server_error() {
        ErrorSeverity::Error
    } else if matches!(
        status,
        StatusCode::UNAUTHORIZED
            | StatusCode::FORBIDDEN
            | StatusCode::UNPROCESSABLE_ENTITY
            | StatusCode::TOO_MANY_REQUESTS
    ) {
        ErrorSeverity::Warn
    } else {
        ErrorSeverity::Info
    }
}

impl AppError {
    #[track_caller]
    fn build(
        status: StatusCode,
        code: &'static str,
        public_message: String,
        internal_detail: Option<String>,
        loc: &'static Location<'static>,
    ) -> Self {
        Self(Box::new(AppErrorInner {
            status,
            code,
            public_message,
            internal_detail,
            origin: format!("{}:{}", loc.file(), loc.line()),
            details: None,
            breadcrumbs: Vec::new(),
            severity: default_severity(status),
            logged: false,
        }))
    }

    /// Crea un error desde cualquier `StatusCode` o entero HTTP (`409`, `429`, `503`...), deduciendo
    /// automáticamente el código estándar, severidad y censura de errores `5xx`.
    #[track_caller]
    pub fn status(code: impl TryInto<StatusCode>, message: impl Into<String>) -> Self {
        let status = code.try_into().unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let msg = message.into();
        let (public_message, internal_detail) = if status.is_server_error() {
            ("An internal error occurred. Use the trace_id to report it.".to_string(), Some(msg))
        } else {
            (msg, None)
        };
        Self::build(
            status,
            default_code(status),
            public_message,
            internal_detail,
            Location::caller(),
        )
    }

    pub fn code(&self) -> &'static str {
        self.0.code
    }

    pub fn origin(&self) -> &str {
        &self.0.origin
    }

    pub fn status_code(&self) -> StatusCode {
        self.0.status
    }

    pub fn severity(&self) -> ErrorSeverity {
        self.0.severity
    }

    pub fn public_message(&self) -> String {
        self.0.public_message.clone()
    }

    pub fn details(&self) -> Option<&serde_json::Value> {
        self.0.details.as_ref()
    }

    pub fn internal_detail(&self) -> String {
        let base = if let Some(ref detail) = self.0.internal_detail {
            format!("{} ({detail})", self.0.origin)
        } else {
            format!("{}: {}", self.0.origin, self.0.public_message)
        };

        if self.0.breadcrumbs.is_empty() {
            base
        } else {
            format!("{base} -> {}", self.0.breadcrumbs.join(" -> "))
        }
    }

    /// Sobrescribe el código semántico devuelto en `error.code` (ej. `"INSUFFICIENT_STOCK"`).
    pub fn with_code(mut self, code: &'static str) -> Self {
        self.0.code = code;
        self
    }

    /// Sobrescribe el mensaje público enviado al cliente en `error.cause`.
    pub fn with_public(mut self, message: impl Into<String>) -> Self {
        self.0.public_message = message.into();
        self
    }

    /// Adjunta un detalle técnico privado exclusivo para consola/logs internos.
    pub fn with_internal(mut self, detail: impl Into<String>) -> Self {
        self.0.internal_detail = Some(detail.into());
        self
    }

    /// Adjunta datos estructurados públicos en `error.details` (serializable tanto en JSON como en MessagePack).
    pub fn with_details<D: Serialize>(mut self, details: D) -> Self {
        match serde_json::to_value(details) {
            Ok(val) => self.0.details = Some(val),
            Err(e) => {
                tracing::warn!(error = %e, "Failed to serialize custom error details");
            }
        }
        self
    }

    /// Registra un salto con ubicación automática (`archivo:línea`) y contexto descriptivo.
    #[track_caller]
    pub fn trace(self, context: impl Into<String>) -> Self {
        self.record_hop(Location::caller(), Some(context.into()))
    }

    /// Registra únicamente el archivo y línea del salto actual sin requerir mensaje manual.
    #[track_caller]
    pub fn trace_here(self) -> Self {
        self.record_hop(Location::caller(), None)
    }

    fn record_hop(mut self, loc: &'static Location<'static>, note: Option<String>) -> Self {
        let step = match note {
            Some(msg) => format!("{}:{} ({msg})", loc.file(), loc.line()),
            None => format!("{}:{}", loc.file(), loc.line()),
        };
        self.0.breadcrumbs.push(step);
        self
    }

    pub fn breadcrumbs(&self) -> &[String] {
        &self.0.breadcrumbs
    }

    /// Devuelve la ruta forense completa desde el origen hasta el último salto registrado.
    pub fn full_trace(&self) -> Vec<String> {
        let mut trace = Vec::with_capacity(self.0.breadcrumbs.len().saturating_add(1));
        let root = match &self.0.internal_detail {
            Some(detail) => format!("{} -> {detail}", self.0.origin),
            None => format!("{} -> {}", self.0.origin, self.0.public_message),
        };
        trace.push(root);
        trace.extend(self.0.breadcrumbs.clone());
        trace
    }

    /// Formatea un reporte legible multilínea con todos los saltos del error (útil en consola/debug).
    pub fn trace_report(&self) -> String {
        let mut out = format!(
            "[{}] HTTP {} — {}\n",
            self.0.code,
            self.0.status.as_u16(),
            self.0.public_message
        );
        for (idx, step) in self.full_trace().iter().enumerate() {
            out.push_str(&format!("  {idx}. {step}\n"));
        }
        out
    }

    pub fn is_logged(&self) -> bool {
        self.0.logged
    }

    /// Emite inmediatamente el error y toda su ruta de saltos al logger/consola, marcándolo como
    /// registrado para no duplicarlo si después llega a la frontera HTTP.
    pub fn log_now(&mut self) {
        if self.0.logged {
            return;
        }

        let trace_json = serde_json::to_string(&self.full_trace()).unwrap_or_default();
        match self.0.severity {
            ErrorSeverity::Error => {
                tracing::error!(
                    error.code = %self.0.code,
                    error.message = %self.0.public_message,
                    error.origin = %self.0.origin,
                    error.trace = %trace_json,
                    http.status_code = self.0.status.as_u16(),
                    "HTTP request error"
                );
            }
            ErrorSeverity::Warn => {
                tracing::warn!(
                    error.code = %self.0.code,
                    error.message = %self.0.public_message,
                    error.origin = %self.0.origin,
                    error.trace = %trace_json,
                    http.status_code = self.0.status.as_u16(),
                    "HTTP request rejected"
                );
            }
            ErrorSeverity::Info => {
                tracing::info!(
                    error.code = %self.0.code,
                    error.message = %self.0.public_message,
                    error.origin = %self.0.origin,
                    error.trace = %trace_json,
                    http.status_code = self.0.status.as_u16(),
                    "HTTP request rejected"
                );
            }
        }

        self.0.logged = true;
    }

    #[track_caller]
    pub fn censored(
        status: impl TryInto<StatusCode>,
        code: &'static str,
        public_message: impl Into<String>,
        internal_detail: impl Into<String>,
    ) -> Self {
        let status = status.try_into().unwrap_or(status_from_u16(500));
        Self::build(
            status,
            code,
            public_message.into(),
            Some(internal_detail.into()),
            Location::caller(),
        )
    }

    #[track_caller]
    pub fn not_found(entity: &'static str, id: impl Into<String>) -> Self {
        Self::build(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            format!("{entity} not found: {}", id.into()),
            None,
            Location::caller(),
        )
    }

    #[track_caller]
    pub fn already_exists(entity: &'static str, details: impl Into<String>) -> Self {
        Self::build(
            StatusCode::CONFLICT,
            "ALREADY_EXISTS",
            format!("{entity} already exists: {}", details.into()),
            None,
            Location::caller(),
        )
    }

    #[track_caller]
    pub fn invalid(field: &'static str, reason: impl Into<String>) -> Self {
        Self::build(
            StatusCode::BAD_REQUEST,
            "INVALID_INPUT",
            format!("Invalid {field}: {}", reason.into()),
            None,
            Location::caller(),
        )
    }

    #[track_caller]
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::build(
            StatusCode::BAD_REQUEST,
            "INVALID_INPUT",
            message.into(),
            None,
            Location::caller(),
        )
    }

    #[track_caller]
    pub fn unauthorized(message: impl Into<String>) -> Self {
        let msg = message.into();
        let public = if msg.is_empty() {
            "Authentication is required to access this resource.".to_string()
        } else {
            msg
        };
        Self::build(StatusCode::UNAUTHORIZED, "UNAUTHORIZED", public, None, Location::caller())
    }

    #[track_caller]
    pub fn forbidden(message: impl Into<String>) -> Self {
        let msg = message.into();
        let public = if msg.is_empty() {
            "You do not have permission to perform this action.".to_string()
        } else {
            msg
        };
        Self::build(StatusCode::FORBIDDEN, "FORBIDDEN", public, None, Location::caller())
    }

    #[track_caller]
    pub fn business_rule(message: impl Into<String>) -> Self {
        Self::build(
            StatusCode::UNPROCESSABLE_ENTITY,
            "BUSINESS_RULE_VIOLATION",
            message.into(),
            None,
            Location::caller(),
        )
    }

    #[track_caller]
    pub fn internal(message: impl Into<String>) -> Self {
        Self::build(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            "An internal error occurred. Use the trace_id to report it.".to_string(),
            Some(message.into()),
            Location::caller(),
        )
    }

    #[track_caller]
    pub fn database(message: impl Into<String>) -> Self {
        Self::build(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            "An internal error occurred. Use the trace_id to report it.".to_string(),
            Some(message.into()),
            Location::caller(),
        )
    }

    #[track_caller]
    pub fn external_service(service: impl Into<String>, message: impl Into<String>) -> Self {
        let service = service.into();
        let detail = message.into();
        Self::build(
            StatusCode::BAD_GATEWAY,
            "EXTERNAL_SERVICE_UNAVAILABLE",
            format!("The '{service}' service is currently unavailable. Please retry later."),
            Some(format!("{service}: {detail}")),
            Location::caller(),
        )
    }

    #[track_caller]
    pub fn timeout(operation: impl Into<String>) -> Self {
        Self::build(
            StatusCode::GATEWAY_TIMEOUT,
            "TIMEOUT",
            "The request took too long to complete. Please retry.".to_string(),
            Some(operation.into()),
            Location::caller(),
        )
    }
}

pub type AppResult<T> = Result<T, AppError>;

/// Extensión para encadenar saltos de rastreo sobre cualquier `Result<T, AppError>`.
pub trait TraceExt<T> {
    #[track_caller]
    fn trace(self, context: impl Into<String>) -> Result<T, AppError>;

    #[track_caller]
    fn trace_here(self) -> Result<T, AppError>;
}

impl<T> TraceExt<T> for Result<T, AppError> {
    #[track_caller]
    fn trace(self, context: impl Into<String>) -> Result<T, AppError> {
        let loc = Location::caller();
        self.map_err(|err| err.record_hop(loc, Some(context.into())))
    }

    #[track_caller]
    fn trace_here(self) -> Result<T, AppError> {
        let loc = Location::caller();
        self.map_err(|err| err.record_hop(loc, None))
    }
}

impl IntoResponse for AppError {
    fn into_response(mut self) -> Response {
        let code = self.0.code;
        let status = self.0.status;
        let message = self.0.public_message.clone();
        let details = self.0.details.clone();

        if !self.is_logged() {
            self.log_now();
        }

        ApiResponse::error(code, message, details, status).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step_one() -> AppResult<()> {
        Err(AppError::not_found("Order", "ord_1"))
    }

    fn step_two() -> AppResult<()> {
        step_one().trace("verificando orden en checkout")
    }

    fn step_three() -> AppResult<()> {
        step_two().trace_here()
    }

    #[test]
    fn captures_origin_and_every_hop_with_file_and_line() {
        let err = step_three().expect_err("must fail");
        let trace = err.full_trace();
        assert_eq!(trace.len(), 3);
        assert!(trace.first().is_some_and(|s| s.contains("error.rs:") && s.contains("ord_1")));
        assert!(
            trace
                .get(1)
                .is_some_and(|s| s.contains("error.rs:") && s.contains("verificando orden"))
        );
        assert!(trace.get(2).is_some_and(|s| s.contains("error.rs:")));
        assert!(err.trace_report().contains("[NOT_FOUND] HTTP 404"));
        assert_eq!(AppError::status(429, "slow down").code(), "TOO_MANY_REQUESTS");
    }
}
