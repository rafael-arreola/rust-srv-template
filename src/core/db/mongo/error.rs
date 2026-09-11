use axum::http::StatusCode;
use mongodb::error::{Error as MongoError, ErrorKind as MongoErrorKind, WriteFailure};

use crate::core::error::{AppError, ErrorKind, ErrorSeverity};

/// Código de servidor MongoDB para violación de índice único.
const DUPLICATE_KEY_CODE: i32 = 11000;

/// Conversión canónica de errores del driver MongoDB hacia `AppError`.
///
/// Habilita la propagación directa con `?` desde los repositorios sin repetir `map_err`.
/// Una violación de índice único se traduce a 409 `ALREADY_EXISTS` con mensaje público genérico;
/// cualquier otro fallo del driver se clasifica como error de base de datos (500). El detalle
/// técnico del driver (nombres de índice, valores duplicados, hosts) queda confinado al log interno.
impl From<MongoError> for AppError {
    fn from(err: MongoError) -> Self {
        if is_duplicate_key(&err) {
            return AppError::from(ErrorKind::Custom {
                code: "ALREADY_EXISTS",
                status: StatusCode::CONFLICT,
                message: "The resource already exists.".to_string(),
                severity: ErrorSeverity::Info,
            })
            .with_internal(err.to_string());
        }

        AppError::database(err.to_string())
    }
}

fn is_duplicate_key(err: &MongoError) -> bool {
    match &*err.kind {
        MongoErrorKind::Write(WriteFailure::WriteError(write_error)) => {
            write_error.code == DUPLICATE_KEY_CODE
        }
        MongoErrorKind::Command(command_error) => command_error.code == DUPLICATE_KEY_CODE,
        MongoErrorKind::InsertMany(insert_error) => insert_error
            .write_errors
            .as_ref()
            .is_some_and(|errors| errors.iter().any(|e| e.code == DUPLICATE_KEY_CODE)),
        MongoErrorKind::BulkWrite(bulk_error) => {
            bulk_error.write_errors.values().any(|e| e.code == DUPLICATE_KEY_CODE)
        }
        _ => false,
    }
}
