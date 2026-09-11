use chrono::{DateTime, Utc};

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
