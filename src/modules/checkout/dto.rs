use serde::{Deserialize, Serialize};
use validator::Validate;

pub use crate::modules::orders::dto::OrderItemInput;

#[derive(Debug, Deserialize, Validate)]
pub struct CheckoutInput {
    #[validate(length(min = 1, message = "user_id is required"))]
    pub user_id: String,

    #[validate(length(min = 1, message = "At least one item is required"), nested)]
    pub items: Vec<OrderItemInput>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CheckoutOutput {
    pub order_id: String,
    pub user_id: String,
    pub user_name: String,
    pub user_email: String,
    pub total_amount: f64,
    pub items_count: usize,
    pub status: String,
}
