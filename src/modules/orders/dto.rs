use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use validator::Validate;

pub use crate::core::pagination::{Paginated, PaginatedResponse, PaginationQuery};

pub type PaginatedOrders = Paginated<OrderOutput>;

#[derive(Debug, Deserialize, Validate)]
pub struct CreateOrderInput {
    #[validate(length(min = 1, message = "user_id is required"))]
    pub user_id: String,

    #[serde(default)]
    pub user_name: Option<String>,

    #[validate(length(min = 1, message = "At least one item is required"))]
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
