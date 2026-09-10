use chrono::{DateTime, Utc};
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

use super::entity::{Order, OrderItem};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OrderItemDocument {
    pub name: String,
    pub quantity: u32,
    pub unit_price: f64,
}

impl OrderItemDocument {
    pub fn from_domain(item: &OrderItem) -> Self {
        Self { name: item.name.clone(), quantity: item.quantity, unit_price: item.unit_price }
    }

    pub fn into_domain(self) -> OrderItem {
        OrderItem { name: self.name, quantity: self.quantity, unit_price: self.unit_price }
    }
}

/// Representación interna en MongoDB BSON para la entidad `Order`.
/// Encapsula el esquema de almacenamiento, anotaciones serde BSON,
/// `_id` nativo de Mongo y borrado lógico (`deleted_at`).
/// Privado al slice de persistencia (consumido por `OrderRepository`).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OrderDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub user_id: String,
    pub user_name: String,
    pub items: Vec<OrderItemDocument>,
    pub total_amount: f64,
    pub status: String,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
}

impl OrderDocument {
    pub fn from_domain(order: &Order, oid: Option<ObjectId>) -> Self {
        Self {
            id: oid,
            user_id: order.user_id.clone(),
            user_name: order.user_name.clone(),
            items: order.items.iter().map(OrderItemDocument::from_domain).collect(),
            total_amount: order.total_amount,
            status: order.status.clone(),
            created_at: order.created_at,
            deleted_at: None,
        }
    }

    pub fn into_domain(self) -> Order {
        Order {
            id: self.id.map(|oid| oid.to_hex()).unwrap_or_default(),
            user_id: self.user_id,
            user_name: self.user_name,
            items: self.items.into_iter().map(OrderItemDocument::into_domain).collect(),
            total_amount: self.total_amount,
            status: self.status,
            created_at: self.created_at,
        }
    }
}
