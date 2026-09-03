use std::fmt;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use thiserror::Error;

use super::response::ApiResponse;

/// Taxonomía semántica de errores del sistema.
#[derive(Error, Debug, Clone)]
pub enum ErrorKind {
    #[error("{entity} not found: {id}")]
    NotFound { entity: &'static str, id: String },

    #[error("{entity} already exists: {details}")]
    AlreadyExists { entity: &'static str, details: String },

    #[error("Invalid {field}: {reason}")]
    Invalid { field: &'static str, reason: String },

    #[error("{field} is required")]
    Required { field: &'static str },

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Business rule violated: {0}")]
    BusinessRule(String),

    #[error("External service error: {service} - {message}")]
    ExternalService { service: String, message: String },

    #[error("Operation timed out: {operation}")]
    Timeout { operation: String },

    #[error("Database error: {0}")]
    Database(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Custom [{code}]: {message}")]
    Custom { code: &'static str, status: StatusCode, message: String, severity: ErrorSeverity },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    Info,
    Warn,
    Error,
}

impl ErrorKind {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound { .. } => "NOT_FOUND",
            Self::AlreadyExists { .. } => "ALREADY_EXISTS",
            Self::Invalid { .. } => "INVALID_INPUT",
            Self::Required { .. } => "REQUIRED_FIELD",
            Self::Unauthorized(_) => "UNAUTHORIZED",
            Self::Forbidden(_) => "FORBIDDEN",
            Self::BusinessRule(_) => "BUSINESS_RULE_VIOLATION",
            Self::ExternalService { .. } => "EXTERNAL_SERVICE_UNAVAILABLE",
            Self::Timeout { .. } => "TIMEOUT",
            Self::Database(_) | Self::Internal(_) => "INTERNAL_ERROR",
            Self::Custom { code, .. } => code,
        }
    }

    pub fn default_public_message(&self) -> String {
        match self {
            Self::NotFound { entity, id } => format!("{entity} not found: {id}"),
            Self::AlreadyExists { entity, details } => {
                format!("{entity} already exists: {details}")
            }
            Self::Invalid { field, reason } => format!("Invalid {field}: {reason}"),
            Self::Required { field } => format!("{field} is required"),
            Self::Unauthorized(msg) => {
                if msg.is_empty() {
                    "Authentication is required to access this resource.".to_string()
                } else {
                    msg.clone()
                }
            }
            Self::Forbidden(msg) => {
                if msg.is_empty() {
                    "You do not have permission to perform this action.".to_string()
                } else {
                    msg.clone()
                }
            }
            Self::BusinessRule(msg) => msg.clone(),

            Self::ExternalService { service, .. } => {
                format!("The '{service}' service is currently unavailable. Please retry later.")
            }
            Self::Timeout { .. } => {
                "The request took too long to complete. Please retry.".to_string()
            }
            Self::Database(_) | Self::Internal(_) => {
                "An internal error occurred. Use the trace_id to report it.".to_string()
            }
            Self::Custom { message, .. } => message.clone(),
        }
    }

    pub fn severity(&self) -> ErrorSeverity {
        match self {
            Self::NotFound { .. }
            | Self::AlreadyExists { .. }
            | Self::Invalid { .. }
            | Self::Required { .. } => ErrorSeverity::Info,
            Self::Unauthorized(_) | Self::Forbidden(_) | Self::BusinessRule(_) => {
                ErrorSeverity::Warn
            }
            Self::ExternalService { .. }
            | Self::Timeout { .. }
            | Self::Database(_)
            | Self::Internal(_) => ErrorSeverity::Error,
            Self::Custom { severity, .. } => *severity,
        }
    }

    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound { .. } => StatusCode::NOT_FOUND,
            Self::AlreadyExists { .. } => StatusCode::CONFLICT,
            Self::Invalid { .. } | Self::Required { .. } => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::BusinessRule(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Timeout { .. } => StatusCode::GATEWAY_TIMEOUT,
            Self::ExternalService { .. } | Self::Database(_) | Self::Internal(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            Self::Custom { status, .. } => *status,
        }
    }
}

#[derive(Debug, Clone)]
struct AppErrorInner {
    kind: ErrorKind,
    internal_detail: Option<String>,
    public_message: Option<String>,
    breadcrumbs: Vec<String>,
    public_breadcrumbs: Vec<String>,
    logged: bool,
}

/// Contenedor unificado de error con soporte de doble vista:
/// - Mensaje público sanitizado y censurado para el cliente (`public_message`).
/// - Detalle técnico específico y privado para observabilidad interna (`internal_detail`).
/// - Acumulación progresiva de trazas internas para observabilidad (`attach_context`).
/// - Acumulación progresiva de trazas públicas para el cliente (`attach_public`).
/// - Emisión controlada de logs (automática en choke-point HTTP o anticipada vía `log_now`).
///
/// Optimizado con un Box interno (8 bytes) para cero sobrecarga de copia en `Result<T, AppError>`.
#[derive(Debug, Clone)]
pub struct AppError(Box<AppErrorInner>);

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.kind)?;
        if let Some(ref detail) = self.0.internal_detail {
            write!(f, " | internal: {detail}")?;
        }
        if !self.0.breadcrumbs.is_empty() {
            write!(f, " | context: [{}]", self.0.breadcrumbs.join(" -> "))?;
        }
        Ok(())
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0.kind)
    }
}

