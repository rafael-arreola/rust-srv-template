use engine::db::{
    BsonExt, Collection, Document, IndexModel, IndexOptions, QueryResult, ReturnDocument,
    decode_entity, doc, encode_entity, escape_regex,
};
use engine::{AppResult, Mongo, UtcDateTime};

use super::helpers::{paginate, primary_collection, soft_delete_by_id};
use crate::models::{User, UserId};

const COL: &str = "users";

/// Estructura auxiliar de criterios tipados para búsquedas cerradas dentro de [`UserStore`].
#[derive(Debug, Clone, Default)]
pub struct UserFilter {
    pub search: Option<String>,
}

/// Resultado tipado de actualización con control de concurrencia optimista.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOutcome<T> {
    Updated(T),
    Conflict,
    NotFound,
}

/// Almacén de `User`: obtiene la conexión del singleton `Mongo` sin DI y usa la API nativa
/// de `mongodb::Collection<Document>` encapsulando índices y consultas BSON.
#[derive(Clone)]
pub struct UserStore {
    col: Collection<Document>,
}

impl UserStore {
    pub fn new() -> AppResult<Self> {
        let db = Mongo::get(Mongo::DEFAULT)?;
        Ok(Self { col: primary_collection(&db, COL) })
    }

    #[tracing::instrument(skip_all)]
    pub async fn init_indexes(&self) -> AppResult<()> {
        let indexes = vec![
            IndexModel::builder()
                .keys(doc! { "email": 1, "deleted_at": 1 })
                .options(
                    IndexOptions::builder()
                        .unique(true)
                        .name("users_email_active_unique_idx".to_string())
                        .build(),
                )
                .build(),
            IndexModel::builder()
                .keys(doc! { "deleted_at": 1, "created_at": -1, "_id": -1 })
                .options(IndexOptions::builder().name("users_active_list_idx".to_string()).build())
                .build(),
        ];
        self.col.create_indexes(indexes).await?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn insert(&self, user: &User) -> AppResult<()> {
        self.col.insert_one(encode_entity(user)?).await?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn find_active(&self, id: &UserId) -> AppResult<Option<User>> {
        let doc = self.col.find_one(doc! { "_id": id.to_bson()?, "deleted_at": null }).await?;
        doc.map(decode_entity).transpose()
    }

    #[tracing::instrument(skip_all)]
    pub async fn email_exists(&self, email: &str) -> AppResult<bool> {
        let count =
            self.col.count_documents(doc! { "email": email, "deleted_at": null }).limit(1).await?;
        Ok(count > 0)
    }

    #[tracing::instrument(skip_all)]
    pub async fn list_active(
        &self,
        filter: &UserFilter,
        skip: u64,
        limit: u32,
    ) -> AppResult<QueryResult<User>> {
        let mut query = doc! { "deleted_at": null };
        if let Some(term) = filter.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            let escaped = escape_regex(term);
            query.insert(
                "$or",
                vec![
                    doc! { "name": { "$regex": &escaped, "$options": "i" } },
                    doc! { "email": { "$regex": &escaped, "$options": "i" } },
                ],
            );
        }

        paginate(&self.col, query, doc! { "created_at": -1, "_id": -1 }, skip, limit).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn update_versioned(&self, user: &User) -> AppResult<UpdateOutcome<User>> {
        let filter = doc! {
            "_id": user.id.to_bson()?,
            "version": user.version,
            "deleted_at": null,
        };
        let changes = doc! {
            "$set": {
                "name": &user.name,
                "email": &user.email,
                "updated_at": user.updated_at.to_bson(),
            },
            "$inc": { "version": 1 },
        };

        let updated = self
            .col
            .find_one_and_update(filter, changes)
            .return_document(ReturnDocument::After)
            .await?;

        if let Some(doc) = updated {
            return Ok(UpdateOutcome::Updated(decode_entity(doc)?));
        }

        match self.find_active(&user.id).await? {
            Some(_) => Ok(UpdateOutcome::Conflict),
            None => Ok(UpdateOutcome::NotFound),
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn soft_delete(&self, id: &UserId, now: UtcDateTime) -> AppResult<bool> {
        soft_delete_by_id(&self.col, id, now).await
    }
}
