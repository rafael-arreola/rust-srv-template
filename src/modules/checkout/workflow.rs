use super::dto::{CheckoutInput, CheckoutOutput};
use crate::core::context::Context;
use crate::core::error::{AppError, AppResult, ContextExt, ErrorKind};
use crate::modules::orders::contract::OrderPlacement;
use crate::modules::orders::dto::{CreateOrderInput, OrderItemInput};
use crate::modules::users::contract::UserLookup;

/// Orquestador del flujo de checkout (Composite / Workflow Slice).
///
/// Recibe los contratos estrechos de cada módulo base, no sus services completos: puede leer
/// usuarios y registrar órdenes, y nada más. Es el único poseedor de `OrderPlacement`, así que la
/// verificación del usuario en el paso 1 es la única forma de llegar a una orden.
#[derive(Clone)]
pub struct CheckoutWorkflow {
    users: UserLookup,
    orders: OrderPlacement,
}

impl CheckoutWorkflow {
    pub fn new(users: UserLookup, orders: OrderPlacement) -> Self {
        Self { users, orders }
    }

    #[tracing::instrument(skip_all, fields(user_id = %input.user_id))]
    pub async fn execute(&self, ctx: &Context, input: CheckoutInput) -> AppResult<CheckoutOutput> {
        // 1. Precondición en el módulo base Users: el usuario existe y está activo. Para quien
        //    llama a `POST /checkout` un usuario inexistente es un body inválido (422 sobre
        //    `user_id`), no un recurso de la URL ausente (404).
        let user = self
            .users
            .find(ctx, &input.user_id)
            .await
            .map_err(|err| match err.kind() {
                ErrorKind::NotFound { .. } => {
                    AppError::business_rule("user_id does not reference an active user")
                        .with_internal(err.internal_detail())
                }
                _ => err,
            })
            .attach_context("Error verificando usuario para checkout")?;

        let items_count = input.items.len();

        // 2. Registro de la orden a través de la única puerta de escritura de Orders.
        let order_input = CreateOrderInput {
            user_id: user.id.clone(),
            idempotency_key: input.idempotency_key,
            items: input
                .items
                .into_iter()
                .map(|item| OrderItemInput {
                    name: item.name,
                    quantity: item.quantity,
                    unit_price: item.unit_price,
                })
                .collect(),
        };

        let order = self
            .orders
            .place(ctx, order_input)
            .await
            .attach_context("Fallo creando la orden durante el proceso de checkout")?;

        tracing::info!(
            order_id = %order.id,
            user_id = %user.id,
            total_minor_units = order.total.amount(),
            currency = %order.total.currency(),
            "Checkout workflow completed successfully"
        );

        // 3. Contrato consolidado: identificadores y el hecho tal como lo registró Orders.
        Ok(CheckoutOutput {
            order_id: order.id,
            user_id: user.id,
            total: order.total,
            items_count,
            status: order.status,
        })
    }
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use super::*;
    use crate::core::db::Store;
    use crate::modules::checkout::dto::CheckoutItemInput;
    use crate::modules::orders::OrdersModule;
    use crate::modules::users::UsersModule;
    use crate::modules::users::dto::CreateUserInput;
    use crate::shared::{Currency, Money};

    fn checkout(user_id: &str) -> CheckoutInput {
        CheckoutInput {
            user_id: user_id.into(),
            idempotency_key: "k1".into(),
            items: vec![CheckoutItemInput {
                name: "Mouse".into(),
                quantity: 2,
                unit_price: Money::new(1999, Currency::Mxn),
            }],
        }
    }

    #[tokio::test]
    #[ignore = "requires MONGO_TEST_URL: a disposable MongoDB"]
    async fn places_the_order_of_an_existing_user_once() {
        let store = Store::connect_for_tests().await;
        let users = UsersModule::new(&store).await.expect("users");
        let orders = OrdersModule::new(&store).await.expect("orders");
        let workflow = CheckoutWorkflow::new(users.lookup(), orders.placement());
        let ctx = Context::new();
        let user = users
            .service
            .create_user(
                &ctx,
                CreateUserInput { name: "Ana".into(), email: "ana@example.com".into() },
            )
            .await
            .expect("user");

        let placed = workflow.execute(&ctx, checkout(&user.id)).await.expect("placed");
        assert_eq!((placed.user_id.as_str(), placed.items_count), (user.id.as_str(), 1));
        assert_eq!(placed.total, Money::new(3998, Currency::Mxn));

        let retried = workflow.execute(&ctx, checkout(&user.id)).await.expect("retried");
        assert_eq!(retried.order_id, placed.order_id);

        store.drop_for_tests().await;
    }

    #[tokio::test]
    #[ignore = "requires MONGO_TEST_URL: a disposable MongoDB"]
    async fn an_unknown_user_is_a_business_rule_violation() {
        let store = Store::connect_for_tests().await;
        let users = UsersModule::new(&store).await.expect("users");
        let orders = OrdersModule::new(&store).await.expect("orders");
        let workflow = CheckoutWorkflow::new(users.lookup(), orders.placement());

        let err = workflow
            .execute(&Context::new(), checkout("65f0a1b2c3d4e5f60718293a"))
            .await
            .expect_err("unknown user");
        assert_eq!(err.code(), "BUSINESS_RULE_VIOLATION");
        assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

        store.drop_for_tests().await;
    }
}
