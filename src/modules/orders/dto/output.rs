use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::super::entity::Order;
use super::items::OrderItemOutput;
use crate::shared::Money;

/// Contrato de salida con el detalle consolidado de una orden.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OrderOutput {
    pub id: String,
    pub user_id: String,
    pub items: Vec<OrderItemOutput>,
    pub total: Money,
    pub status: String,
    pub created_at: chrono::DateTime<Utc>,
}

/// El mapper vive en la frontera de transporte: `entity.rs` permanece puro y sin conocimiento
/// de los contratos HTTP de salida.
impl From<Order> for OrderOutput {
    fn from(order: Order) -> Self {
        Self {
            id: order.id().to_string(),
            user_id: order.user_id().to_string(),
            items: order
                .items()
                .iter()
                .map(|item| OrderItemOutput {
                    name: item.name().to_string(),
                    quantity: item.quantity(),
                    unit_price: item.unit_price(),
                    total: item.line_total(),
                })
                .collect(),
            total: order.total(),
            status: order.status().as_str().to_string(),
            created_at: order.created_at(),
        }
    }
}
