use axum::{
    Json,
    body::Bytes,
    extract::{FromRequest, Request},
    http::{StatusCode, header},
};
use serde::de::DeserializeOwned;
use validator::Validate;

use super::validation_rejection;
use crate::error::AppError;
use crate::web::format::is_msgpack_content_type;

#[derive(Debug, Clone, Copy, Default)]
pub struct ValidatedBody<T>(pub T);

impl<T, S> FromRequest<S> for ValidatedBody<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let (value, _) = extract_validated_body::<T, S>(req, state).await?;
        Ok(Self(value))
    }
}

/// Igual que [`ValidatedBody`], pero conserva los bytes exactos recibidos para archivado forense
/// o reenvío crudo (ej. respaldos de ingesta en Cloud Storage).
#[derive(Debug, Clone, Default)]
pub struct ValidatedBodyWithRaw<T>(pub T, pub Bytes);

impl<T, S> FromRequest<S> for ValidatedBodyWithRaw<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let (value, raw) = extract_validated_body::<T, S>(req, state).await?;
        Ok(Self(value, raw))
    }
}

async fn extract_validated_body<T, S>(req: Request, state: &S) -> Result<(T, Bytes), AppError>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    let is_msgpack = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(is_msgpack_content_type);

    if is_msgpack {
        let bytes = Bytes::from_request(req, state)
            .await
            .map_err(|e| body_rejection(e.status(), e.body_text()))?;
        let value: T = rmp_serde::from_slice(&bytes).map_err(msgpack_rejection)?;
        value.validate().map_err(|errs| validation_rejection("body", &errs))?;
        return Ok((value, bytes));
    }

    let is_json = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(is_json_content_type);

    if !is_json {
        return Err(body_rejection(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "expected Content-Type: application/json".to_string(),
        ));
    }

    let bytes = Bytes::from_request(req, state)
        .await
        .map_err(|e| body_rejection(e.status(), e.body_text()))?;
    let Json(value) =
        Json::<T>::from_bytes(&bytes).map_err(|e| body_rejection(e.status(), e.body_text()))?;
    value.validate().map_err(|errs| validation_rejection("body", &errs))?;
    Ok((value, bytes))
}

fn is_json_content_type(content_type: &str) -> bool {
    let media_type = content_type.split(';').next().unwrap_or_default().trim();
    media_type.eq_ignore_ascii_case("application/json")
        || media_type.split_once('+').is_some_and(|(_, suffix)| suffix.eq_ignore_ascii_case("json"))
}

