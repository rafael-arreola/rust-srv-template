use chrono::{DateTime, Utc};

use super::dto::UserOutput;

/// Entidad de dominio pura `User`.
/// Desacoplada de MongoDB, BSON y serde de infraestructura.
/// Encapsula datos de negocio, invariantes y mutaciones válidas.
#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: String,
    pub name: String,
    pub email: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    pub fn new(name: String, email: String) -> Self {
        let now = Utc::now();
        Self { id: String::new(), name, email, created_at: now, updated_at: now }
    }

    pub fn update_profile(&mut self, name: String, email: String) {
        self.name = name;
        self.email = email;
        self.updated_at = Utc::now();
    }
}

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
