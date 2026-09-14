use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use serde::de::DeserializeOwned;

use crate::core::http::error::{AppError, ErrorKind};

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
