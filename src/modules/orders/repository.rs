use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};

use super::model::OrderModel;
use crate::core::error::{AppError, AppResult};

#[derive(Clone)]
pub struct OrderRepository {
    collection: Collection<OrderModel>,
}

impl OrderRepository {
    pub async fn new(db: &Database) -> AppResult<Self> {
        let repo = Self { collection: db.collection::<OrderModel>("orders") };
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

        self.collection
            .create_indexes(indexes)
            .await
            .map_err(|e| AppError::database(e.to_string()))?;

        tracing::info!("✓ Orders collection indexes verified");
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn create(&self, mut order: OrderModel) -> AppResult<OrderModel> {
        let result = self
            .collection
            .insert_one(&order)
            .await
            .map_err(|e| AppError::database(e.to_string()))?;

        order.id = result.inserted_id.as_object_id();
        Ok(order)
    }

    #[tracing::instrument(skip_all)]
    pub async fn find_by_id(&self, id: &str) -> AppResult<Option<OrderModel>> {
        let oid =
            ObjectId::parse_str(id).map_err(|_| AppError::invalid_param("id", "Order", id))?;

        let order = self
            .collection
            .find_one(doc! { "_id": oid, "deleted_at": { "$exists": false } })
            .await
            .map_err(|e| AppError::database(e.to_string()))?;

        Ok(order)
    }
}
