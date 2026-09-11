use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::entity::Order;

pub use crate::core::pagination::{Paginated, PaginatedResponse, PaginationQuery};

pub type PaginatedOrders = Paginated<OrderOutput>;

#[derive(Debug, Deserialize, Validate)]
pub struct CreateOrderInput {
    #[validate(length(min = 1, message = "user_id is required"))]
    pub user_id: String,

    #[serde(default)]
    pub user_name: Option<String>,

    #[validate(length(min = 1, message = "At least one item is required"), nested)]
    pub items: Vec<OrderItemInput>,
}

#[derive(Debug, Serialize, Deserialize, Validate, Clone)]
pub struct OrderItemInput {
    #[validate(length(min = 1, message = "Item name is required"))]
    pub name: String,

    #[validate(range(min = 1, message = "Quantity must be at least 1"))]
    pub quantity: u32,

    #[validate(range(min = 0.01, message = "Unit price must be positive"))]
    pub unit_price: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OrderItemOutput {
    pub name: String,
    pub quantity: u32,
    pub unit_price: f64,
    pub total: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OrderOutput {
    pub id: String,
    pub user_id: String,
    pub user_name: String,
    pub items: Vec<OrderItemOutput>,
    pub total_amount: f64,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

/// El mapper vive en la frontera de transporte: `entity.rs` permanece puro y sin conocimiento
/// de los contratos HTTP de salida.
impl From<Order> for OrderOutput {
    fn from(order: Order) -> Self {
        Self {
            id: order.id,
            user_id: order.user_id,
            user_name: order.user_name,
            items: order
                .items
                .into_iter()
                .map(|item| OrderItemOutput {
                    total: item.total(),
                    name: item.name,
                    quantity: item.quantity,
                    unit_price: item.unit_price,
                })
                .collect(),
            total_amount: order.total_amount,
            status: order.status,
            created_at: order.created_at,
        }
    }
}
