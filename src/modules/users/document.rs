use mongodb::bson;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

use super::entity::User;

/// Esquema de almacenamiento en MongoDB para `User`.
///
/// Es la representación de la colección, con los tipos del motor: `_id` es un `ObjectId` siempre
/// presente y las fechas son `bson::DateTime` (tipo Date nativo, ordenable y filtrable por rango),
/// no el string RFC 3339 en que serde convierte `chrono::DateTime`. La entidad expone lo mismo con
/// tipos de dominio (`String`, `chrono`). El borrado es lógico (`deleted_at`) y `version` sostiene
/// el bloqueo optimista de `update_one`. Privado al slice: solo `UserRepository` lo importa.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserDocument {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub name: String,
    pub email: String,
    pub version: u32,
    pub created_at: bson::DateTime,
    pub updated_at: bson::DateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<bson::DateTime>,
}

impl UserDocument {
    /// Documento listo para insertar bajo el `_id` que el repositorio acaba de generar.
    pub fn from_domain(id: ObjectId, user: &User) -> Self {
        Self {
            id,
            name: user.name().to_string(),
            email: user.email().to_string(),
            version: user.version(),
            created_at: bson::DateTime::from_chrono(user.created_at()),
            updated_at: bson::DateTime::from_chrono(user.updated_at()),
            deleted_at: None,
        }
    }

    pub fn into_domain(self) -> User {
        User::restore(
            self.id.to_hex(),
            self.name,
            self.email,
            self.version,
            self.created_at.to_chrono(),
            self.updated_at.to_chrono(),
        )
    }
}
