use axum::extract::{FromRequestParts, Query};
use axum::http::request::Parts;
use serde::de::DeserializeOwned;
use validator::Validate;

use crate::core::http::error::{AppError, ErrorKind};

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

/// Error de frontera para la query string. No reutiliza `AppError::bad_request`, que etiqueta el
/// campo como `body` y produciría mensajes engañosos en un fallo de query.
fn invalid_query(reason: String) -> AppError {
    ErrorKind::Invalid { field: "query", reason }.into()
}
