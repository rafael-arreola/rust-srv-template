use axum::http::{Method, StatusCode, Uri};

use super::error::{AppError, ErrorKind, ErrorSeverity};

/// Handler global para peticiones que no coinciden con ninguna ruta registrada.
///
/// Sin este fallback Axum responde un 404 de texto plano, sin `trace_id` ni `cause`, rompiendo el
/// contrato de envelope uniforme. Solo se registra el path (nunca la query string) para no volcar
/// parámetros sensibles al log.
pub async fn not_found(method: Method, uri: Uri) -> AppError {
    AppError::from(ErrorKind::Custom {
        code: "ROUTE_NOT_FOUND",
        status: StatusCode::NOT_FOUND,
        message: "The requested resource was not found.".to_string(),
        severity: ErrorSeverity::Info,
    })
    .with_internal(format!("no route matches {} {}", method, uri.path()))
}

/// Handler global para una ruta existente invocada con un método HTTP no soportado.
///
/// Se registra vía `Router::method_not_allowed_fallback`, que lo instala como `default_fallback`
/// del `MethodRouter`. Verificado empíricamente: la cabecera `Allow` que exige RFC 9110 se sigue
/// emitiendo (`allow: POST,GET,HEAD`), porque la añade el propio `MethodRouter` antes de delegar.
pub async fn method_not_allowed(method: Method, uri: Uri) -> AppError {
    AppError::from(ErrorKind::Custom {
        code: "METHOD_NOT_ALLOWED",
        status: StatusCode::METHOD_NOT_ALLOWED,
        message: "The HTTP method is not supported for this resource.".to_string(),
        severity: ErrorSeverity::Warn,
    })
    .with_internal(format!("method {} not allowed on {}", method, uri.path()))
}
