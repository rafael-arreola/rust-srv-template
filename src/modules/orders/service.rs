use std::sync::Arc;

use chrono::Utc;

use super::dto::{CreateOrderInput, OrderOutput};
use super::entity::{Order, OrderItem};
use super::repository::{OrderRepository, PlaceOutcome};
use crate::core::context::Context;
use crate::core::error::{AppError, AppResult};

#[derive(Clone)]
pub struct OrderService {
    repo: Arc<OrderRepository>,
}

impl OrderService {
    pub fn new(repo: Arc<OrderRepository>) -> Self {
        Self { repo }
    }

    /// Visible solo dentro de `orders`: la única puerta de escritura hacia afuera es
    /// `contract::OrderPlacement`, que el composition root entrega al orquestador que ya verificó
    /// al usuario. Ningún controller ni módulo ajeno puede crear órdenes por otro camino.
    #[tracing::instrument(skip_all, fields(user_id = %input.user_id))]
    pub(super) async fn create_order(
        &self,
        _ctx: &Context,
        input: CreateOrderInput,
    ) -> AppResult<OrderOutput> {
        let items = input
            .items
            .into_iter()
            .map(|item| OrderItem::new(item.name, item.quantity, item.unit_price))
            .collect::<AppResult<Vec<_>>>()?;

        let order = Order::new(input.user_id, input.idempotency_key, items, Utc::now())?;

        match self.repo.place(&order).await? {
            PlaceOutcome::Inserted(created) => {
                tracing::info!(order_id = %created.id(), "Order created successfully");
                Ok(created.into())
            }
            PlaceOutcome::AlreadyPlaced(existing) if existing.is_same_request(&order) => {
                tracing::info!(order_id = %existing.id(), "Idempotent replay: order already placed");
                Ok(existing.into())
            }
            PlaceOutcome::AlreadyPlaced(existing) => Err(AppError::already_exists(
                "Order",
                "idempotency_key was already used for a different purchase",
            )
            .with_internal(format!(
                "idempotency_key reused with different items; existing order {}",
                existing.id()
            ))),
        }
    }

    #[tracing::instrument(skip_all, fields(%id))]
    pub async fn get_order(&self, _ctx: &Context, id: &str) -> AppResult<OrderOutput> {
        let order =
            self.repo.find_by_id(id).await?.ok_or_else(|| AppError::not_found("Order", id))?;

        Ok(order.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::db::Store;
    use crate::modules::orders::dto::OrderItemInput;
    use crate::shared::{Currency, Money};

    /// `_id` de usuario con la forma que `users` emite: 24 dígitos hexadecimales.
    const USER_ID: &str = "65f0a1b2c3d4e5f60718293a";

    async fn service(store: &Store) -> OrderService {
        OrderService::new(Arc::new(OrderRepository::new(store).await.expect("repository")))
    }

    fn input(user_id: &str, key: &str, quantity: u32) -> CreateOrderInput {
        CreateOrderInput {
            user_id: user_id.into(),
            idempotency_key: key.into(),
            items: vec![OrderItemInput {
                name: "Mouse".into(),
                quantity,
                unit_price: Money::new(1999, Currency::Mxn),
            }],
        }
    }

    #[tokio::test]
    #[ignore = "requires MONGO_TEST_URL: a disposable MongoDB"]
    async fn a_replay_with_the_same_key_returns_the_recorded_order() {
        let store = Store::connect_for_tests().await;
        let orders = service(&store).await;
        let ctx = Context::new();

        let first = orders.create_order(&ctx, input(USER_ID, "k1", 2)).await.expect("created");
        let replay = orders.create_order(&ctx, input(USER_ID, "k1", 2)).await.expect("replayed");
        assert_eq!(replay.id, first.id);
        assert_eq!(first.total, Money::new(3998, Currency::Mxn));
        assert_eq!(orders.get_order(&ctx, &first.id).await.expect("found").status, "CREATED");

        store.drop_for_tests().await;
    }

    #[tokio::test]
    #[ignore = "requires MONGO_TEST_URL: a disposable MongoDB"]
    async fn the_same_key_for_a_different_purchase_is_a_conflict() {
        let store = Store::connect_for_tests().await;
        let orders = service(&store).await;
        let ctx = Context::new();

        orders.create_order(&ctx, input(USER_ID, "k1", 1)).await.expect("created");
        let err = orders.create_order(&ctx, input(USER_ID, "k1", 5)).await.expect_err("reused key");
        assert_eq!(err.code(), "ALREADY_EXISTS");

        store.drop_for_tests().await;
    }

    #[tokio::test]
    #[ignore = "requires MONGO_TEST_URL: a disposable MongoDB"]
    async fn a_user_id_that_is_not_an_object_id_is_rejected() {
        let store = Store::connect_for_tests().await;
        let orders = service(&store).await;

        let err = orders
            .create_order(&Context::new(), input("not-an-id", "k1", 1))
            .await
            .expect_err("invalid user id");
        assert_eq!(err.code(), "INVALID_INPUT");

        store.drop_for_tests().await;
    }
}
