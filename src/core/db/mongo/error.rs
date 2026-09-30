use mongodb::error::{Error as MongoError, ErrorKind as MongoErrorKind, WriteFailure};

use crate::core::error::{AppError, ErrorKind};

/// Código de servidor MongoDB para violación de índice único.
const DUPLICATE_KEY_CODE: i32 = 11000;

/// Conversión canónica de errores del driver MongoDB hacia `AppError`.
///
/// Habilita la propagación directa con `?` desde los repositorios sin repetir `map_err`.
/// Una violación de índice único se traduce al `ErrorKind::AlreadyExists` canónico (409) con un
/// mensaje público genérico: en esta capa no se conoce la entidad ni el campo, y el detalle del
/// driver (nombre del índice, valor duplicado, hosts) queda confinado al log interno. Cualquier
/// otro fallo del driver se clasifica como error de base de datos (500).
impl From<MongoError> for AppError {
    fn from(err: MongoError) -> Self {
        if is_duplicate_key(&err) {
            return AppError::from(ErrorKind::AlreadyExists {
                entity: "Resource",
                details: "a unique field is already in use".to_string(),
            })
            // `err.to_string()` incluye la clave duplicada (`dup key: { email: "..." }`), que puede
            // ser un dato personal: al log solo llega que fue una violación de índice único.
            .with_internal("unique index violation (E11000)");
        }

        AppError::database(err.to_string())
    }
}

/// Violación de un índice único. Pública para el repositorio que necesita distinguir "ya
/// existe" de otro fallo y reaccionar (por ejemplo, releer el registro de una clave idempotente).
pub fn is_duplicate_key(err: &MongoError) -> bool {
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
