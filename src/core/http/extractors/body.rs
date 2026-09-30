use axum::{
    Json,
    body::Bytes,
    extract::{FromRequest, Request},
    http::{StatusCode, header},
};
use serde::de::DeserializeOwned;
use validator::Validate;

use super::msgpack::is_msgpack_content_type;
use crate::core::http::error::{AppError, ErrorKind};

/// Content-negotiating, validating body extractor.
/// Deserializes JSON or MessagePack according to `Content-Type` and runs `validator::Validate`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ValidatedBody<T>(pub T);

impl<T, S> FromRequest<S> for ValidatedBody<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let is_msgpack = req
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(is_msgpack_content_type);

        let value: T = if is_msgpack {
            let bytes = Bytes::from_request(req, state)
                .await
                .map_err(|e| body_rejection(e.status(), e.body_text()))?;

            rmp_serde::from_slice(&bytes).map_err(|e| {
                AppError::invalid("body", "malformed MessagePack payload")
                    .attach_context(format!("rmp_serde decode error: {e}"))
            })?
        } else {
            let Json(value) = Json::<T>::from_request(req, state)
                .await
                .map_err(|e| body_rejection(e.status(), e.body_text()))?;
            value
        };

        value.validate().map_err(|e| AppError::bad_request(e.to_string()))?;

        Ok(ValidatedBody(value))
    }
}

/// Conserva la clasificación HTTP del rechazo de Axum en lugar de aplanar todo a 400: un cuerpo
/// que excede el límite es 413, un `Content-Type` no soportado es 415 y un JSON sintácticamente
/// válido que no encaja en el contrato es 422. Un cliente que sube un archivo demasiado grande no
/// debe leer que su cuerpo está "malformado".
fn body_rejection(status: StatusCode, detail: String) -> AppError {
    match status {
        StatusCode::PAYLOAD_TOO_LARGE => AppError::censored(
            status,
            "PAYLOAD_TOO_LARGE",
            "The request body exceeds the maximum allowed size.",
            detail,
        ),
        StatusCode::UNSUPPORTED_MEDIA_TYPE => AppError::censored(
            status,
            "UNSUPPORTED_MEDIA_TYPE",
            "The request Content-Type is not supported.",
            detail,
        ),
        StatusCode::UNPROCESSABLE_ENTITY => {
            AppError::censored(status, "INVALID_INPUT", format!("Invalid body: {detail}"), detail)
        }
        status if status.is_server_error() => {
            AppError::internal(format!("body extraction failed: {detail}"))
        }
        _ => ErrorKind::Invalid { field: "body", reason: detail }.into(),
    }
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode, header};
    use axum::response::Response;
    use axum::routing::post;
    use serde::Deserialize;
    use tower::ServiceExt;
    use validator::Validate;

    use super::ValidatedBody;

    #[derive(Debug, Deserialize, Validate)]
    struct Probe {
        #[validate(length(min = 2, message = "name too short"))]
        name: String,
    }

    #[derive(Deserialize)]
    struct ErrorEnvelope {
        cause: String,
    }

    fn app() -> Router {
        Router::new()
            .route("/probe", post(|ValidatedBody(p): ValidatedBody<Probe>| async { p.name }))
    }

    async fn call(content_type: Option<&str>, body: &'static str) -> Response {
        let mut request = Request::post("/probe");
        if let Some(content_type) = content_type {
            request = request.header(header::CONTENT_TYPE, content_type);
        }
        let request = request.body(Body::from(body)).expect("request");
        app().oneshot(request).await.expect("infallible")
    }

    async fn cause(response: Response) -> String {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.expect("readable body");
        serde_json::from_slice::<ErrorEnvelope>(&bytes).expect("error envelope").cause
    }

    #[tokio::test]
    async fn valid_json_reaches_the_handler() {
        let response = call(Some("application/json"), r#"{"name":"Ana"}"#).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.expect("readable body");
        assert_eq!(bytes.as_ref(), b"Ana");
    }

    #[tokio::test]
    async fn validator_rules_run_and_answer_400() {
        let response = call(Some("application/json"), r#"{"name":"A"}"#).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(cause(response).await, "INVALID_INPUT");
    }

    #[tokio::test]
    async fn json_that_breaks_the_contract_answers_422() {
        let response = call(Some("application/json"), r#"{"name":7}"#).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(cause(response).await, "INVALID_INPUT");
    }

    #[tokio::test]
    async fn missing_content_type_answers_415() {
        let response = call(None, r#"{"name":"Ana"}"#).await;
        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        assert_eq!(cause(response).await, "UNSUPPORTED_MEDIA_TYPE");
    }
}
