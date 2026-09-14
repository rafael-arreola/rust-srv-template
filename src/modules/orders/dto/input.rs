use serde::Deserialize;
use validator::Validate;

use super::items::OrderItemInput;

/// Contrato de entrada para la creación de una orden de compra.
#[derive(Debug, Deserialize, Validate)]
pub struct CreateOrderInput {
    #[validate(length(min = 1, message = "user_id is required"))]
    pub user_id: String,

    #[serde(default)]
    pub user_name: Option<String>,

    #[validate(length(min = 1, message = "At least one item is required"), nested)]
    pub items: Vec<OrderItemInput>,
}
