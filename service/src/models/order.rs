use serde::{Deserialize, Serialize};

use super::shared::Money;
use super::user::UserId;
use engine::{AppError, AppResult, Cast, ObjectId, UtcDateTime};

pub type OrderId = Cast<String, ObjectId>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderStatus {
    Created,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderItem {
    pub name: String,
    pub quantity: u32,
    pub unit_price: Money,
    pub line_total: Money,
}

impl OrderItem {
    pub fn new(name: String, quantity: u32, unit_price: Money) -> AppResult<Self> {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::invalid("items", "item name is required"));
        }
        if quantity == 0 {
            return Err(AppError::invalid("items", "item quantity must be at least 1"));
        }
        if !unit_price.is_positive() {
            return Err(AppError::invalid("items", "item unit_price must be positive"));
        }
        let line_total = unit_price.checked_mul(quantity)?;
        Ok(Self { name, quantity, unit_price, line_total })
    }
}

/// Modelo `Order`: `id` y `user_id` son `String` en Rust/JSON y se castean a `ObjectId` en MongoDB.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    pub id: OrderId,
    pub user_id: UserId,
    pub idempotency_key: String,
    pub items: Vec<OrderItem>,
    pub total: Money,
    pub status: OrderStatus,
    pub created_at: UtcDateTime,
}

impl Order {
    pub fn new(
        user_id: UserId,
        idempotency_key: String,
        items: Vec<OrderItem>,
        now: UtcDateTime,
    ) -> AppResult<Self> {
        let idempotency_key = idempotency_key.trim().to_string();
        if idempotency_key.is_empty() {
            return Err(AppError::invalid("idempotency_key", "is required"));
        }
        let total = Money::try_sum(items.iter().map(|item| item.line_total))?
            .ok_or_else(|| AppError::invalid("items", "an order needs at least one item"))?;

        Ok(Self {
            id: Cast::generate(),
            user_id,
            idempotency_key,
            items,
            total,
            status: OrderStatus::Created,
            created_at: now,
        })
    }

    pub fn is_same_request(&self, other: &Order) -> bool {
        self.user_id == other.user_id
            && self.idempotency_key == other.idempotency_key
            && self.items == other.items
    }
}

#[cfg(test)]
mod tests {
    use super::super::shared::Currency;
    use super::*;

    fn at(seconds: i64) -> UtcDateTime {
        UtcDateTime::from_timestamp_secs(seconds).expect("valid timestamp")
    }

    fn mxn(amount: i64) -> Money {
        Money::new(amount, Currency::Mxn)
    }

    #[test]
    fn validates_items_currency_and_idempotency_equality() {
        let item = OrderItem::new("Mouse".into(), 3, mxn(1999)).expect("item");
        assert_eq!(item.line_total, mxn(5997));

        let uid = Cast::generate();
        let a = Order::new(uid.clone(), "k1".into(), vec![item.clone()], at(1)).expect("order");
        let b = Order::new(uid, "k1".into(), vec![item], at(9)).expect("replay");
        assert!(a.is_same_request(&b));
        assert_eq!(a.total, mxn(5997));
    }
}
