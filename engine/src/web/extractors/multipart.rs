use axum::{
    extract::{FromRequest, Request},
    http::StatusCode,
};
pub use axum_typed_multipart::{FieldData, TryFromMultipart, TryFromMultipartWithState};
use axum_typed_multipart::{TypedMultipart, TypedMultipartError};
use validator::Validate;

use super::validation_rejection;
use crate::error::AppError;

/// Extractor multipart validado con `axum_typed_multipart` y `validator::Validate`.
///
/// Traduce errores de parseo, campos faltantes y límites de tamaño al sobre uniforme `AppError`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ValidatedMultipart<T>(pub T);

impl<T, S> FromRequest<S> for ValidatedMultipart<T>
where
    T: TryFromMultipartWithState<S> + Validate,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let TypedMultipart(value) =
            TypedMultipart::<T>::from_request(req, state).await.map_err(multipart_rejection)?;

        value.validate().map_err(|errs| validation_rejection("body", &errs))?;

        Ok(Self(value))
    }
}

fn multipart_rejection(error: TypedMultipartError) -> AppError {
    match error {
        TypedMultipartError::InvalidRequest { .. } => {
            AppError::invalid("body", "malformed multipart/form-data request or invalid boundary")
        }
        TypedMultipartError::InvalidRequestBody { source }
            if source.status() == StatusCode::PAYLOAD_TOO_LARGE =>
        {
            AppError::censored(
                StatusCode::PAYLOAD_TOO_LARGE,
                "PAYLOAD_TOO_LARGE",
                "The uploaded multipart payload exceeds the maximum allowed size.",
                source.body_text(),
            )
        }
        TypedMultipartError::MissingField { field_name } => {
            AppError::invalid("body", format!("field '{field_name}' is required"))
        }
        TypedMultipartError::WrongFieldType { field_name, wanted_type, .. } => AppError::invalid(
            "body",
            format!("field '{field_name}' must be of type '{wanted_type}'"),
        ),
        TypedMultipartError::DuplicateField { field_name } => {
            AppError::invalid("body", format!("field '{field_name}' is already present"))
        }
        TypedMultipartError::FieldTooLarge { field_name, limit_bytes } => AppError::censored(
            StatusCode::PAYLOAD_TOO_LARGE,
            "PAYLOAD_TOO_LARGE",
            format!("Field '{field_name}' exceeds the maximum size of {limit_bytes} bytes."),
            format!("multipart field '{field_name}' exceeded {limit_bytes} bytes"),
        ),
        TypedMultipartError::NamelessField => {
            AppError::invalid("body", "field name cannot be empty")
        }
        other => AppError::invalid("body", "malformed multipart request")
            .with_internal(format!("multipart decode failure: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::{
        Router,
        body::{Body, Bytes, to_bytes},
        http::{Request, StatusCode, header},
        routing::post,
    };
    use serde::Deserialize;
    use tower::ServiceExt;
    use validator::Validate;

    use super::{FieldData, TryFromMultipart, ValidatedMultipart};

    #[derive(TryFromMultipart, Validate)]
    struct UploadForm {
        #[form_data(limit = "1MiB")]
        file: FieldData<Bytes>,
        #[validate(length(min = 2, message = "name too short"))]
        name: String,
    }

    #[derive(Debug, Deserialize)]
    struct ErrorEnvelope {
        code: String,
        details: Option<BTreeMap<String, Vec<String>>>,
    }

    async fn upload_handler(ValidatedMultipart(form): ValidatedMultipart<UploadForm>) -> String {
        format!(
            "{}:{}:{}",
            form.name,
            form.file.contents.len(),
            form.file.metadata.file_name.unwrap_or_default()
        )
    }

    fn app() -> Router {
        Router::new().route("/upload", post(upload_handler))
    }

    fn multipart_req(boundary: &str, body: &str) -> Request<Body> {
        Request::post("/upload")
            .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
            .body(Body::from(body.to_string()))
            .expect("request")
    }

    #[tokio::test]
    async fn validates_multipart_uploads_and_reports_structured_errors() {
        let b = "BOUNDARY123";
        let valid_body = format!(
            "--{b}\r\nContent-Disposition: form-data; name=\"name\"\r\n\r\nAna\r\n\
             --{b}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"doc.txt\"\r\nContent-Type: text/plain\r\n\r\nhola\r\n\
             --{b}--\r\n"
        );
        let ok = app().oneshot(multipart_req(b, &valid_body)).await.expect("res");
        assert_eq!(ok.status(), StatusCode::OK);
        assert_eq!(
            to_bytes(ok.into_body(), usize::MAX).await.expect("b").as_ref(),
            b"Ana:4:doc.txt"
        );

        let short_body = format!(
            "--{b}\r\nContent-Disposition: form-data; name=\"name\"\r\n\r\nA\r\n\
             --{b}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"doc.txt\"\r\n\r\nhola\r\n\
             --{b}--\r\n"
        );
        let invalid = app().oneshot(multipart_req(b, &short_body)).await.expect("res");
        assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
        let err: ErrorEnvelope =
            serde_json::from_slice(&to_bytes(invalid.into_body(), usize::MAX).await.expect("b"))
                .expect("json");
        assert_eq!(err.code, "INVALID_INPUT");
        assert!(err.details.is_some_and(|d| d.contains_key("name")));

        let missing_body = format!(
            "--{b}\r\nContent-Disposition: form-data; name=\"name\"\r\n\r\nAna\r\n--{b}--\r\n"
        );
        let missing = app().oneshot(multipart_req(b, &missing_body)).await.expect("res");
        assert_eq!(missing.status(), StatusCode::BAD_REQUEST);
        let err_missing: ErrorEnvelope =
            serde_json::from_slice(&to_bytes(missing.into_body(), usize::MAX).await.expect("b"))
                .expect("json");
        assert_eq!(err_missing.code, "INVALID_INPUT");
    }
}
