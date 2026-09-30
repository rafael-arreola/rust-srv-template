use axum::http::{Method, StatusCode, Uri};

use super::error::{AppError, ErrorKind, ErrorSeverity};

/// Handler global para peticiones que no coinciden con ninguna ruta registrada.
///
/// Sin este fallback Axum responde un 404 de texto plano, sin `trace_id` ni `cause`, que el
/// cliente no puede correlacionar ni interpretar. Solo se registra el path (nunca la query string)
/// para no volcar parámetros sensibles al log.
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
/// del `MethodRouter`. La cabecera `Allow` que exige RFC 9110 se sigue emitiendo
/// (`allow: POST,GET,HEAD`), porque la añade el propio `MethodRouter` antes de delegar.
pub async fn method_not_allowed(method: Method, uri: Uri) -> AppError {
    AppError::from(ErrorKind::Custom {
        code: "METHOD_NOT_ALLOWED",
        status: StatusCode::METHOD_NOT_ALLOWED,
        message: "The HTTP method is not supported for this resource.".to_string(),
        severity: ErrorSeverity::Warn,
    })
    .with_internal(format!("method {} not allowed on {}", method, uri.path()))
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode, header};
    use axum::response::Response;
    use axum::routing::get;
    use serde::Deserialize;
    use tower::ServiceExt;

    use super::{method_not_allowed, not_found};

    #[derive(Deserialize)]
    struct ErrorEnvelope {
        trace_id: Option<String>,
        data: ErrorData,
        cause: String,
    }

    #[derive(Deserialize)]
    struct ErrorData {
        message: String,
    }

    fn app() -> Router {
        Router::new()
            .route("/items", get(|| async { "ok" }))
            .fallback(not_found)
            .method_not_allowed_fallback(method_not_allowed)
    }

    async fn envelope(response: Response) -> ErrorEnvelope {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.expect("readable body");
        serde_json::from_slice(&bytes).expect("error envelope")
    }

    #[tokio::test]
    async fn unknown_route_answers_404_inside_the_envelope() {
        let request = Request::get("/missing?token=secret").body(Body::empty()).expect("request");
        let response = app().oneshot(request).await.expect("infallible");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = envelope(response).await;
        assert_eq!(body.cause, "ROUTE_NOT_FOUND");
        assert_eq!(body.data.message, "The requested resource was not found.");
        // Sin el layer de `trace_context` no hay span de petición: el envelope lo dice con `null`.
        assert_eq!(body.trace_id, None);
    }

    #[tokio::test]
    async fn wrong_method_answers_405_and_keeps_the_allow_header() {
        let request = Request::delete("/items").body(Body::empty()).expect("request");
        let response = app().oneshot(request).await.expect("infallible");

        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        let allow = response.headers().get(header::ALLOW).expect("Allow header").to_str();
        assert!(allow.expect("ascii header").contains("GET"));
        assert_eq!(envelope(response).await.cause, "METHOD_NOT_ALLOWED");
    }
}
