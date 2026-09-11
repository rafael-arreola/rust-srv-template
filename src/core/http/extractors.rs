use axum::{
    Json,
    body::Bytes,
    extract::{FromRequest, FromRequestParts, Path, Query, Request},
    http::{header, request::Parts},
};
use serde::de::DeserializeOwned;
use validator::Validate;

use super::error::{AppError, ErrorKind};

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
            .is_some_and(|ct| ct.contains("msgpack"));

        let value: T = if is_msgpack {
            let bytes = Bytes::from_request(req, state)
                .await
                .map_err(|e| AppError::bad_request(e.to_string()))?;

            rmp_serde::from_slice(&bytes)
                .map_err(|e| AppError::bad_request(format!("Invalid MessagePack body: {}", e)))?
        } else {
            let Json(value) = Json::<T>::from_request(req, state)
                .await
                .map_err(|e| AppError::bad_request(e.to_string()))?;
            value
        };

        value.validate().map_err(|e| AppError::bad_request(e.to_string()))?;

        Ok(ValidatedBody(value))
    }
}

/// Query-string extractor que deserializa y ejecuta `validator::Validate`.
///
/// Sustituye a `Query<T>` en los handlers: el rechazo de Axum es texto plano sin `trace_id`, y
/// además las reglas `#[validate(...)]` declaradas en los DTO de query nunca llegaban a ejecutarse.
#[derive(Debug, Clone, Copy, Default)]
pub struct ValidatedQuery<T>(pub T);

impl<T, S> FromRequestParts<S> for ValidatedQuery<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Query(value) = Query::<T>::from_request_parts(parts, state)
            .await
            .map_err(|e| invalid_query(e.to_string()))?;

        value.validate().map_err(|e| invalid_query(e.to_string()))?;

        Ok(ValidatedQuery(value))
    }
}

/// Path extractor cuyo rechazo viaja en el envelope estándar.
///
/// Respeta la clasificación de Axum: `MissingPathParams`, `WrongNumberOfParameters` y
/// `UnsupportedType` son 5xx porque denotan una desalineación entre la ruta declarada y la firma
/// del handler (defecto del servidor), y su detalle se confina al log interno. Solo un fallo real
/// de deserialización del valor recibido se reporta como 400 al cliente.
#[derive(Debug, Clone, Copy, Default)]
pub struct PathParam<T>(pub T);

impl<T, S> FromRequestParts<S> for PathParam<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Path(value) = Path::<T>::from_request_parts(parts, state).await.map_err(|e| {
            if e.status().is_server_error() {
                AppError::internal(format!("path extraction failed: {e}"))
            } else {
                ErrorKind::Invalid { field: "path", reason: e.body_text() }.into()
            }
        })?;

        Ok(PathParam(value))
    }
}

/// Error de frontera para la query string. No reutiliza `AppError::bad_request`, que etiqueta el
/// campo como `body` y produciría mensajes engañosos en un fallo de query.
fn invalid_query(reason: String) -> AppError {
    ErrorKind::Invalid { field: "query", reason }.into()
}
