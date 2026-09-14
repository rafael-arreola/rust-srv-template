use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::super::entity::Order;
use super::items::OrderItemOutput;
use crate::core::pagination::Paginated;

pub type PaginatedOrders = Paginated<OrderOutput>;

/// Contrato de salida con el detalle consolidado de una orden.
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
