use std::sync::Arc;

use super::dto::{CheckoutInput, CheckoutOutput};
use crate::core::error::{AppResult, ContextExt};
use crate::modules::orders::dto::CreateOrderInput;
use crate::modules::orders::service::OrderService;
use crate::modules::users::service::UserService;

/// Orquestador del flujo de negocio de checkout (Composite / Workflow Slice).
/// Coordina la validación del usuario y la persistencia de la orden sin acoplar
/// directamente los módulos base entre sí.
#[derive(Clone)]
pub struct CheckoutWorkflow {
    user_service: Arc<UserService>,
    order_service: Arc<OrderService>,
}

impl CheckoutWorkflow {
    pub fn new(user_service: Arc<UserService>, order_service: Arc<OrderService>) -> Self {
        Self { user_service, order_service }
    }

    #[tracing::instrument(skip_all, fields(user_id = %input.user_id))]
    pub async fn execute(&self, input: CheckoutInput) -> AppResult<CheckoutOutput> {
        // 1. Validar precondición en el módulo base Users
        let user = self
            .user_service
            .get_user(&input.user_id)
            .await
            .attach_context("Error verificando usuario para checkout")
            .attach_public("No fue posible validar la cuenta de usuario para el checkout")?;

        let items_count = input.items.len();

        // 2. Coordinar creación de la orden en el módulo base Orders
        let order_input = CreateOrderInput {
            user_id: user.id.clone(),
            user_name: Some(user.name.clone()),
            items: input.items,
        };

        let order = self
            .order_service
            .create_order(order_input)
            .await
            .attach_context("Fallo creando la orden durante el proceso de checkout")?;

        tracing::info!(
            order_id = %order.id,
            user_id = %user.id,
            total_amount = %order.total_amount,
            "Checkout workflow completed successfully"
        );

        // 3. Ensamblar contrato consolidado del flujo de negocio
        Ok(CheckoutOutput {
            order_id: order.id,
            user_id: user.id,
            user_name: user.name,
            user_email: user.email,
            total_amount: order.total_amount,
            items_count,
            status: "COMPLETED".to_string(),
        })
    }
}