impl From<ErrorKind> for AppError {
    fn from(kind: ErrorKind) -> Self {
        Self(Box::new(AppErrorInner {
            kind,
            internal_detail: None,
            public_message: None,
            breadcrumbs: Vec::new(),
            public_breadcrumbs: Vec::new(),
            logged: false,
        }))
    }
}

impl AppError {
    /// Retorna la variante semántica interna del error.
    pub fn kind(&self) -> &ErrorKind {
        &self.0.kind
    }

    /// Retorna el código de error estable y estructurado de la API (ej. NOT_FOUND, INTERNAL_ERROR).
    pub fn code(&self) -> &'static str {
        self.0.kind.code()
    }

    /// Retorna el status HTTP correspondiente.
    pub fn status_code(&self) -> StatusCode {
        self.0.kind.status_code()
    }

    /// Retorna la severidad requerida para el registro del log.
    pub fn severity(&self) -> ErrorSeverity {
        self.0.kind.severity()
    }

    /// Mensaje seguro, natural y censurado destinado al cliente HTTP.
    /// Si se configuró un mensaje público personalizado vía `.with_public(...)`, se utiliza ese valor.
    /// De lo contrario, se emplea el mensaje por defecto libre de datos sensibles.
    pub fn public_message(&self) -> String {
        if let Some(ref custom) = self.0.public_message {
            custom.clone()
        } else {
            self.0.kind.default_public_message()
        }
    }

    /// Detalle técnico privado y específico para observabilidad interna y debugging.
    /// Si se especificó un detalle interno vía `.with_internal(...)`, se retorna dicho valor.
    /// De lo contrario, se retorna la representación textual de la causa base (`self.kind.to_string()`).
    /// Si se acumularon trazas de contexto (`.attach_context(...)`), se adjuntan en la salida.
    pub fn internal_detail(&self) -> String {
        let base = if let Some(ref detail) = self.0.internal_detail {
            detail.clone()
        } else {
            self.0.kind.to_string()
        };

        if self.0.breadcrumbs.is_empty() {
            base
        } else {
            format!("{base} | context: [{}]", self.0.breadcrumbs.join(" -> "))
        }
    }

    /// Adjunta un detalle técnico interno y privado que se registrará exclusivamente en los logs del servidor.
    /// NUNCA se expondrá al cliente en la respuesta HTTP.
    pub fn with_internal(mut self, detail: impl Into<String>) -> Self {
        self.0.internal_detail = Some(detail.into());
        self
    }

    /// Acumula una traza/migaja de contexto interno conforme el error asciende en el call-stack.
    /// Solo se registra en el log interno (error.breadcrumbs) y NUNCA se expone al cliente.
    pub fn attach_context(mut self, context: impl Into<String>) -> Self {
        self.0.breadcrumbs.push(context.into());
        self
    }

    /// Acumula una traza/migaja de contexto PÚBLICA que el cliente HTTP recibirá en su respuesta (`data.breadcrumbs`).
    /// Si es la primera migaja que se agrega, incluye automáticamente el mensaje público base como primer elemento.
    pub fn attach_public(mut self, message: impl Into<String>) -> Self {
        if self.0.public_breadcrumbs.is_empty() {
            self.0.public_breadcrumbs.push(self.public_message());
        }
        self.0.public_breadcrumbs.push(message.into());
        self
    }

    /// Retorna la lista de migajas de contexto públicas acumuladas para el cliente.
    pub fn public_breadcrumbs(&self) -> &[String] {
        &self.0.public_breadcrumbs
    }

    /// Sobrescribe el mensaje público para enviar una explicación censurada, amigable y natural al cliente.
    pub fn with_public(mut self, message: impl Into<String>) -> Self {
        self.0.public_message = Some(message.into());
        self
    }

    /// Retorna la lista de migajas de contexto acumuladas durante el proceso de propagación.
    pub fn breadcrumbs(&self) -> &[String] {
        &self.0.breadcrumbs
    }

    /// Retorna la traza forense completa como un vector secuencial:
    /// - Elemento 0: La causa raíz original (detalle técnico base).
    /// - Elementos 1..N: Cada contexto agregado durante el proceso de rebote ascendente.
    pub fn full_trace(&self) -> Vec<String> {
        let mut trace = Vec::with_capacity(1 + self.0.breadcrumbs.len());
        let base = if let Some(ref detail) = self.0.internal_detail {
            detail.clone()
        } else {
            self.0.kind.to_string()
        };
        trace.push(base);
        trace.extend(self.0.breadcrumbs.clone());
        trace
    }

    /// Retorna `true` si este error ya fue registrado en los logs del servidor.
    pub fn is_logged(&self) -> bool {
        self.0.logged
    }

    /// Emite el log estructurado inmediatamente en el sitio donde se invoca.
    /// Marca el error como `logged: true` para que el choke-point de salida HTTP (`into_response`)
    /// no duplique el evento de log.
    pub fn log_now(&mut self) {
        if self.0.logged {
            return;
        }

        let code = self.code();
        let status = self.status_code();
        let message = self.public_message();
        let breadcrumbs_json = serde_json::to_string(&self.full_trace()).unwrap_or_default();

        match self.severity() {
            ErrorSeverity::Error => {
                tracing::error!(
                    error.code = %code,
                    error.message = %message,
                    error.breadcrumbs = %breadcrumbs_json,
                    http.status_code = status.as_u16(),
                    "HTTP request error"
                );
            }
            ErrorSeverity::Warn => {
                tracing::warn!(
                    error.code = %code,
                    error.message = %message,
                    error.breadcrumbs = %breadcrumbs_json,
                    http.status_code = status.as_u16(),
                    "HTTP request rejected"
                );
            }
            ErrorSeverity::Info => {
                tracing::info!(
                    error.code = %code,
                    error.message = %message,
                    error.breadcrumbs = %breadcrumbs_json,
                    http.status_code = status.as_u16(),
                    "HTTP request rejected"
                );
            }
        }

        self.0.logged = true;
    }

    /// Constructor para errores con mensaje público natural y detalle técnico privado específico.
    pub fn censored(
        status: StatusCode,
        code: &'static str,
        public_message: impl Into<String>,
        internal_detail: impl Into<String>,
    ) -> Self {
        let severity = if status.is_server_error() {
            ErrorSeverity::Error
        } else if status.is_client_error() {
            ErrorSeverity::Warn
        } else {
            ErrorSeverity::Info
        };

        Self(Box::new(AppErrorInner {
            kind: ErrorKind::Custom { code, status, message: public_message.into(), severity },
            internal_detail: Some(internal_detail.into()),
            public_message: None,
            breadcrumbs: Vec::new(),
            public_breadcrumbs: Vec::new(),
            logged: false,
        }))
    }

    pub fn not_found(entity: &'static str, id: impl Into<String>) -> Self {
        ErrorKind::NotFound { entity, id: id.into() }.into()
    }

    pub fn duplicate(entity: &'static str, field: &'static str, value: impl Into<String>) -> Self {
        ErrorKind::AlreadyExists {
            entity,
            details: format!("{} '{}' already in use", field, value.into()),
        }
        .into()
    }

    pub fn invalid_param(
        param: &'static str,
        entity: &'static str,
        value: impl Into<String>,
    ) -> Self {
        ErrorKind::Invalid {
            field: param,
            reason: format!("Invalid {} ID: {}", entity, value.into()),
        }
        .into()
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        ErrorKind::Invalid { field: "body", reason: message.into() }.into()
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        ErrorKind::Unauthorized(message.into()).into()
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        ErrorKind::Forbidden(message.into()).into()
    }

    pub fn business_rule(message: impl Into<String>) -> Self {
        ErrorKind::BusinessRule(message.into()).into()
    }

    pub fn internal(message: impl Into<String>) -> Self {
        ErrorKind::Internal(message.into()).into()
    }

    pub fn database(message: impl Into<String>) -> Self {
        ErrorKind::Database(message.into()).into()
    }

    pub fn external_service(service: impl Into<String>, message: impl Into<String>) -> Self {
        ErrorKind::ExternalService { service: service.into(), message: message.into() }.into()
    }

    pub fn timeout(operation: impl Into<String>) -> Self {
        ErrorKind::Timeout { operation: operation.into() }.into()
    }
}

