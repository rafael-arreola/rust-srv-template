use engine::{AppError, AppResult, Context, UtcDateTime};

use crate::models::{Order, OrderId, OrderItem, UserId};
use crate::stores::{OrderStore, PlaceOutcome};

#[derive(Clone)]
pub struct OrderService {
    store: OrderStore,
}

impl OrderService {
    pub fn new(store: OrderStore) -> Self {
        Self { store }
    }

    #[tracing::instrument(skip_all, fields(%user_id))]
    pub async fn place(
        &self,
        _ctx: &Context,
        user_id: UserId,
        idempotency_key: String,
        items: Vec<OrderItem>,
    ) -> AppResult<Order> {
        let order = Order::new(user_id, idempotency_key, items, UtcDateTime::now())?;

        match self.store.insert_idempotent(&order).await? {
            PlaceOutcome::Inserted => {
                tracing::info!(order_id = %order.id, "Order created successfully");
                Ok(order)
            }
            PlaceOutcome::AlreadyPlaced(existing) if existing.is_same_request(&order) => {
                tracing::info!(order_id = %existing.id, "Idempotent replay: order already placed");
                Ok(existing)
            }
            PlaceOutcome::AlreadyPlaced(existing) => Err(AppError::already_exists(
                "Order",
                "idempotency_key was already used for a different purchase",
            )
            .with_internal(format!(
                "idempotency_key reused with different items; existing order {}",
                existing.id
            ))),
        }
    }

    #[tracing::instrument(skip_all, fields(%id))]
    pub async fn get_by_id(&self, _ctx: &Context, id: &OrderId) -> AppResult<Order> {
        self.store.find_by_id(id).await?.ok_or_else(|| AppError::not_found("Order", id.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use engine::{Cast, Mongo, MongoDb};

    use super::*;
    use crate::models::{Currency, Money, OrderStatus};

    #[tokio::test]
    #[ignore = "requires MONGO_TEST_URL: a disposable MongoDB"]
    async fn idempotent_replay_and_conflict_detection() {
        let db = MongoDb::connect_for_tests().await;

        Mongo::scope(Mongo::DEFAULT, db.clone(), async {
            let store = OrderStore::new().expect("order store");
            store.init_indexes().await.expect("indexes");
            let svc = OrderService::new(store);
            let ctx = Context::new();
            let uid = Cast::generate();
            let items = |qty| {
                vec![
                    OrderItem::new("Mouse".into(), qty, Money::new(1999, Currency::Mxn))
                        .expect("item"),
                ]
            };

            let first = svc.place(&ctx, uid.clone(), "k1".into(), items(2)).await.expect("created");
            let replay = svc.place(&ctx, uid.clone(), "k1".into(), items(2)).await.expect("replay");
            assert_eq!(replay.id, first.id);
            assert_eq!(
                svc.get_by_id(&ctx, &first.id).await.expect("found").status,
                OrderStatus::Created
            );

            let err = svc.place(&ctx, uid, "k1".into(), items(5)).await.expect_err("conflict");
            assert_eq!(err.code(), "ALREADY_EXISTS");
        })
        .await;

        db.drop_for_tests().await;
    }
}