fn msgpack_rejection(err: rmp_serde::decode::Error) -> AppError {
    use rmp_serde::decode::Error as DecodeErr;

    match &err {
        DecodeErr::InvalidMarkerRead(_)
        | DecodeErr::InvalidDataRead(_)
        | DecodeErr::DepthLimitExceeded
        | DecodeErr::Utf8Error(_) => AppError::invalid("body", "malformed MessagePack payload")
            .with_internal(format!("rmp_serde wire error: {err}")),
        _ => {
            let detail = err.to_string();
            AppError::censored(
                StatusCode::UNPROCESSABLE_ENTITY,
                "INVALID_INPUT",
                format!("Invalid body: {detail}"),
                detail,
            )
        }
    }
}

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
        _ => AppError::invalid("body", detail),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode, header};
    use axum::response::Response;
    use axum::routing::post;
    use serde::{Deserialize, Serialize};
    use tower::ServiceExt;
    use validator::Validate;

    use super::{ValidatedBody, ValidatedBodyWithRaw};
    use crate::response::ApiResponse;
    use crate::web::format::CONTENT_TYPE_MSGPACK;
    use crate::web::layers::msgpack_negotiation;
    use crate::{Cast, UtcDateTime};

    #[derive(Debug, Deserialize, Validate)]
    struct Probe {
        #[validate(length(min = 2, message = "name too short"))]
        name: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
    struct DirectEntity {
        id: Cast<String, i64>,
        #[validate(length(min = 2, message = "name too short"))]
        name: String,
        #[serde(default)]
        created_at: UtcDateTime,
    }

    #[derive(Debug, Deserialize)]
    struct Envelope<T> {
        data: Option<T>,
        code: Option<String>,
        cause: Option<String>,
        details: Option<BTreeMap<String, Vec<String>>>,
    }

    fn app() -> Router {
        Router::new()
            .route("/probe", post(|ValidatedBody(p): ValidatedBody<Probe>| async { p.name }))
            .route(
                "/raw",
                post(|ValidatedBodyWithRaw(p, raw): ValidatedBodyWithRaw<Probe>| async move {
                    format!("{}:{}", p.name, raw.len())
                }),
            )
            .route(
                "/entities",
                post(|ValidatedBody(entity): ValidatedBody<DirectEntity>| async move {
                    ApiResponse::with_status(StatusCode::CREATED, entity)
                }),
            )
            .layer(axum::middleware::from_fn(msgpack_negotiation))
    }

    async fn call(content_type: Option<&str>, body: &'static str) -> Response {
        let mut request = Request::post("/probe");
        if let Some(content_type) = content_type {
            request = request.header(header::CONTENT_TYPE, content_type);
        }
        let request = request.body(Body::from(body)).expect("request");
        app().oneshot(request).await.expect("infallible")
    }

    async fn error_envelope(response: Response) -> Envelope<()> {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.expect("readable body");
        serde_json::from_slice::<Envelope<()>>(&bytes).expect("envelope")
    }

    #[tokio::test]
    async fn validates_and_populates_structured_error_details() {
        let ok = call(Some("application/json"), r#"{"name":"Ana"}"#).await;
        assert_eq!(ok.status(), StatusCode::OK);

        let raw_res = app()
            .oneshot(
                Request::post("/raw")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"name":"Ana"}"#))
                    .expect("req"),
            )
            .await
            .expect("ok");
        assert_eq!(to_bytes(raw_res.into_body(), usize::MAX).await.expect("b").as_ref(), b"Ana:14");

        let invalid = call(Some("application/json"), r#"{"name":"A"}"#).await;
        assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
        let err = error_envelope(invalid).await;
        assert_eq!(err.code.as_deref(), Some("INVALID_INPUT"));
        assert!(err.cause.as_deref().is_some_and(|c| c.contains("failed validation")));
        assert_eq!(
            err.details.and_then(|mut d| d.remove("name")),
            Some(vec!["name too short".to_string()])
        );

        let wrong_type = call(Some("application/json"), r#"{"name":7}"#).await;
        assert_eq!(wrong_type.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(error_envelope(wrong_type).await.code.as_deref(), Some("INVALID_INPUT"));

        let no_ct = call(None, r#"{"name":"Ana"}"#).await;
        assert_eq!(no_ct.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        assert_eq!(error_envelope(no_ct).await.code.as_deref(), Some("UNSUPPORTED_MEDIA_TYPE"));
    }

    #[tokio::test]
    async fn msgpack_roundtrips_directly_into_validated_entity_and_mirrors_format() {
        #[derive(Serialize)]
        struct RawInput<'a> {
            id: &'a str,
            name: &'a str,
        }

        let payload =
            rmp_serde::to_vec_named(&RawInput { id: "1001", name: "Teclado" }).expect("encode");
        let req = Request::post("/entities")
            .header(header::CONTENT_TYPE, CONTENT_TYPE_MSGPACK)
            .body(Body::from(payload))
            .expect("req");

        let res = app().oneshot(req).await.expect("ok");
        assert_eq!(res.status(), StatusCode::CREATED);
        let bytes = to_bytes(res.into_body(), usize::MAX).await.expect("bytes");
        let decoded: Envelope<DirectEntity> = rmp_serde::from_slice(&bytes).expect("msgpack res");
        assert!(decoded.code.is_none() && decoded.cause.is_none());
        let entity = decoded.data.expect("entity data");
        assert_eq!((entity.id.as_str(), entity.name.as_str()), ("1001", "Teclado"));

        let short_name =
            rmp_serde::to_vec_named(&RawInput { id: "1001", name: "A" }).expect("encode");
        let res_400 = app()
            .oneshot(
                Request::post("/entities")
                    .header(header::CONTENT_TYPE, CONTENT_TYPE_MSGPACK)
                    .body(Body::from(short_name))
                    .expect("req"),
            )
            .await
            .expect("ok");
        assert_eq!(res_400.status(), StatusCode::BAD_REQUEST);
        let env_400: Envelope<DirectEntity> =
            rmp_serde::from_slice(&to_bytes(res_400.into_body(), usize::MAX).await.expect("b"))
                .expect("msgpack err");
        assert_eq!(env_400.code.as_deref(), Some("INVALID_INPUT"));
        assert!(env_400.cause.is_some());
        assert!(env_400.details.is_some_and(|d| d.contains_key("name")));
    }
}
