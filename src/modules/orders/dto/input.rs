use serde::Deserialize;
use validator::Validate;

use super::items::OrderItemInput;

/// Contrato de entrada para registrar una orden. Lo recibe `contract::OrderPlacement` desde un
/// orquestador que ya verificó al usuario; `user_id` es el identificador que `users` devolvió e
/// `idempotency_key` la clave que el cliente eligió para esa compra.
#[derive(Debug, Deserialize, Validate)]
pub struct CreateOrderInput {
    #[validate(length(min = 1, message = "user_id is required"))]
    pub user_id: String,

    #[validate(length(min = 1, max = 128, message = "idempotency_key must be 1-128 characters"))]
    pub idempotency_key: String,

    #[validate(length(min = 1, message = "At least one item is required"), nested)]
    pub items: Vec<OrderItemInput>,
}
