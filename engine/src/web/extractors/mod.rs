pub mod body;
#[cfg(feature = "multipart")]
pub mod multipart;
pub mod params;

use std::collections::BTreeMap;

use validator::{ValidationErrors, ValidationErrorsKind};

pub use self::body::{ValidatedBody, ValidatedBodyWithRaw};
#[cfg(feature = "multipart")]
pub use self::multipart::{FieldData, TryFromMultipart, ValidatedMultipart};
pub use self::params::{PaginationQuery, PathParam, ValidatedQuery};
pub use super::format::{CONTENT_TYPE_MSGPACK, is_msgpack_content_type, wants_msgpack_response};
use crate::error::AppError;

/// Convierte `ValidationErrors` (incluyendo structs y listas anidadas) en un mapa limpio
/// `{ "campo": ["mensaje"] }` dentro de `error.details`.
pub(super) fn validation_rejection(source: &'static str, errors: &ValidationErrors) -> AppError {
    let mut fields = BTreeMap::new();
    collect_validation_errors("", errors, &mut fields);

    AppError::invalid(source, "validation failed")
        .with_public(format!("Invalid {source}: one or more fields failed validation"))
        .with_details(fields)
}

fn collect_validation_errors(
    prefix: &str,
    errors: &ValidationErrors,
    out: &mut BTreeMap<String, Vec<String>>,
) {
    for (field, kind) in errors.errors() {
        let path = if prefix.is_empty() { field.to_string() } else { format!("{prefix}.{field}") };
        match kind {
            ValidationErrorsKind::Field(field_errs) => {
                let msgs = field_errs
                    .iter()
                    .map(|err| {
                        err.message
                            .as_ref()
                            .map(ToString::to_string)
                            .unwrap_or_else(|| format!("failed '{}' validation", err.code))
                    })
                    .collect();
                out.insert(path, msgs);
            }
            ValidationErrorsKind::Struct(nested) => {
                collect_validation_errors(&path, nested, out);
            }
            ValidationErrorsKind::List(list) => {
                for (index, nested) in list {
                    collect_validation_errors(&format!("{path}[{index}]"), nested, out);
                }
            }
        }
    }
}
