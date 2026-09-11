use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};

use super::document::OrderDocument;
use super::entity::Order;
use crate::core::error::{AppError, AppResult};

#[derive(Clone)]
pub struct OrderRepository {
    collection: Collection<OrderDocument>,
}

impl OrderRepository {
    pub async fn new(db: &Database) -> AppResult<Self> {
        let repo = Self { collection: db.collection::<OrderDocument>("orders") };
        repo.create_indexes().await?;
        Ok(repo)
    }

    async fn create_indexes(&self) -> AppResult<()> {
        let indexes = vec![
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "created_at": -1 })
                .options(
                    IndexOptions::builder().name("orders_user_created_idx".to_string()).build(),
                )
                .build(),
        ];

        self.collection.create_indexes(indexes).await?;

        tracing::info!("✓ Orders collection indexes verified");
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn create(&self, order: &Order) -> AppResult<Order> {
        let doc = OrderDocument::from_domain(order, None);
        let result = self.collection.insert_one(&doc).await?;

        let mut created = order.clone();
        created.id = result.inserted_id.as_object_id().map(|oid| oid.to_hex()).unwrap_or_default();
        Ok(created)
    }

    #[tracing::instrument(skip_all)]
    pub async fn find_by_id(&self, id: &str) -> AppResult<Option<Order>> {
        let oid =
            ObjectId::parse_str(id).map_err(|_| AppError::invalid_param("id", "Order", id))?;

        let order = self
            .collection
            .find_one(doc! { "_id": oid, "deleted_at": { "$exists": false } })
            .await?;

        Ok(order.map(OrderDocument::into_domain))
    }
}
