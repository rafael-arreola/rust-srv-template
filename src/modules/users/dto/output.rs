use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::super::entity::User;
use crate::core::pagination::Paginated;

pub type PaginatedUsers = Paginated<UserOutput>;

/// Contrato de salida con la representación pública de un usuario.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserOutput {
    pub id: String,
    pub name: String,
    pub email: String,
    /// Versión a reenviar en `UpdateUserInput` (bloqueo optimista).
    pub version: u32,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

/// El mapper vive en la frontera de transporte: `entity.rs` permanece puro y sin conocimiento
/// de los contratos HTTP de salida.
impl From<User> for UserOutput {
    fn from(user: User) -> Self {
        Self {
            id: user.id().to_string(),
            name: user.name().to_string(),
            email: user.email().to_string(),
            version: user.version(),
            created_at: user.created_at(),
            updated_at: user.updated_at(),
        }
    }
}
