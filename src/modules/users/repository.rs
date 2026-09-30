use chrono::Utc;
use futures::stream::TryStreamExt;
use mongodb::bson;
use mongodb::bson::oid::ObjectId;
use mongodb::{Collection, IndexModel, bson::doc, options::IndexOptions};

use super::document::UserDocument;
use super::entity::User;
use crate::core::db::{QueryResult, Store};
use crate::core::error::AppResult;

/// Resultado de una actualización condicionada por versión.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateOutcome {
    Updated,
    /// El usuario existe pero su `version` ya no es la leída: otra petición escribió antes.
    Conflict,
    NotFound,
}

#[derive(Clone)]
pub struct UserRepository {
    collection: Collection<UserDocument>,
}

impl UserRepository {
    pub async fn new(store: &Store) -> AppResult<Self> {
        let repo = Self { collection: store.mongo().collection::<UserDocument>("users") };
        repo.create_indexes().await?;
        Ok(repo)
    }

    /// Índices propios del slice, creados de forma idempotente.
    ///
    /// La unicidad del email es entre usuarios **activos**: el índice único es compuesto con
    /// `deleted_at`, de modo que dos documentos activos (ambos sin el campo, indexado como `null`)
    /// colisionan y uno borrado (con fecha) no bloquea el alta de ese email otra vez.
    /// `partialFilterExpression` no admite `$exists: false`, por eso se usa esta forma.
    async fn create_indexes(&self) -> AppResult<()> {
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

        self.collection.create_indexes(indexes).await?;

        tracing::info!("Users collection indexes verified");
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn create(&self, user: &User) -> AppResult<User> {
        let id = ObjectId::new();
        self.collection.insert_one(UserDocument::from_domain(id, user)).await?;

        Ok(user.clone().with_id(id.to_hex()))
    }

    /// Un identificador sin forma de `ObjectId` no puede existir en la colección: es `None`, no
    /// un 400, para que el formato de almacenamiento no asome en el contrato HTTP.
    #[tracing::instrument(skip_all)]
    pub async fn find_by_id(&self, id: &str) -> AppResult<Option<User>> {
        let Ok(oid) = ObjectId::parse_str(id) else {
            return Ok(None);
        };

        let filter = doc! { "_id": oid, "deleted_at": { "$exists": false } };

        let document = self.collection.find_one(filter).await?;

        Ok(document.map(UserDocument::into_domain))
    }

    #[tracing::instrument(skip_all)]
    pub async fn find_by_email(&self, email: &str) -> AppResult<Option<User>> {
        let filter = doc! { "email": email, "deleted_at": { "$exists": false } };

        let document = self.collection.find_one(filter).await?;

        Ok(document.map(UserDocument::into_domain))
    }

    /// Lista activos por fecha de creación descendente con `_id` como desempate, para que dos
    /// usuarios creados en el mismo milisegundo no cambien de página entre peticiones.
    #[tracing::instrument(skip_all)]
    pub async fn list(&self, skip: u64, limit: u32) -> AppResult<QueryResult<User>> {
        let filter = doc! { "deleted_at": { "$exists": false } };

        let cursor = self
            .collection
            .find(filter.clone())
            .skip(skip)
            .limit(i64::from(limit))
            .sort(doc! { "created_at": -1, "_id": -1 })
            .await?;

        let documents: Vec<UserDocument> = cursor.try_collect().await?;
        let items = documents.into_iter().map(UserDocument::into_domain).collect();

        let total = self.collection.count_documents(filter).await?;

        Ok(QueryResult::new(items, total))
    }

    /// Escritura condicionada a la `version` leída (bloqueo optimista): solo se tocan los campos
    /// del perfil y la versión avanza en la misma operación atómica. Si nada coincide, se
    /// distingue "borrado o inexistente" de "modificado por otra petición".
    #[tracing::instrument(skip_all)]
    pub async fn update(&self, user: &User) -> AppResult<UpdateOutcome> {
        let Ok(oid) = ObjectId::parse_str(user.id()) else {
            return Ok(UpdateOutcome::NotFound);
        };

        let filter = doc! {
            "_id": oid,
            "version": user.version(),
            "deleted_at": { "$exists": false },
        };
        let changes = doc! {
            "$set": {
                "name": user.name(),
                "email": user.email(),
                "updated_at": bson::DateTime::from_chrono(user.updated_at()),
            },
            "$inc": { "version": 1 },
        };

        let result = self.collection.update_one(filter, changes).await?;

        if result.matched_count > 0 {
            return Ok(UpdateOutcome::Updated);
        }

        match self.find_by_id(user.id()).await? {
            Some(_) => Ok(UpdateOutcome::Conflict),
            None => Ok(UpdateOutcome::NotFound),
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn soft_delete(
        &self,
        id: &str,
        deleted_at: chrono::DateTime<Utc>,
    ) -> AppResult<bool> {
        let Ok(oid) = ObjectId::parse_str(id) else {
            return Ok(false);
        };

        let filter = doc! { "_id": oid, "deleted_at": { "$exists": false } };
        let changes = doc! { "$set": { "deleted_at": bson::DateTime::from_chrono(deleted_at) } };

        let result = self.collection.update_one(filter, changes).await?;

        Ok(result.matched_count > 0)
    }
}
