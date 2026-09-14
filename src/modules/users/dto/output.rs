use chrono::{DateTime, Utc};
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
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// El mapper vive en la frontera de transporte: `entity.rs` permanece puro y sin conocimiento
/// de los contratos HTTP de salida.
impl From<User> for UserOutput {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            name: user.name,
            email: user.email,
            created_at: user.created_at,
            updated_at: user.updated_at,
        }
    }
}