pub type AppResult<T> = Result<T, AppError>;

/// Extensión ergonómica para encadenar trazas de contexto directamente sobre `Result<T, AppError>`.
pub trait ContextExt<T> {
    /// Acumula una traza de contexto técnica privada (exclusiva para logs del servidor).
    fn attach_context(self, context: impl Into<String>) -> Result<T, AppError>;

    /// Acumula una traza de contexto pública y segura (enviada al cliente en `data.breadcrumbs`).
    fn attach_public(self, message: impl Into<String>) -> Result<T, AppError>;
}

impl<T> ContextExt<T> for Result<T, AppError> {
    fn attach_context(self, context: impl Into<String>) -> Result<T, AppError> {
        self.map_err(|err| err.attach_context(context))
    }

    fn attach_public(self, message: impl Into<String>) -> Result<T, AppError> {
        self.map_err(|err| err.attach_public(message))
    }
}

impl IntoResponse for AppError {
    fn into_response(mut self) -> Response {
        let code = self.code();
        let status = self.status_code();
        let message = self.public_message();
        let public_breadcrumbs = self.public_breadcrumbs().to_vec();

        // Choke point único de emisión de logs estructurados uniformes.
        // Si el desarrollador ya ejecutó `log_now()` anticipadamente, no se duplica.
        if !self.is_logged() {
            self.log_now();
        }

        ApiResponse::error_with_breadcrumbs(code, message, public_breadcrumbs, status)
            .into_response()
    }
}
