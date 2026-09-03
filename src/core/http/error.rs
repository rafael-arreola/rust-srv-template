use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use thiserror::Error;

use super::response::ApiResponse;

#[derive(Error, Debug)]
pub enum AppError {
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    Info,
    Warn,
    Error,
}

impl AppError {
    /// Returns a stable, machine-readable code for every error variant.
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
        }
    }

    /// Client-safe message: internal infrastructure details are redacted for clients.
    pub fn public_message(&self) -> String {
        match self {
            Self::NotFound { .. }
            | Self::AlreadyExists { .. }
            | Self::Invalid { .. }
            | Self::Required { .. }
            | Self::Unauthorized(_)
            | Self::Forbidden(_)
            | Self::BusinessRule(_) => self.to_string(),

            Self::ExternalService { service, .. } => {
                format!("The '{service}' service is currently unavailable. Please retry later.")
            }
            Self::Timeout { .. } => {
                "The request took too long to complete. Please retry.".to_string()
            }
            Self::Database(_) | Self::Internal(_) => {
                "An internal error occurred. Use the trace_id to report it.".to_string()
            }
        }
    }

    /// Log severity required for this error.
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
        }
    }

    pub fn not_found(entity: &'static str, id: impl Into<String>) -> Self {
        Self::NotFound { entity, id: id.into() }
    }

    pub fn duplicate(entity: &'static str, field: &'static str, value: impl Into<String>) -> Self {
        Self::AlreadyExists {
            entity,
            details: format!("{} '{}' already in use", field, value.into()),
        }
    }

    pub fn invalid_param(
        param: &'static str,
        entity: &'static str,
        value: impl Into<String>,
    ) -> Self {
        Self::Invalid { field: param, reason: format!("Invalid {} ID: {}", entity, value.into()) }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::Invalid { field: "body", reason: message.into() }
    }

    pub fn business_rule(message: impl Into<String>) -> Self {
        Self::BusinessRule(message.into())
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    pub fn database(message: impl Into<String>) -> Self {
        Self::Database(message.into())
    }

    pub fn external_service(service: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ExternalService { service: service.into(), message: message.into() }
    }

    pub fn timeout(operation: impl Into<String>) -> Self {
        Self::Timeout { operation: operation.into() }
    }
}

pub type AppResult<T> = Result<T, AppError>;

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let code = self.code();
        let internal = self.to_string();
        let message = self.public_message();
        let status = self.status_code();

        // Single logging choke point for all errors in HTTP responses
        match self.severity() {
            ErrorSeverity::Error => {
                tracing::error!(%code, %internal, status = status.as_u16(), "Request failed");
            }
            ErrorSeverity::Warn => {
                tracing::warn!(%code, %internal, status = status.as_u16(), "Request rejected");
            }
            ErrorSeverity::Info => {
                tracing::info!(%code, %internal, status = status.as_u16(), "Request rejected");
            }
        }

        ApiResponse::error(code, message, status).into_response()
    }
}
