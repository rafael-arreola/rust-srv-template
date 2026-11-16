use engine::{AppError, AppResult, Context, StatusCode, TraceExt};

use crate::clients::CarrierClient;
use crate::models::{Order, OrderItem, UserId};
use crate::services::orders::OrderService;
use crate::services::users::UserService;

/// Estructura auxiliar de entrada para orquestar el flujo de checkout.
#[derive(Debug, Clone)]
pub struct PlaceCheckout {
    pub user_id: UserId,
    pub idempotency_key: String,
    pub items: Vec<OrderItem>,
}

/// Servicio agregador: recibe otros servicios y clientes externos por inyección de dependencias.
#[derive(Clone)]
pub struct CheckoutService {
    users: UserService,
    orders: OrderService,
    carrier: Option<CarrierClient>,
}

impl CheckoutService {
    pub fn new(users: UserService, orders: OrderService, carrier: Option<CarrierClient>) -> Self {
        Self { users, orders, carrier }
    }

    #[tracing::instrument(skip_all, fields(user_id = %cmd.user_id))]
    pub async fn execute(&self, ctx: &Context, cmd: PlaceCheckout) -> AppResult<Order> {
        let user = self
            .users
            .find_active(ctx, &cmd.user_id)
            .await
            .map_err(|err| {
                if err.status_code() == StatusCode::NOT_FOUND {
                    AppError::business_rule("user_id does not reference an active user")
                        .with_internal(err.internal_detail())
                } else {
                    err
                }
            })
            .trace("Error verificando usuario para checkout")?;

        let order = self
            .orders
            .place(ctx, user.id, cmd.idempotency_key, cmd.items)
            .await
            .trace("Fallo creando la orden durante el proceso de checkout")?;

        tracing::info!(
            order_id = %order.id,
            user_id = %order.user_id,
            total_minor_units = order.total.amount(),
            currency = %order.total.currency(),
            carrier_configured = self.carrier.is_some(),
            "Checkout completed successfully"
        );

        Ok(order)
    }
}

#[cfg(test)]
mod tests {
    use engine::{Cast, Mongo, MongoDb, StatusCode};

    use super::*;
    use crate::models::{Currency, Money, OrderStatus};
    use crate::stores::{OrderStore, UserStore};

    #[tokio::test]
    #[ignore = "requires MONGO_TEST_URL: a disposable MongoDB"]
    async fn places_order_and_rejects_unknown_user() {
        let db = MongoDb::connect_for_tests().await;

        Mongo::scope(Mongo::DEFAULT, db.clone(), async {
            let user_store = UserStore::new().expect("user store");
            let order_store = OrderStore::new().expect("order store");
            user_store.init_indexes().await.expect("user indexes");
            order_store.init_indexes().await.expect("order indexes");

            let users = UserService::new(user_store);
            let orders = OrderService::new(order_store);
            let checkout = CheckoutService::new(users.clone(), orders, None);

            let ctx = Context::new();
            let user =
                users.create(&ctx, "Ana".into(), "ana@example.com".into()).await.expect("user");

            let items = || {
                vec![
                    OrderItem::new("Mouse".into(), 2, Money::new(1999, Currency::Mxn))
                        .expect("item"),
                ]
            };

            let placed = checkout
                .execute(
                    &ctx,
                    PlaceCheckout {
                        user_id: user.id.clone(),
                        idempotency_key: "k1".into(),
                        items: items(),
                    },
                )
                .await
                .expect("placed");
            assert_eq!((&placed.user_id, placed.status), (&user.id, OrderStatus::Created));

            let err = checkout
                .execute(
                    &ctx,
                    PlaceCheckout {
                        user_id: Cast::generate(),
                        idempotency_key: "k1".into(),
                        items: items(),
                    },
                )
                .await
                .expect_err("unknown user");
            assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        })
        .await;

        db.drop_for_tests().await;
    }
}
