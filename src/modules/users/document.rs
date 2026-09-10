use chrono::{DateTime, Utc};
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

use super::entity::User;

/// Representación interna en MongoDB BSON para la entidad `User`.
/// Encapsula el esquema de almacenamiento, anotaciones serde BSON,
/// `_id` nativo de Mongo y borrado lógico (`deleted_at`).
/// Este tipo es privado al slice de persistencia y solo interactúa con `UserRepository`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub name: String,
    pub email: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
}

impl UserDocument {
    pub fn from_domain(user: &User, oid: Option<ObjectId>) -> Self {
        Self {
            id: oid,
            name: user.name.clone(),
            email: user.email.clone(),
            created_at: user.created_at,
            updated_at: user.updated_at,
            deleted_at: None,
        }
    }

    pub fn into_domain(self) -> User {
        User {
            id: self.id.map(|oid| oid.to_hex()).unwrap_or_default(),
            name: self.name,
            email: self.email,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
