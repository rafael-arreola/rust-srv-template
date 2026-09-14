use futures::stream::TryStreamExt;
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};

use super::document::UserDocument;
use super::entity::User;
use crate::core::db::query::QueryResult;
use crate::core::error::{AppError, AppResult};

#[derive(Clone)]
pub struct UserRepository {
    collection: Collection<UserDocument>,
}

impl UserRepository {
    pub async fn new(db: &Database) -> AppResult<Self> {
        let repo = Self { collection: db.collection::<UserDocument>("users") };
        repo.create_indexes().await?;
        Ok(repo)
    }

    async fn create_indexes(&self) -> AppResult<()> {
        let indexes = vec![
            IndexModel::builder()
                .keys(doc! { "email": 1 })
                .options(
                    IndexOptions::builder()
                        .unique(true)
                        .name("users_email_unique_idx".to_string())
                        .build(),
                )
                .build(),
            IndexModel::builder()
                .keys(doc! { "deleted_at": 1, "created_at": -1 })
                .options(
                    IndexOptions::builder().name("users_deleted_created_idx".to_string()).build(),
                )
                .build(),
        ];

        self.collection.create_indexes(indexes).await?;

        tracing::info!("✓ Users collection indexes verified");
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn create(&self, user: &User) -> AppResult<User> {
        let doc = UserDocument::from_domain(user, None);
        let result = self.collection.insert_one(&doc).await?;

        let mut created = user.clone();
        created.id = result.inserted_id.as_object_id().map(|oid| oid.to_hex()).unwrap_or_default();
        Ok(created)
    }

    #[tracing::instrument(skip_all)]
    pub async fn find_by_id(&self, id: &str) -> AppResult<Option<User>> {
        let oid = ObjectId::parse_str(id).map_err(|_| AppError::invalid_param("id", "User", id))?;

        let user = self
            .collection
            .find_one(doc! { "_id": oid, "deleted_at": { "$exists": false } })
            .await?;

        Ok(user.map(UserDocument::into_domain))
    }

    #[tracing::instrument(skip_all)]
    pub async fn find_by_email(&self, email: &str) -> AppResult<Option<User>> {
        let user = self
            .collection
            .find_one(doc! { "email": email, "deleted_at": { "$exists": false } })
            .await?;

        Ok(user.map(UserDocument::into_domain))
    }

    #[tracing::instrument(skip_all)]
    pub async fn list(&self, skip: u64, limit: u64) -> AppResult<QueryResult<User>> {
        let filter = doc! { "deleted_at": { "$exists": false } };

        let cursor = self
            .collection
            .find(filter.clone())
            .skip(skip)
            .limit(limit as i64)
            .sort(doc! { "created_at": -1 })
            .await?;

        let docs: Vec<UserDocument> = cursor.try_collect().await?;
        let items = docs.into_iter().map(UserDocument::into_domain).collect();

        let total = self.collection.count_documents(filter).await?;

        Ok(QueryResult::new(items, total))
    }

    #[tracing::instrument(skip_all)]
    pub async fn update(&self, id: &str, user: &User) -> AppResult<bool> {
        let oid = ObjectId::parse_str(id).map_err(|_| AppError::invalid_param("id", "User", id))?;

        let doc = UserDocument::from_domain(user, Some(oid));
        let bson_doc = mongodb::bson::serialize_to_document(&doc)
            .map_err(|e| AppError::internal(e.to_string()))?;

        let result = self
            .collection
            .update_one(
                doc! { "_id": oid, "deleted_at": { "$exists": false } },
                doc! { "$set": bson_doc },
            )
            .await?;

        Ok(result.matched_count > 0)
    }

    #[tracing::instrument(skip_all)]
    pub async fn delete(&self, id: &str) -> AppResult<bool> {
        let oid = ObjectId::parse_str(id).map_err(|_| AppError::invalid_param("id", "User", id))?;

        let now = mongodb::bson::DateTime::from_chrono(chrono::Utc::now());

        let result = self
            .collection
            .update_one(
                doc! { "_id": oid, "deleted_at": { "$exists": false } },
                doc! { "$set": { "deleted_at": now } },
            )
            .await?;

        Ok(result.matched_count > 0)
    }
}
