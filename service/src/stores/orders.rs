use engine::db::{
    BsonExt, Collection, Document, IndexModel, IndexOptions, decode_entity, doc, encode_entity,
    is_duplicate_key,
};
use engine::{AppError, AppResult, Mongo};

use super::helpers::primary_collection;
use crate::models::{Order, OrderId};

const COL: &str = "orders";

/// Resultado de escritura idempotente sobre `orders`.
#[derive(Debug, Clone)]
pub enum PlaceOutcome {
    Inserted,
    AlreadyPlaced(Order),
}

/// Almacén de `Order`: obtiene la conexión del singleton `Mongo` sin DI y encapsula
/// las fórmulas BSON e índices sobre `mongodb::Collection<Document>`.
#[derive(Clone)]
pub struct OrderStore {
    col: Collection<Document>,
}

impl OrderStore {
    pub fn new() -> AppResult<Self> {
        let db = Mongo::get(Mongo::DEFAULT)?;
        Ok(Self { col: primary_collection(&db, COL) })
    }

    #[tracing::instrument(skip_all)]
    pub async fn init_indexes(&self) -> AppResult<()> {
        let index = IndexModel::builder()
            .keys(doc! { "user_id": 1, "idempotency_key": 1 })
            .options(
                IndexOptions::builder()
                    .unique(true)
                    .name("orders_user_idempotency_unique_idx".to_string())
                    .build(),
            )
            .build();
        self.col.create_indexes(vec![index]).await?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn insert_idempotent(&self, order: &Order) -> AppResult<PlaceOutcome> {
        match self.col.insert_one(encode_entity(order)?).await {
            Ok(_) => Ok(PlaceOutcome::Inserted),
            Err(err) if is_duplicate_key(&err) => {
                let existing = self
                    .col
                    .find_one(doc! {
                        "user_id": order.user_id.to_bson()?,
                        "idempotency_key": &order.idempotency_key,
                    })
                    .await?
                    .ok_or_else(|| {
                        AppError::database(
                            "duplicate idempotency key reported but no order matches it",
                        )
                    })?;

                Ok(PlaceOutcome::AlreadyPlaced(decode_entity(existing)?))
            }
            Err(err) => Err(err.into()),
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn find_by_id(&self, id: &OrderId) -> AppResult<Option<Order>> {
        let doc = self.col.find_one(doc! { "_id": id.to_bson()? }).await?;
        doc.map(decode_entity).transpose()
    }
}
