use chrono::{DateTime, Utc};
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

use super::dto::UserOutput;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserModel {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub name: String,
    pub email: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
}

impl UserModel {
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}

impl From<UserModel> for UserOutput {
    fn from(model: UserModel) -> Self {
        Self {
            id: model.id.map(|oid| oid.to_hex()).unwrap_or_default(),
            name: model.name,
            email: model.email,
            created_at: model.created_at,
            updated_at: model.updated_at,
        }
    }
}
