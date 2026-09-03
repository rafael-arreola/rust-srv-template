use chrono::{DateTime, Utc};
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

use super::dto::{OrderItemOutput, OrderOutput};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OrderItemModel {
    pub name: String,
    pub quantity: u32,
    pub unit_price: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OrderModel {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub user_id: String,
    pub user_name: String,
    pub items: Vec<OrderItemModel>,
    pub total_amount: f64,
    pub status: String,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
}

impl OrderModel {
    pub fn to_output(self) -> OrderOutput {
        OrderOutput {
            id: self.id.map(|oid| oid.to_hex()).unwrap_or_default(),
            user_id: self.user_id,
            user_name: self.user_name,
            items: self
                .items
                .into_iter()
                .map(|item| OrderItemOutput {
                    name: item.name,
                    quantity: item.quantity,
                    unit_price: item.unit_price,
                    total: item.quantity as f64 * item.unit_price,
                })
                .collect(),
            total_amount: self.total_amount,
            status: self.status,
            created_at: self.created_at,
        }
    }
}
