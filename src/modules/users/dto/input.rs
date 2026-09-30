use serde::Deserialize;
use validator::Validate;

/// Contrato de entrada para la creación de un nuevo usuario.
#[derive(Debug, Deserialize, Validate)]
pub struct CreateUserInput {
    #[validate(length(min = 2, max = 100, message = "Name must be between 2 and 100 characters"))]
    pub name: String,

    #[validate(email(message = "Invalid email format"))]
    pub email: String,
}

/// Contrato de entrada para la actualización total de un usuario existente.
///
/// `version` es la que el cliente leyó en `UserOutput`. Si otro escribió después, la versión ya
/// no coincide y la edición se rechaza con 409 en lugar de pisar en silencio ese cambio.
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateUserInput {
    #[validate(length(min = 2, max = 100, message = "Name must be between 2 and 100 characters"))]
    pub name: String,

    #[validate(email(message = "Invalid email format"))]
    pub email: String,

    pub version: u32,
}
